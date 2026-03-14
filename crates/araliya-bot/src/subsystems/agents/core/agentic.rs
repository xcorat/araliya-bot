//! Shared agentic loop — session lifecycle + instruction pass + tool execution + response pass.
//!
//! [`AgenticLoop`] is the composable building block for multi-pass agent plugins.
//! Plugins create an `AgenticLoop`, register any in-process [`LocalTool`]s, and call
//! [`AgenticLoop::run`] (buffered) or [`AgenticLoop::run_stream`] (SSE-streaming)
//! from within `tokio::spawn`.
//!
//! ## Local tools vs bus tools
//!
//! - **Local tools** implement [`LocalTool`] and run inside `tokio::task::spawn_blocking`.
//!   Use them for in-process blocking I/O (e.g. docstore RAG).
//! - **Bus tools** are dispatched through `state.execute_tool()` after all local tools
//!   have been checked.

use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::warn;

use crate::error::AppError;
use crate::llm::StreamChunk;
use crate::subsystems::memory::handle::SessionHandle;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, StreamReceiver};

use super::prompt::{PromptBuilder, preamble};
use crate::subsystems::agents::AgentsState;

/// How many recent transcript entries to inject as conversation context.
const CONTEXT_WINDOW: usize = 20;

// ── LocalTool ─────────────────────────────────────────────────────────────────

/// An in-process tool that runs synchronously inside `tokio::task::spawn_blocking`.
///
/// Implement this for blocking context-retrieval operations (e.g. docstore BM25
/// or KG search) so they integrate with the agentic loop without going through
/// the bus.
pub(crate) trait LocalTool: Send + Sync + 'static {
    /// Tool identifier used in the JSON tool-call schema (e.g. `"docs_search"`).
    fn name(&self) -> &str;

    /// One-line description injected into the instruction-pass prompt.
    ///
    /// Include the action name and params schema so the LLM knows how to call
    /// it.  Example:
    /// ```text
    /// action: "search", params: {"query": "<search terms>"}
    ///   Description: Searches the documentation and returns relevant passages.
    /// ```
    fn description(&self) -> &str;

    /// Execute the tool synchronously.  May do blocking I/O; called from
    /// `tokio::task::spawn_blocking`.
    fn call(&self, params: &serde_json::Value) -> Result<String, String>;
}

// ── Internal tool-call schema ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ToolCall {
    tool: String,
    action: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// Parsed result from the instruction pass.
struct InstructionResponse {
    tool_calls: Vec<ToolCall>,
    /// Direct reply from the instruction pass — skip the response-pass LLM call
    /// when this is `Some` and `tool_calls` is empty.
    reply: Option<String>,
}

// ── PreparedTurn ──────────────────────────────────────────────────────────────

/// Intermediate state after running steps 1–5 of the agentic loop
/// (session, history, instruction pass, tool execution).
///
/// Both [`AgenticLoop::run`] and [`AgenticLoop::run_stream`] use this to
/// avoid duplicating the instruction + tool pipeline.
struct PreparedTurn {
    handle: SessionHandle,
    channel_id: String,
    /// Assembled system preamble for the response pass.
    system: String,
    /// Fully-rendered response-pass prompt (context + history + user input).
    response_prompt: String,
    debug_n: usize,
}

/// Result of [`AgenticLoop::prepare_turn`].
///
/// `EarlyReply` means the instruction pass returned a direct answer with no
/// tool calls — the caller can short-circuit instead of entering the response
/// pass.
enum TurnOutcome {
    Ready(PreparedTurn),
    EarlyReply(BusResult),
}

// ── AgenticLoop ───────────────────────────────────────────────────────────────

