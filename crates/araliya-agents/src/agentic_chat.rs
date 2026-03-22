//! `agentic-chat` agent plugin — dual-pass instruction loop.
//!
//! ## Workflow
//!
//! 1. **Instruction pass** — builds a tool manifest from the agent's declared skills,
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

use araliya_core::bus::message::BusResult;
use araliya_core::config::AgenticChatConfig;

use super::core::agentic::{AgenticLoop, LocalTool};
use super::{Agent, AgentsState};

#[cfg(feature = "idocstore")]
use super::docs::DocsRagTool;
#[cfg(feature = "idocstore")]
use araliya_memory::stores::docstore::IDocStore;

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

impl AgenticChatPlugin {
    fn build_loop(&self, state: &AgentsState) -> AgenticLoop {
        let allowed_tools = state
            .agent_skills
            .get("agentic-chat")
            .cloned()
            .unwrap_or_default();

        let memory_tools = Self::build_memory_tools(state);

        AgenticLoop::new(
            "agentic-chat",
            self.use_instruction_llm,
            "instruct.md",
            "context.md",
            vec![],
            memory_tools,
            allowed_tools,
            &state.agents_dir,
            state.debug_logging,
        )
    }

    fn build_memory_tools(state: &AgentsState) -> Vec<Arc<dyn LocalTool + Send + Sync>> {
        #[cfg(feature = "idocstore")]
        {
            if let Some(docs_cfg) = state.agent_docs.get("agentic-chat")
                && let Some(identity) = state.agent_identities.get("agentic-chat")
            {
                let dir = &identity.identity_dir;
                let populated = IDocStore::open(dir)
                    .and_then(|s| s.list_documents())
                    .map(|docs| !docs.is_empty())
                    .unwrap_or(false);
                if populated {
                    let tool: Arc<dyn LocalTool + Send + Sync> = Arc::new(DocsRagTool {
                        identity_dir: dir.clone(),
                        index_name: docs_cfg
                            .index
                            .clone()
                            .unwrap_or_else(|| "index.md".to_string()),
                        use_kg: docs_cfg.use_kg,
                        kg_cfg: docs_cfg.kg.clone(),
                    });
                    return vec![tool];
                }
            }
        }
        #[cfg(not(feature = "idocstore"))]
        let _ = state;
        vec![]
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
        let loop_ = self.build_loop(&state);
        tokio::spawn(async move {
            let result = loop_.run(channel_id, content, session_id, state).await;
            let _ = reply_tx.send(result);
        });
    }

    fn handle_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let loop_ = self.build_loop(&state);
        tokio::spawn(async move {
            let result = loop_
                .run_stream(channel_id, content, session_id, state)
                .await;
            let _ = reply_tx.send(result);
        });
    }
}
