//! `agentic-chat` agent plugin — dual-pass instruction loop.
//!
//! ## Workflow
//!
//! 1. **Instruction pass** — builds a tool manifest from `state.enabled_tools`,
//!    submits it with the user message to the instruction LLM (`llm/instruct`),
//!    parses the response as a JSON array of `{tool, action, params}` calls.
//! 2. **Tool execution** — runs each call generically via `state.execute_tool()`,
//!    collects outputs into a local `context` string.
//! 3. **Response pass** — sends user prompt + context + history to the main LLM
//!    (`llm/complete`) and returns the reply with the session ID attached.
//!
//! Both LLM calls go through the bus; the plugin never owns a provider directly.

use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::oneshot;
use tracing::warn;

use crate::config::AgenticChatConfig;
use crate::subsystems::memory::handle::SessionHandle;
use crate::supervisor::bus::{BusError, BusPayload, BusResult};

use super::{Agent, AgentsState};
use crate::subsystems::agents::core::prompt::PromptBuilder;
use crate::subsystems::agents::core::prompt::preamble;

/// How many recent transcript entries to inject as conversation context.
const CONTEXT_WINDOW: usize = 20;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub(crate) struct AgenticChatPlugin {
    use_instruction_llm: bool,
}

impl AgenticChatPlugin {
    pub fn new(cfg: &AgenticChatConfig) -> Self {
        Self {
            use_instruction_llm: cfg.use_instruction_llm,
        }
    }
}

impl Agent for AgenticChatPlugin {
    fn id(&self) -> &str {
        "agentic-chat"
    }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let use_instruction_llm = self.use_instruction_llm;
        tokio::spawn(async move {
            let result =
                handle_agentic_loop(use_instruction_llm, channel_id, content, session_id, state)
                    .await;
            let _ = reply_tx.send(result);
        });
    }
}

// ── Tool call schema ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ToolCall {
    tool: String,
    action: String,
    #[serde(default)]
    params: serde_json::Value,
}

// ── Core loop ─────────────────────────────────────────────────────────────────

