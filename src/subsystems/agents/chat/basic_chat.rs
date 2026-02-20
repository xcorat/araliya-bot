//! `basic_chat` agent plugin — minimal LLM pass-through.
//!
//! Delegates entirely to [`ChatCore::basic_complete`](super::core::ChatCore::basic_complete).

use std::sync::Arc;

use tokio::sync::oneshot;

use crate::supervisor::bus::BusResult;
use super::super::{Agent, AgentsState};
use super::core::ChatCore;

pub(crate) struct BasicChatPlugin;

impl Agent for BasicChatPlugin {
    fn id(&self) -> &str { "basic_chat" }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        _session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        // Spawn so the supervisor loop is not blocked on the LLM round-trip.
        tokio::spawn(async move {
            let result = ChatCore::basic_complete(&state, &channel_id, &content).await;
            let _ = reply_tx.send(result);
        });
    }
}
