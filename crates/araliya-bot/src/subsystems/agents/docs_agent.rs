//! `docs_agent` — thin wrapper that re-registers [`DocsAgentPlugin`] under
//! agent ID `"docs_agent"` for the public-facing `/ui/docs` route.
//!
//! All logic is delegated to the existing docs agent; this wrapper only
//! overrides [`Agent::id()`].

use std::sync::Arc;

use tokio::sync::oneshot;

use super::docs::DocsAgentPlugin;
use super::{Agent, AgentsState};
use crate::supervisor::bus::BusResult;

pub(crate) struct DocsAgentWrapper {
    inner: DocsAgentPlugin,
}

impl DocsAgentWrapper {
    pub fn new() -> Self {
        Self {
            inner: DocsAgentPlugin,
        }
    }
}

impl Agent for DocsAgentWrapper {
    fn id(&self) -> &str {
        "docs_agent"
    }

    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        self.inner
            .handle(action, channel_id, content, session_id, reply_tx, state);
    }

    fn handle_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        self.inner
            .handle_stream(channel_id, content, session_id, reply_tx, state);
    }
}
