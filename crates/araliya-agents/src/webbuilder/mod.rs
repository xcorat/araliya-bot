//! `webbuilder` agent plugin — iteratively builds static Svelte pages.
//!
//! The agent scaffolds a Vite + Svelte 5 workspace on first use, then enters
//! an LLM-driven loop: the LLM writes files and runs shell commands until
//! the page is built.  Progress events are streamed to the caller via SSE as
//! `>>STEP<<{...}` prefixed [`StreamChunk::Content`] messages.
//!
//! The built page is served at `/preview/{workspace_name}/` by the Axum
//! preview route.

use std::sync::Arc;

use tokio::sync::oneshot;

use araliya_core::bus::message::{BusPayload, BusResult, StreamReceiver};
#[cfg(feature = "plugin-homebuilder")]
use araliya_core::config::HomebuildAgentConfig;
use araliya_core::config::WebBuilderAgentConfig;
use araliya_llm::StreamChunk;

use super::{Agent, AgentsState};

mod loop_;
mod tools;
#[cfg(feature = "plugin-homebuilder")]
pub(crate) mod init_home;

// ── WebBuilderAgent ───────────────────────────────────────────────────────────

pub(crate) struct WebBuilderAgent {
    max_iterations: usize,
    theme_guides_dir: Option<std::path::PathBuf>,
}

impl WebBuilderAgent {
    pub fn new(cfg: &WebBuilderAgentConfig) -> Self {
        Self {
            max_iterations: cfg.max_iterations,
            theme_guides_dir: cfg.theme_guides_dir.clone(),
        }
    }
}

impl Agent for WebBuilderAgent {
    fn id(&self) -> &str {
        "webbuilder"
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
        let max_iterations = self.max_iterations;
        let theme_guides_dir = self.theme_guides_dir.clone();
        tokio::spawn(async move {
            // Run the streaming loop and collect the full response.
            let loop_ = loop_::WebBuilderLoop::new(max_iterations, theme_guides_dir);
            let stream_result = loop_
                .run_stream(channel_id.clone(), content, session_id.clone(), state)
                .await;

            // Drain the stream to collect the final content.
            let buffered = match stream_result {
                Ok(BusPayload::LlmStreamResult {
                    rx: StreamReceiver(mut rx),
                }) => {
                    let mut buf = String::new();
                    while let Some(chunk) = rx.recv().await {
                        match chunk {
                            StreamChunk::Content(delta) => buf.push_str(&delta),
                            StreamChunk::Thinking(_) => {}
                            StreamChunk::Done { .. } => break,
                        }
                    }
                    Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: buf,
                        session_id,
                        usage: None,
                        timing: None,
                        thinking: None,
                    })
                }
                Err(e) => Err(e),
                other => other,
            };
            let _ = reply_tx.send(buffered);
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
        let max_iterations = self.max_iterations;
        let theme_guides_dir = self.theme_guides_dir.clone();
        tokio::spawn(async move {
            let loop_ = loop_::WebBuilderLoop::new(max_iterations, theme_guides_dir);
            let result = loop_
                .run_stream(channel_id, content, session_id, state)
                .await;
            let _ = reply_tx.send(result);
        });
    }
}

// ── HomebuilderAgent ──────────────────────────────────────────────────────────

/// Singleton variant of [`WebBuilderAgent`] that generates the bot's landing
/// page and serves it at `/home/` (and `/preview/homebuilder/`).
#[cfg(feature = "plugin-homebuilder")]
pub(crate) struct HomebuilderAgent {
    user_name: String,
    notes_dir: Option<String>,
}

#[cfg(feature = "plugin-homebuilder")]
impl HomebuilderAgent {
    pub fn new(cfg: &HomebuildAgentConfig) -> Self {
        Self {
            user_name: cfg.user_name.clone(),
            notes_dir: cfg.notes_dir.as_ref().map(|p| p.to_string_lossy().into_owned()),
        }
    }
}

#[cfg(feature = "plugin-homebuilder")]
impl Agent for HomebuilderAgent {
    fn id(&self) -> &str {
        "homebuilder"
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
        let user_name = self.user_name.clone();
        let notes_dir = self.notes_dir.clone();

        tokio::spawn(async move {
            let loop_ = loop_::HomebuilderLoop::new(user_name, notes_dir);
            let stream_result = loop_
                .run_stream(channel_id.clone(), content, session_id.clone(), state)
                .await;

            let buffered = match stream_result {
                Ok(BusPayload::LlmStreamResult {
                    rx: StreamReceiver(mut rx),
                }) => {
                    let mut buf = String::new();
                    while let Some(chunk) = rx.recv().await {
                        match chunk {
                            StreamChunk::Content(delta) => buf.push_str(&delta),
                            StreamChunk::Thinking(_) => {}
                            StreamChunk::Done { .. } => break,
                        }
                    }
                    Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: buf,
                        session_id,
                        usage: None,
                        timing: None,
                        thinking: None,
                    })
                }
                Err(e) => Err(e),
                other => other,
            };
            let _ = reply_tx.send(buffered);
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
        let user_name = self.user_name.clone();
        let notes_dir = self.notes_dir.clone();

        tokio::spawn(async move {
            let loop_ = loop_::HomebuilderLoop::new(user_name, notes_dir);
            let result = loop_
                .run_stream(channel_id, content, session_id, state)
                .await;
            let _ = reply_tx.send(result);
        });
    }
}
