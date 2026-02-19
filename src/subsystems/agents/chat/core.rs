//! Shared chat logic used by all chat-family plugins.
//!
//! [`ChatCore`] provides composable building blocks so that
//! `BasicChatPlugin`, `SessionChatPlugin`, and future chat variants
//! can share common behaviour without duplicating code.

use std::sync::Arc;

use crate::supervisor::bus::BusResult;
use crate::subsystems::agents::AgentsState;

/// Reusable core for chat-family plugins.
///
/// Holds no state of its own today — it operates on the shared
/// [`AgentsState`] passed into each call.  Future additions (prompt
/// templates, memory, tool dispatch) will live here.
pub struct ChatCore;

impl ChatCore {
    /// Simple one-shot completion: forward content to the LLM and return the
    /// result.  This is the primitive both `basic_chat` and `session_chat`
    /// build on top of.
    pub async fn basic_complete(
        state: &Arc<AgentsState>,
        channel_id: &str,
        content: &str,
    ) -> BusResult {
        state.complete_via_llm(channel_id, content).await
    }
}