/// Composable agentic loop shared by multiple agent plugins.
///
/// Encapsulates the full per-request lifecycle:
///
/// 1. Session management (load or create).
/// 2. Instruction pass (LLM → JSON tool calls).
/// 3. Tool execution (local tools first, then bus dispatch).
/// 4. Response pass (main LLM with context + history).
/// 5. Transcript and spend persistence.
pub(crate) struct AgenticLoop {
    agent_id: String,
    use_instruction_llm: bool,
    instruct_prompt_file: String,
    context_prompt_file: String,
    /// Action tools: external capabilities (e.g. gmail, news).
    /// Shown under `Available tools:` in the instruction-pass prompt.
    local_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
    /// Memory tools: agent knowledge stores (e.g. docs_search).
    /// Shown under `Available memory:` in the instruction-pass prompt.
    /// Dispatched via the same `LocalTool::call()` mechanism as `local_tools`.
    memory_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
    /// Bus tools this agent is allowed to invoke (from config `skills`).
    /// Only these appear in the instruction-pass tool manifest.
    allowed_tools: Vec<String>,
    prompts_dir: String,
    /// When `true`, each turn writes intermediate data to the session KV store
    /// under `debug:turn:{n}:*` keys.  Writes are fire-and-forget.
    debug_logging: bool,
}

