//! `session_chat` agent plugin — session-aware chat built on [`ChatCore`].
//!
//! Creates a new memory session on first message (ephemeral — one per bot run).
//! Records user messages and assistant replies in the transcript, and injects
//! recent conversation history into the LLM prompt for multi-turn context.

use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};
use tracing::{info, warn};

use crate::supervisor::bus::{BusPayload, BusResult};
use super::super::{Agent, AgentsState};
use super::core::ChatCore;
use crate::subsystems::agents::core::prompt::PromptBuilder;

use crate::subsystems::memory::handle::SessionHandle;

/// How many recent transcript entries to inject as conversation context.
const CONTEXT_WINDOW: usize = 20;

pub(crate) struct SessionChatPlugin {
    /// Lazily initialised on first message.
    session: Arc<Mutex<Option<SessionHandle>>>,
}

impl SessionChatPlugin {
    pub fn new() -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
        }
    }
}

impl Agent for SessionChatPlugin {
    fn id(&self) -> &str { "chat" }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let session = self.session.clone();
        tokio::spawn(async move {
            let result = handle_with_memory(
                &session,
                &state,
                &channel_id,
                &content,
                session_id.as_deref(),
            ).await;
            let _ = reply_tx.send(result);
        });
    }
}

async fn handle_with_memory(
    session_mutex: &Mutex<Option<SessionHandle>>,
    state: &Arc<AgentsState>,
    channel_id: &str,
    content: &str,
    requested_session_id: Option<&str>,
) -> BusResult {
    // Ensure session exists (reuse requested session when provided).
    let handle = {
        let mut guard = session_mutex.lock().await;
        if let Some(session_id) = requested_session_id {
            let must_load = guard
                .as_ref()
                .map(|h| h.session_id.as_str() != session_id)
                .unwrap_or(true);

            if must_load {
                match load_session(state, session_id) {
                    Ok(h) => {
                        info!(session_id = %h.session_id, "session_chat: session loaded from request");
                        *guard = Some(h);
                    }
                    Err(e) => {
                        return Err(crate::supervisor::bus::BusError::new(
                            -32000,
                            format!("session load failed: {e}"),
                        ));
                    }
                }
            }
        } else {
            match init_session(state) {
                Ok(h) => {
                    info!(session_id = %h.session_id, "session_chat: session created");
                    *guard = Some(h);
                }
                Err(e) => {
                    warn!("session_chat: failed to create session: {e}");
                    // Fall back to stateless completion.
                    return ChatCore::basic_complete(state, channel_id, content).await;
                }
            }
        }
        guard.clone().unwrap()
    };

    // Record user message.
    if let Err(e) = handle.transcript_append("user", content).await {
        warn!("session_chat: transcript_append(user) failed: {e}");
    }

    // Build conversation context from recent transcript.
    let context = match handle.transcript_read_last(CONTEXT_WINDOW).await {
        Ok(entries) => {
            let mut ctx = String::new();
            for entry in &entries {
                // Skip the just-appended user message (it's already the prompt).
                if std::ptr::eq(&entries[entries.len() - 1], entry) {
                    continue;
                }
                ctx.push_str(&format!("{}: {}\n", entry.role, entry.content));
            }
            ctx
        }
        Err(e) => {
            warn!("session_chat: transcript_read_last failed: {e}");
            String::new()
        }
    };

    // Build system preamble (identity layers) and user message separately.
    let system = crate::subsystems::agents::core::prompt::preamble("config/prompts", &state.enabled_tools).build();

    let body = std::fs::read_to_string("config/prompts/chat_context.txt")
        .unwrap_or_else(|_| "Conversation history:\n{{history}}\nUser: {{user_input}}\nAI:".to_string());
    let prompt = PromptBuilder::new("config/prompts")
        .append(body)
        .var("history", &context)
        .var("user_input", content)
        .build();

    // Get LLM completion with identity in system role.
    let result = state.complete_via_llm_with_system(channel_id, &prompt, Some(&system)).await;

    // Record assistant reply in transcript + accumulate token spend.
    if let Ok(BusPayload::CommsMessage { content: ref reply, ref usage, .. }) = result {
        if let Err(e) = handle.transcript_append("assistant", reply).await {
            warn!("session_chat: transcript_append(assistant) failed: {e}");
        }
        if let Some(u) = usage {
            if let Err(e) = handle.accumulate_spend(u, &state.llm_rates).await {
                warn!("session_chat: accumulate_spend failed: {e}");
            }
        }
    }

    match result {
        Ok(BusPayload::CommsMessage { channel_id, content, usage, .. }) => Ok(BusPayload::CommsMessage {
            channel_id,
            content,
            session_id: Some(handle.session_id.clone()),
            usage,
        }),
        other => other,
    }
}

fn init_session(state: &AgentsState) -> Result<SessionHandle, crate::error::AppError> {
    let memory = &state.memory;
    let agent_store = state.open_agent_store("chat")?;
    let default_store_types = state
        .agent_memory
        .get("chat")
        .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["basic_session"]);

    if default_store_types.len() == 1 && default_store_types[0] == "basic_session" {
        return agent_store.get_or_create_session(memory, "chat");
    }

    let sessions_root = agent_store.agent_sessions_dir();
    let index_path = agent_store.agent_sessions_index();
    memory.create_session_in(&sessions_root, &index_path, &default_store_types, Some("chat"))
}

fn load_session(state: &AgentsState, session_id: &str) -> Result<SessionHandle, crate::error::AppError> {
    let memory = &state.memory;
    let agent_store = state.open_agent_store("chat")?;
    let sessions_root = agent_store.agent_sessions_dir();
    let index_path = agent_store.agent_sessions_index();

    memory.load_session_in(&sessions_root, &index_path, session_id, Some("chat"))
}
