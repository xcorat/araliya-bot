//! `session_chat` agent plugin — session-aware chat built on [`ChatCore`].
//!
//! Currently identical to `basic_chat`; the plugin exists as the extension
//! point for session management, memory, prompt templating, and tool use
//! that will be layered on top of the shared [`ChatCore`].

use std::sync::Arc;

use tokio::sync::oneshot;

use crate::supervisor::bus::BusResult;
use super::super::{AgentPlugin, AgentsState};
use super::core::ChatCore;

pub(crate) struct SessionChatPlugin;

impl AgentPlugin for SessionChatPlugin {
    fn id(&self) -> &str { "chat" }

    fn handle(
        &self,
        channel_id: String,
        content: String,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            // TODO: session lookup, prompt templating, memory injection, etc.
            let result = ChatCore::basic_complete(&state, &channel_id, &content).await;
            let _ = reply_tx.send(result);
        });
    }
}