impl AgenticLoop {
    pub(crate) fn new(
        agent_id: impl Into<String>,
        use_instruction_llm: bool,
        instruct_prompt_file: impl Into<String>,
        context_prompt_file: impl Into<String>,
        local_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
        memory_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
        allowed_tools: Vec<String>,
        prompts_dir: impl Into<String>,
        debug_logging: bool,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            use_instruction_llm,
            instruct_prompt_file: instruct_prompt_file.into(),
            context_prompt_file: context_prompt_file.into(),
            local_tools,
            memory_tools,
            allowed_tools,
            prompts_dir: prompts_dir.into(),
            debug_logging,
        }
    }

    // ── Public entry points ──────────────────────────────────────────────

    /// Run the full agentic loop for a single request (buffered response).
    ///
    /// Call this from inside `tokio::spawn(async move { ... })` in the
    /// agent's `handle` method.
    pub(crate) async fn run(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        state: Arc<AgentsState>,
    ) -> BusResult {
        let turn = match self
            .prepare_turn(channel_id, content.clone(), session_id, &state)
            .await
        {
            TurnOutcome::EarlyReply(result) => return result,
            TurnOutcome::Ready(t) => t,
        };

        // ── Response pass (buffered) ────────────────────────────────
        let result = state
            .complete_via_llm_with_system(
                &turn.channel_id,
                &turn.response_prompt,
                Some(&turn.system),
            )
            .await;

        // ── Persist reply + spend ───────────────────────────────────
        if let Ok(BusPayload::CommsMessage {
            content: ref reply,
            ref usage,
            ..
        }) = result
        {
            if let Err(e) = turn.handle.transcript_append("assistant", reply).await {
                warn!(
                    "{}: transcript_append(assistant) failed: {e}",
                    self.agent_id
                );
            }
            if let Some(u) = usage
                && let Err(e) = turn.handle.accumulate_spend(u, &state.llm_rates).await
            {
                warn!("{}: accumulate_spend failed: {e}", self.agent_id);
            }
        }

        // ── Attach session_id and return ────────────────────────────
        match result {
            Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                usage,
                timing,
                thinking,
                ..
            }) => Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                session_id: Some(turn.handle.session_id.clone()),
                usage,
                timing,
                thinking,
            }),
            other => other,
        }
    }

    /// Run the agentic loop with a **streaming** response pass.
    ///
    /// Steps 1–5 (session, instruction pass, tools) are buffered.  The
    /// response pass streams via `llm/stream` and returns a
    /// `BusPayload::LlmStreamResult` whose receiver the caller forwards
    /// as SSE events.
    ///
    /// Transcript persistence and spend accounting happen asynchronously
    /// in a background tee-task after the stream completes.
    pub(crate) async fn run_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        state: Arc<AgentsState>,
    ) -> BusResult {
        let turn = match self
            .prepare_turn(channel_id, content.clone(), session_id, &state)
            .await
        {
            TurnOutcome::EarlyReply(result) => {
                // Wrap buffered early-reply into a synthetic stream so the
                // caller always gets a uniform LlmStreamResult.
                return Self::wrap_as_stream(result);
            }
            TurnOutcome::Ready(t) => t,
        };

        // ── Streaming response pass ─────────────────────────────────
        let llm_rx = match state
            .stream_via_llm_with_system(&turn.channel_id, &turn.response_prompt, Some(&turn.system))
            .await
        {
            Ok(rx) => rx,
            Err(e) => return Err(e),
        };

        // ── Tee task: forward chunks + deferred persistence ─────────
        let (fwd_tx, fwd_rx) = mpsc::channel::<StreamChunk>(64);

        let agent_id = self.agent_id.clone();
        let llm_rates = state.llm_rates.clone();
        let debug_logging = self.debug_logging;
        let debug_n = turn.debug_n;
        let handle = turn.handle;

        tokio::spawn(async move {
            tee_and_persist(
                llm_rx,
                fwd_tx,
                handle,
                agent_id,
                llm_rates,
                debug_logging,
                debug_n,
            )
            .await;
        });

        Ok(BusPayload::LlmStreamResult {
            rx: StreamReceiver(fwd_rx),
        })
    }

    // ── Shared preparation (steps 1–5) ──────────────────────────────────

    /// Run steps 1–5: session, history, instruction pass, parse, tool execution.
    ///
    /// Returns [`TurnOutcome::Ready`] with the prepared context or
    /// [`TurnOutcome::EarlyReply`] when the instruction pass short-circuits
    /// with a direct reply.
    async fn prepare_turn(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        state: &Arc<AgentsState>,
    ) -> TurnOutcome {
        // ── 1. Session ────────────────────────────────────────────────
        let handle = match self.load_or_create_session(state, session_id.as_deref()) {
            Ok(h) => h,
            Err(e) => {
                return TurnOutcome::EarlyReply(Err(BusError::new(
                    -32000,
                    format!("session error: {e}"),
                )));
            }
        };

        if let Err(e) = handle.transcript_append("user", &content).await {
            warn!("{}: transcript_append(user) failed: {e}", self.agent_id);
        }

        // ── Debug: increment turn counter and record user input ───────
        let debug_n: usize = if self.debug_logging {
            let prev: usize = handle
                .kv_get("debug:turn_count")
                .await
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let n = prev + 1;
            if let Err(e) = handle.kv_set("debug:turn_count", &n.to_string()).await {
                warn!("{}: debug kv_set turn_count: {e}", self.agent_id);
            }
            if let Err(e) = handle
                .kv_set(&format!("debug:turn:{n}:user_input"), &content)
                .await
            {
                warn!("{}: debug kv_set user_input: {e}", self.agent_id);
            }
            n
        } else {
            0
        };

        // ── 2. History ────────────────────────────────────────────────
        let history = self.read_history(&handle).await;

        // ── 3. Instruction pass ───────────────────────────────────────
        let tool_manifest = self.build_tool_manifest(&self.allowed_tools);
        let memory_manifest = self.build_memory_manifest();

        let instruct_prompt = PromptBuilder::new(&self.prompts_dir)
            .layer(&self.instruct_prompt_file)
            .var("tools", &tool_manifest)
            .var("memory", &memory_manifest)
            .var("user_input", &content)
            .build();

        if self.debug_logging {
            if let Err(e) = handle
                .kv_set(
                    &format!("debug:turn:{debug_n}:instruct_prompt"),
                    &instruct_prompt,
                )
                .await
            {
                warn!("{}: debug kv_set instruct_prompt: {e}", self.agent_id);
            }
        }

        let instruction_text = if self.use_instruction_llm {
            extract_text(
                state
                    .complete_via_instruct_llm(&channel_id, &instruct_prompt, None)
                    .await,
            )
        } else {
            extract_text(
                state
                    .complete_via_llm_with_system(&channel_id, &instruct_prompt, None)
                    .await,
            )
        };

        let instruction_text = match instruction_text {
            Ok(t) => t,
            Err(e) => {
                warn!("{}: instruction pass failed: {}", self.agent_id, e.message);
                String::new()
            }
        };

        if self.debug_logging {
            if let Err(e) = handle
                .kv_set(
                    &format!("debug:turn:{debug_n}:instruction_response"),
                    &instruction_text,
                )
                .await
            {
                warn!("{}: debug kv_set instruction_response: {e}", self.agent_id);
            }
        }

        // ── 4. Parse instruction response ─────────────────────────────
        let InstructionResponse {
            tool_calls,
            reply: early_reply,
        } = parse_instruction_response(&instruction_text);

        if tool_calls.is_empty() {
            tracing::debug!("{}: no tool calls from instruction pass", self.agent_id);
        }

        if self.debug_logging {
            if let Err(e) = handle
                .kv_set(
                    &format!("debug:turn:{debug_n}:tool_calls_json"),
                    &instruction_text,
                )
                .await
            {
                warn!("{}: debug kv_set tool_calls_json: {e}", self.agent_id);
            }
        }

        // Early return: instruction pass provided a direct reply and no tools.
        if let Some(reply_text) = early_reply {
            if tool_calls.is_empty() {
                tracing::debug!(
                    "{}: instruction pass returned direct reply — skipping response pass",
                    self.agent_id
                );
                if let Err(e) = handle.transcript_append("assistant", &reply_text).await {
                    warn!(
                        "{}: transcript_append(assistant) failed: {e}",
                        self.agent_id
                    );
                }
                return TurnOutcome::EarlyReply(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: reply_text,
                    session_id: Some(handle.session_id.clone()),
                    usage: None,
                    timing: None,
                    thinking: None,
                }));
            }
        }

        // ── 5. Execute tools ──────────────────────────────────────────
        let mut context_parts: Vec<String> = Vec::new();
        let mut debug_tool_outputs: Vec<serde_json::Value> = Vec::new();

        for call in tool_calls {
            let tool_name = call.tool.clone();
            let action_name = call.action.clone();

            // Memory tools and local tools (in-process, blocking) — checked first.
            // Memory tools take priority; local tools are checked second.
            let in_process = self
                .memory_tools
                .iter()
                .chain(self.local_tools.iter())
                .find(|t| t.name() == tool_name.as_str());
            if let Some(local) = in_process {
                let tool = Arc::clone(local);
                let params = call.params.clone();
                match tokio::task::spawn_blocking(move || tool.call(&params)).await {
                    Ok(Ok(output)) => {
                        if self.debug_logging {
                            debug_tool_outputs.push(serde_json::json!({
                                "tool": &tool_name, "action": &action_name,
                                "ok": true, "output": &output,
                            }));
                        }
                        context_parts.push(output);
                    }
                    Ok(Err(e)) => {
                        if self.debug_logging {
                            debug_tool_outputs.push(serde_json::json!({
                                "tool": &tool_name, "action": &action_name,
                                "ok": false, "output": e.to_string(),
                            }));
                        }
                        warn!("{}: local tool '{}' error: {e}", self.agent_id, tool_name)
                    }
                    Err(e) => {
                        if self.debug_logging {
                            debug_tool_outputs.push(serde_json::json!({
                                "tool": &tool_name, "action": &action_name,
                                "ok": false, "output": format!("panic: {e}"),
                            }));
                        }
                        warn!("{}: local tool '{}' panic: {e}", self.agent_id, tool_name)
                    }
                }
                continue;
            }

            // External tools — dispatched through the bus.
            let params_json = call.params.to_string();
            match state
                .execute_tool(
                    &tool_name,
                    &action_name,
                    params_json,
                    &channel_id,
                    Some(handle.session_id.clone()),
                )
                .await
            {
                Ok(BusPayload::ToolResponse {
                    ok: true,
                    data_json: Some(data),
                    ..
                }) => {
                    if self.debug_logging {
                        debug_tool_outputs.push(serde_json::json!({
                            "tool": &tool_name, "action": &action_name,
                            "ok": true, "output": &data,
                        }));
                    }
                    context_parts.push(data);
                }
                Ok(BusPayload::ToolResponse {
                    ok: false,
                    error: Some(e),
                    ..
                }) => {
                    if self.debug_logging {
                        debug_tool_outputs.push(serde_json::json!({
                            "tool": &tool_name, "action": &action_name,
                            "ok": false, "output": &e,
                        }));
                    }
                    warn!(
                        "{}: bus tool {}/{} error: {e}",
                        self.agent_id, tool_name, action_name
                    )
                }
                Err(e) => {
                    if self.debug_logging {
                        debug_tool_outputs.push(serde_json::json!({
                            "tool": &tool_name, "action": &action_name,
                            "ok": false, "output": format!("bus error: {}", e.message),
                        }));
                    }
                    warn!("{}: bus tool dispatch failed: {}", self.agent_id, e.message)
                }
                _ => {}
            }
        }
        let context = context_parts.join("\n\n---\n\n");

        if self.debug_logging {
            let outputs_json = serde_json::to_string(&debug_tool_outputs).unwrap_or_default();
            if let Err(e) = handle
                .kv_set(
                    &format!("debug:turn:{debug_n}:tool_outputs_json"),
                    &outputs_json,
                )
                .await
            {
                warn!("{}: debug kv_set tool_outputs_json: {e}", self.agent_id);
            }
            if let Err(e) = handle
                .kv_set(&format!("debug:turn:{debug_n}:context"), &context)
                .await
            {
                warn!("{}: debug kv_set context: {e}", self.agent_id);
            }
        }

        // ── Build response-pass prompt ────────────────────────────────
        let system = preamble(&self.prompts_dir, &self.allowed_tools).build();

        let context_ref = if context.is_empty() {
            "(no context retrieved)"
        } else {
            &context
        };

        let response_prompt = PromptBuilder::new(&self.prompts_dir)
            .layer(&self.context_prompt_file)
            .var("context", context_ref)
            .var("history", &history)
            .var("user_input", &content)
            .build();

        if self.debug_logging {
            if let Err(e) = handle
                .kv_set(
                    &format!("debug:turn:{debug_n}:response_prompt"),
                    &response_prompt,
                )
                .await
            {
                warn!("{}: debug kv_set response_prompt: {e}", self.agent_id);
            }
        }

        TurnOutcome::Ready(PreparedTurn {
            handle,
            channel_id,
            system,
            response_prompt,
            debug_n,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Wrap a buffered `BusResult` into a synthetic `LlmStreamResult` so
    /// streaming callers always get a uniform return type.
    fn wrap_as_stream(result: BusResult) -> BusResult {
        match result {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                let (tx, rx) = mpsc::channel::<StreamChunk>(2);
                tokio::spawn(async move {
                    let _ = tx.send(StreamChunk::Content(content)).await;
                    let _ = tx.send(StreamChunk::Done { usage: None, timing: None }).await;
                });
                Ok(BusPayload::LlmStreamResult {
                    rx: StreamReceiver(rx),
                })
            }
            Err(e) => Err(e),
            other => other,
        }
    }

    fn load_or_create_session(
        &self,
        state: &AgentsState,
        session_id: Option<&str>,
    ) -> Result<SessionHandle, AppError> {
        let agent_store = state.open_agent_store(&self.agent_id)?;
        let memory = &state.memory;

        if let Some(sid) = session_id {
            return memory.load_session_in(
                &agent_store.agent_sessions_dir(),
                &agent_store.agent_sessions_index(),
                sid,
                Some(&self.agent_id),
            );
        }

        let store_types: Vec<&str> = state
            .agent_memory
            .get(&self.agent_id)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_else(|| vec!["basic_session"]);

        if store_types.len() == 1 && store_types[0] == "basic_session" {
            return agent_store.get_or_create_session(memory, &self.agent_id);
        }

        memory.create_session_in(
            &agent_store.agent_sessions_dir(),
            &agent_store.agent_sessions_index(),
            &store_types,
            Some(&self.agent_id),
        )
    }

    async fn read_history(&self, handle: &SessionHandle) -> String {
        match handle.transcript_read_last(CONTEXT_WINDOW).await {
            Ok(entries) => {
                let mut h = String::new();
                for entry in entries.iter().rev().skip(1).rev() {
                    h.push_str(&format!("{}: {}\n", entry.role, entry.content));
                }
                h
            }
            Err(e) => {
                warn!("{}: transcript_read_last failed: {e}", self.agent_id);
                String::new()
            }
        }
    }

    fn build_memory_manifest(&self) -> String {
        if self.memory_tools.is_empty() {
            return "No memory available.".to_string();
        }
        self.memory_tools
            .iter()
            .map(|t| format!("- tool: \"{}\", {}", t.name(), t.description()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn build_tool_manifest(&self, bus_tools: &[String]) -> String {
        let mut lines: Vec<String> = Vec::new();

        // Local tools first.
        for tool in &self.local_tools {
            lines.push(format!(
                "- tool: \"{}\", {}",
                tool.name(),
                tool.description()
            ));
        }

        // Bus-dispatched tools.
        for tool in bus_tools {
            match tool.as_str() {
                "gmail" => lines.push(
                    "- tool: \"gmail\", action: \"read_latest\", params: {\"n\": <count>}\n\
                     \x20 Description: Reads the most recent Gmail messages."
                        .to_string(),
                ),
                "newsmail_aggregator" => lines.push(
                    "- tool: \"newsmail_aggregator\", action: \"get\", params: {\"n_last\": <count>}\n\
                     \x20 Description: Fetches recent news email summaries."
                        .to_string(),
                ),
                other => lines.push(format!(
                    "- tool: \"{other}\", action: \"<action>\", params: {{}}"
                )),
            }
        }

        if lines.is_empty() {
            "No tools available.".to_string()
        } else {
            lines.join("\n")
        }
    }
}

// ── Tee task ─────────────────────────────────────────────────────────────────

/// Forward chunks from the LLM stream to the browser while buffering the full
/// response text.  On [`StreamChunk::Done`], persist the transcript and
/// accumulate spend.
///
/// If the browser disconnects (`fwd_tx` send fails), the task continues
/// draining the LLM stream to ensure the full response is still persisted.
async fn tee_and_persist(
    mut llm_rx: mpsc::Receiver<StreamChunk>,
    fwd_tx: mpsc::Sender<StreamChunk>,
    handle: SessionHandle,
    agent_id: String,
    llm_rates: crate::llm::ModelRates,
    debug_logging: bool,
    debug_n: usize,
) {
    let mut content_buf = String::new();
    let mut browser_alive = true;

    while let Some(chunk) = llm_rx.recv().await {
        match &chunk {
            StreamChunk::Content(delta) => content_buf.push_str(delta),
            StreamChunk::Thinking(_) => { /* forward only */ }
            StreamChunk::Done { usage, .. } => {
                // Persist transcript.
                if !content_buf.is_empty() {
                    if let Err(e) = handle.transcript_append("assistant", &content_buf).await {
                        warn!("{agent_id}: transcript_append(assistant) failed: {e}");
                    }
                }
                // Accumulate spend.
                if let Some(u) = usage {
                    if let Err(e) = handle.accumulate_spend(u, &llm_rates).await {
                        warn!("{agent_id}: accumulate_spend failed: {e}");
                    }
                }
                // Debug: record final response.
                if debug_logging {
                    if let Err(e) = handle
                        .kv_set(&format!("debug:turn:{debug_n}:response"), &content_buf)
                        .await
                    {
                        warn!("{agent_id}: debug kv_set response: {e}");
                    }
                }
            }
        }

        // Forward chunk to the browser-bound channel.
        if browser_alive && fwd_tx.send(chunk).await.is_err() {
            // Browser disconnected — keep draining to persist the full response.
            browser_alive = false;
        }
    }
}

// ── Shared helpers (pub(crate) for use in plugin tests) ──────────────────────

/// Extract the text content from a `BusPayload::CommsMessage` result.
fn extract_text(result: BusResult) -> Result<String, BusError> {
    match result {
        Ok(BusPayload::CommsMessage { content, .. }) => Ok(content),
        Ok(other) => Err(BusError::new(
            -32000,
            format!("unexpected payload variant: {other:?}"),
        )),
        Err(e) => Err(e),
    }
}

/// Parse the instruction LLM response into tool calls and an optional direct reply.
///
/// Handles two formats (strips markdown code fences if present):
///
/// 1. **New object format** `{"tools": [...], "reply": "..."}` — the model can
///    optionally include a direct reply when no tools are needed, eliminating
///    the second LLM call for simple questions.
/// 2. **Legacy array format** `[{"tool": "...", ...}]` — backward-compatible.
///
/// Returns an empty tool list and `None` reply on any parse failure so the
/// response pass still runs (graceful degradation).
fn parse_instruction_response(text: &str) -> InstructionResponse {
    let trimmed = text.trim();

    // Strip ```json ... ``` or ``` ... ``` fences.
    let json_text = if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim()
    } else {
        trimmed
    };

    // Determine format by the first non-whitespace character.
    let first_char = json_text.chars().find(|c| !c.is_whitespace());

    if first_char == Some('{') {
        // New object format `{"tools": [...], "reply": "..."}`.
        if let Some(start) = json_text.find('{')
            && let Some(end) = json_text.rfind('}')
            && end >= start
        {
            let slice = &json_text[start..=end];
            #[derive(Deserialize)]
            struct InstructWrapper {
                #[serde(default)]
                tools: Vec<ToolCall>,
                reply: Option<String>,
            }
            if let Ok(wrapper) = serde_json::from_str::<InstructWrapper>(slice) {
                let reply = wrapper.reply.and_then(|r| {
                    let r = r.trim().to_string();
                    if r.is_empty() { None } else { Some(r) }
                });
                return InstructionResponse {
                    tool_calls: wrapper.tools,
                    reply,
                };
            }
        }
    }

    // Legacy `[...]` array format (or fallback).
    if let Some(start) = json_text.find('[')
        && let Some(end) = json_text.rfind(']')
        && end >= start
    {
        let slice = &json_text[start..=end];
        if let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(slice) {
            return InstructionResponse {
                tool_calls: calls,
                reply: None,
            };
        }
    }

    if let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(json_text) {
        return InstructionResponse {
            tool_calls: calls,
            reply: None,
        };
    }

    InstructionResponse {
        tool_calls: Vec::new(),
        reply: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_instruction_response: new object format ─────────────────

    #[test]
    fn parse_instr_object_with_reply_no_tools() {
        let json = r#"{"tools": [], "reply": "Hello!"}"#;
        let r = parse_instruction_response(json);
        assert!(r.tool_calls.is_empty());
        assert_eq!(r.reply.as_deref(), Some("Hello!"));
    }

    #[test]
    fn parse_instr_object_with_tools_null_reply() {
        let json = r#"{"tools": [{"tool":"gmail","action":"read_latest","params":{"n":5}}], "reply": null}"#;
        let r = parse_instruction_response(json);
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].tool, "gmail");
        assert!(r.reply.is_none());
    }

    #[test]
    fn parse_instr_object_empty_reply_treated_as_none() {
        let json = r#"{"tools": [], "reply": "   "}"#;
        let r = parse_instruction_response(json);
        assert!(r.reply.is_none());
    }

    #[test]
    fn parse_instr_object_with_fence() {
        let text = "```json\n{\"tools\": [], \"reply\": \"Hi\"}\n```";
        let r = parse_instruction_response(text);
        assert_eq!(r.reply.as_deref(), Some("Hi"));
    }

    // ── parse_instruction_response: legacy array format ───────────────

    #[test]
    fn parse_instr_legacy_empty_array() {
        let r = parse_instruction_response("[]");
        assert!(r.tool_calls.is_empty());
        assert!(r.reply.is_none());
    }

    #[test]
    fn parse_instr_legacy_valid() {
        let json = r#"[{"tool":"gmail","action":"read_latest","params":{"n":5}}]"#;
        let r = parse_instruction_response(json);
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].action, "read_latest");
        assert!(r.reply.is_none());
    }

    #[test]
    fn parse_instr_legacy_with_fence() {
        let text = "```json\n[{\"tool\":\"gmail\",\"action\":\"read_latest\"}]\n```";
        let r = parse_instruction_response(text);
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].tool, "gmail");
    }

    #[test]
    fn parse_instr_invalid_graceful() {
        let r = parse_instruction_response("not json at all");
        assert!(r.tool_calls.is_empty());
        assert!(r.reply.is_none());
    }
}