async fn handle_agentic_loop(
    use_instruction_llm: bool,
    channel_id: String,
    content: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
) -> BusResult {
    // ── 1. Session ────────────────────────────────────────────────────
    let handle = match load_or_create_session(&state, session_id.as_deref()) {
        Ok(h) => h,
        Err(e) => {
            return Err(BusError::new(-32000, format!("session error: {e}")));
        }
    };

    if let Err(e) = handle.transcript_append("user", &content).await {
        warn!("agentic-chat: transcript_append(user) failed: {e}");
    }

    // ── 2. History ────────────────────────────────────────────────────
    let history = match handle.transcript_read_last(CONTEXT_WINDOW).await {
        Ok(entries) => {
            let mut h = String::new();
            for entry in &entries {
                // Skip the just-appended user message (last entry).
                if std::ptr::eq(&entries[entries.len() - 1], entry) {
                    continue;
                }
                h.push_str(&format!("{}: {}\n", entry.role, entry.content));
            }
            h
        }
        Err(e) => {
            warn!("agentic-chat: transcript_read_last failed: {e}");
            String::new()
        }
    };

    // ── 3. Instruction pass ───────────────────────────────────────────
    let tool_manifest = build_tool_manifest(&state.enabled_tools);

    let instruct_prompt = PromptBuilder::new("config/prompts")
        .layer("agentic_instruct.txt")
        .var("tools", &tool_manifest)
        .var("user_input", &content)
        .build();

    let instruction_text = if use_instruction_llm {
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
            warn!("agentic-chat: instruction pass failed: {}", e.message);
            String::new()
        }
    };

    // ── 4. Parse tool calls ───────────────────────────────────────────
    let tool_calls: Vec<ToolCall> = parse_tool_calls(&instruction_text);
    if tool_calls.is_empty() {
        tracing::debug!("agentic-chat: no tool calls parsed from instruction pass");
    }

    // ── 5. Execute tools ──────────────────────────────────────────────
    let mut context_parts: Vec<String> = Vec::new();
    for call in tool_calls {
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
            }) => {
                context_parts.push(data);
            }
            Ok(BusPayload::ToolResponse {
                ok: false,
                error: Some(e),
                ..
            }) => {
                warn!(
                    "agentic-chat: tool {}/{} error: {e}",
                    call.tool, call.action
                );
            }
            Err(e) => {
                warn!("agentic-chat: tool dispatch failed: {}", e.message);
            }
            _ => {}
        }
    }
    let context = context_parts.join("\n\n---\n\n");

    // ── 6. Response pass ──────────────────────────────────────────────
    let system = preamble("config/prompts", &state.enabled_tools).build();

    let context_ref = if context.is_empty() {
        "(no context retrieved)"
    } else {
        &context
    };

    let response_prompt = PromptBuilder::new("config/prompts")
        .layer("agentic_context.txt")
        .var("context", context_ref)
        .var("history", &history)
        .var("user_input", &content)
        .build();

    let result = state
        .complete_via_llm_with_system(&channel_id, &response_prompt, Some(&system))
        .await;

    // ── 7. Persist reply + spend ──────────────────────────────────────
    if let Ok(BusPayload::CommsMessage {
        content: ref reply,
        ref usage,
        ..
    }) = result
    {
        if let Err(e) = handle.transcript_append("assistant", reply).await {
            warn!("agentic-chat: transcript_append(assistant) failed: {e}");
        }
        if let Some(u) = usage {
            if let Err(e) = handle.accumulate_spend(u, &state.llm_rates).await {
                warn!("agentic-chat: accumulate_spend failed: {e}");
            }
        }
    }

    // ── 8. Attach session_id and return ──────────────────────────────
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load_or_create_session(
    state: &AgentsState,
    session_id: Option<&str>,
) -> Result<SessionHandle, crate::error::AppError> {
    let memory = &state.memory;
    let agent_store = state.open_agent_store("agentic-chat")?;
    let sessions_root = agent_store.agent_sessions_dir();
    let index_path = agent_store.agent_sessions_index();

    if let Some(sid) = session_id {
        return memory.load_session_in(&sessions_root, &index_path, sid, Some("agentic-chat"));
    }

    let default_store_types = state
        .agent_memory
        .get("agentic-chat")
        .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["basic_session"]);

    if default_store_types.len() == 1 && default_store_types[0] == "basic_session" {
        return agent_store.get_or_create_session(memory, "agentic-chat");
    }

    memory.create_session_in(
        &sessions_root,
        &index_path,
        &default_store_types,
        Some("agentic-chat"),
    )
}

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
/// Strips markdown code fences if present.  Returns an empty vec on any
/// parse failure so the response pass still runs (graceful degradation).
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
    if let Some(start) = json_text.find('[') {
        if let Some(end) = json_text.rfind(']') {
            if end >= start {
                let slice = &json_text[start..=end];
                return serde_json::from_str(slice).unwrap_or_default();
            }
        }
    }

    serde_json::from_str(json_text).unwrap_or_default()
}

/// Build a human-readable tool manifest from the list of enabled tool names.
///
/// The manifest is inserted into the instruction prompt so the LLM knows
/// which tools are available and how to call them.
fn build_tool_manifest(enabled_tools: &[String]) -> String {
    if enabled_tools.is_empty() {
        return "No tools available.".to_string();
    }

    let mut lines = Vec::new();
    for tool in enabled_tools {
        match tool.as_str() {
            "gmail" => {
                lines.push(
                    r#"- tool: "gmail", action: "read_latest", params: {"n": <count>}
  Description: Reads the most recent Gmail messages."#
                        .to_string(),
                );
            }
            "newsmail_aggregator" => {
                lines.push(
                    r#"- tool: "newsmail_aggregator", action: "get", params: {"n_last": <count>}
  Description: Fetches recent news email summaries."#
                        .to_string(),
                );
            }
            other => {
                lines.push(format!(
                    "- tool: \"{other}\", action: \"<action>\", params: {{}}"
                ));
            }
        }
    }
    lines.join("\n")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_calls_empty_array() {
        let calls = parse_tool_calls("[]");
        assert!(calls.is_empty());
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
        let calls = parse_tool_calls("not json at all");
        assert!(calls.is_empty());
    }

    #[test]
    fn build_tool_manifest_empty() {
        let manifest = build_tool_manifest(&[]);
        assert_eq!(manifest, "No tools available.");
    }

    #[test]
    fn build_tool_manifest_known_tools() {
        let tools = vec!["gmail".to_string(), "newsmail_aggregator".to_string()];
        let manifest = build_tool_manifest(&tools);
        assert!(manifest.contains("gmail"));
        assert!(manifest.contains("newsmail_aggregator"));
    }
}
