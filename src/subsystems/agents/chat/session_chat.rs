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

    // Build the full prompt with context.
    let prompt = if context.is_empty() {
        content.to_string()
    } else {
        format!("Previous conversation:\n{context}\nuser: {content}")
    };

    // Get LLM completion.
    let result = ChatCore::basic_complete(state, channel_id, &prompt).await;

    // Record assistant reply in transcript.
    if let Ok(BusPayload::CommsMessage { content: ref reply, .. }) = result {
        if let Err(e) = handle.transcript_append("assistant", reply).await {
            warn!("session_chat: transcript_append(assistant) failed: {e}");
        }
    }

    match result {
        Ok(BusPayload::CommsMessage { channel_id, content, .. }) => Ok(BusPayload::CommsMessage {
            channel_id,
            content,
            session_id: Some(handle.session_id.clone()),
        }),
        other => other,
    }
}

fn init_session(state: &AgentsState) -> Result<SessionHandle, crate::error::AppError> {
    let memory = &state.memory;

    // Look up which stores this agent needs from config.
    let store_types = state
        .agent_memory
        .get("chat")
        .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["basic_session"]);

    memory.create_session(&store_types, Some("chat"))
}

fn load_session(state: &AgentsState, session_id: &str) -> Result<SessionHandle, crate::error::AppError> {
    let memory = &state.memory;

    memory.load_session(session_id, Some("chat"))
}
