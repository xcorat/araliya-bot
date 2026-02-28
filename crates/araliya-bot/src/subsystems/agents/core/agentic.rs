//! Shared agentic loop — session lifecycle + instruction pass + tool execution + response pass.
//!
//! [`AgenticLoop`] is the composable building block for multi-pass agent plugins.
//! Plugins create an `AgenticLoop`, register any in-process [`LocalTool`]s, and call
//! [`AgenticLoop::run`] from within `tokio::spawn`.
//!
//! ## Local tools vs bus tools
//!
//! - **Local tools** implement [`LocalTool`] and run inside `tokio::task::spawn_blocking`.
//!   Use them for in-process blocking I/O (e.g. docstore RAG).
//! - **Bus tools** are dispatched through `state.execute_tool()` after all local tools
//!   have been checked.

use std::sync::Arc;

use serde::Deserialize;
use tracing::warn;

use crate::error::AppError;
use crate::subsystems::memory::handle::SessionHandle;
use crate::supervisor::bus::{BusError, BusPayload, BusResult};

use crate::subsystems::agents::AgentsState;
use super::prompt::{preamble, PromptBuilder};

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
    local_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
    prompts_dir: String,
}

impl AgenticLoop {
    pub(crate) fn new(
        agent_id: impl Into<String>,
        use_instruction_llm: bool,
        instruct_prompt_file: impl Into<String>,
        context_prompt_file: impl Into<String>,
        local_tools: Vec<Arc<dyn LocalTool + Send + Sync>>,
        prompts_dir: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            use_instruction_llm,
            instruct_prompt_file: instruct_prompt_file.into(),
            context_prompt_file: context_prompt_file.into(),
            local_tools,
            prompts_dir: prompts_dir.into(),
        }
    }

    /// Run the full agentic loop for a single request.
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
        // ── 1. Session ────────────────────────────────────────────────
        let handle = match self.load_or_create_session(&state, session_id.as_deref()) {
            Ok(h) => h,
            Err(e) => return Err(BusError::new(-32000, format!("session error: {e}"))),
        };

        if let Err(e) = handle.transcript_append("user", &content).await {
            warn!("{}: transcript_append(user) failed: {e}", self.agent_id);
        }

        // ── 2. History ────────────────────────────────────────────────
        let history = self.read_history(&handle).await;

        // ── 3. Instruction pass ───────────────────────────────────────
        let tool_manifest = self.build_tool_manifest(&state.enabled_tools);

        let instruct_prompt = PromptBuilder::new(&self.prompts_dir)
            .layer(&self.instruct_prompt_file)
            .var("tools", &tool_manifest)
            .var("user_input", &content)
            .build();

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
                warn!(
                    "{}: instruction pass failed: {}",
                    self.agent_id, e.message
                );
                String::new()
            }
        };

        // ── 4. Parse tool calls ───────────────────────────────────────
        let tool_calls = parse_tool_calls(&instruction_text);
        if tool_calls.is_empty() {
            tracing::debug!("{}: no tool calls from instruction pass", self.agent_id);
        }

        // ── 5. Execute tools ──────────────────────────────────────────
        let mut context_parts: Vec<String> = Vec::new();
        for call in tool_calls {
            // Local tools (in-process, blocking) — checked first.
            if let Some(local) = self
                .local_tools
                .iter()
                .find(|t| t.name() == call.tool.as_str())
            {
                let tool = Arc::clone(local);
                let params = call.params.clone();
                match tokio::task::spawn_blocking(move || tool.call(&params)).await {
                    Ok(Ok(output)) => context_parts.push(output),
                    Ok(Err(e)) => {
                        warn!("{}: local tool '{}' error: {e}", self.agent_id, call.tool)
                    }
                    Err(e) => {
                        warn!("{}: local tool '{}' panic: {e}", self.agent_id, call.tool)
                    }
                }
                continue;
            }

            // External tools — dispatched through the bus.
            let params_json = call.params.to_string();
            match state
                .execute_tool(
                    &call.tool,
                    &call.action,
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
                }) => context_parts.push(data),
                Ok(BusPayload::ToolResponse {
                    ok: false,
                    error: Some(e),
                    ..
                }) => {
                    warn!(
                        "{}: bus tool {}/{} error: {e}",
                        self.agent_id, call.tool, call.action
                    )
                }
                Err(e) => {
                    warn!(
                        "{}: bus tool dispatch failed: {}",
                        self.agent_id, e.message
                    )
                }
                _ => {}
            }
        }
        let context = context_parts.join("\n\n---\n\n");

        // ── 6. Response pass ──────────────────────────────────────────
        let system = preamble(&self.prompts_dir, &state.enabled_tools).build();

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

        let result = state
            .complete_via_llm_with_system(&channel_id, &response_prompt, Some(&system))
            .await;

        // ── 7. Persist reply + spend ──────────────────────────────────
        if let Ok(BusPayload::CommsMessage {
            content: ref reply,
            ref usage,
            ..
        }) = result
        {
            if let Err(e) = handle.transcript_append("assistant", reply).await {
                warn!("{}: transcript_append(assistant) failed: {e}", self.agent_id);
            }
            if let Some(u) = usage
                && let Err(e) = handle.accumulate_spend(u, &state.llm_rates).await
            {
                warn!("{}: accumulate_spend failed: {e}", self.agent_id);
            }
        }

        // ── 8. Attach session_id and return ──────────────────────────
        match result {
            Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                usage,
                ..
            }) => Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                session_id: Some(handle.session_id.clone()),
                usage,
            }),
            other => other,
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

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

    fn build_tool_manifest(&self, bus_tools: &[String]) -> String {
        let mut lines: Vec<String> = Vec::new();

        // Local tools first.
        for tool in &self.local_tools {
            lines.push(format!("- tool: \"{}\", {}", tool.name(), tool.description()));
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

/// Parse a JSON array of tool calls from the instruction LLM response.
///
/// Strips markdown code fences if present.  Returns an empty vec on any parse
/// failure so the response pass still runs (graceful degradation).
fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
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

    // Find the outermost `[...]` array.
    if let Some(start) = json_text.find('[')
        && let Some(end) = json_text.rfind(']')
        && end >= start
    {
        let slice = &json_text[start..=end];
        return serde_json::from_str(slice).unwrap_or_default();
    }

    serde_json::from_str(json_text).unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_calls_empty_array() {
        assert!(parse_tool_calls("[]").is_empty());
    }

    #[test]
    fn parse_tool_calls_valid() {
        let json = r#"[{"tool":"gmail","action":"read_latest","params":{"n":5}}]"#;
        let calls = parse_tool_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "gmail");
        assert_eq!(calls[0].action, "read_latest");
    }

    #[test]
    fn parse_tool_calls_with_fence() {
        let text = "```json\n[{\"tool\":\"gmail\",\"action\":\"read_latest\"}]\n```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "gmail");
    }

    #[test]
    fn parse_tool_calls_invalid_graceful() {
        assert!(parse_tool_calls("not json at all").is_empty());
    }
}
