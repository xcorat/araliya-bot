//! `agentic-chat` agent plugin — dual-pass instruction loop.
//!
//! ## Workflow
//!
//! 1. **Instruction pass** — builds a tool manifest from `state.enabled_tools`,
//!    submits it with the user message to the instruction LLM (`llm/instruct`),
//!    parses the response as a JSON array of `{tool, action, params}` calls.
//! 2. **Tool execution** — runs each call generically via `state.execute_tool()`,
//!    collecting outputs into a context string.
//! 3. **Response pass** — sends user prompt + context + history to the main LLM
//!    (`llm/complete`) and returns the reply with the session ID attached.
//!
//! The shared [`AgenticLoop`] in `core::agentic` handles the full session
//! lifecycle; this file is a thin configuration wrapper.

use std::sync::Arc;

use tokio::sync::oneshot;

use crate::config::AgenticChatConfig;
use crate::supervisor::bus::BusResult;

use super::core::agentic::AgenticLoop;
use super::{Agent, AgentsState};

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
        let loop_ = AgenticLoop::new(
            "agentic-chat",
            self.use_instruction_llm,
            "agentic_instruct.txt",
            "agentic_context.txt",
            vec![],
            "config/prompts",
        );
        tokio::spawn(async move {
            let result = loop_.run(channel_id, content, session_id, state).await;
            let _ = reply_tx.send(result);
        });
    }
}
