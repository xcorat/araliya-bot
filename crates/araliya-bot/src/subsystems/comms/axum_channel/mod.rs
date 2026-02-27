//! Axum-based HTTP channel — serves API endpoints under `/api/` and
//! delegates all other paths to the UI backend when available.
//!
//! This channel is a drop-in replacement for [`super::http::HttpChannel`]
//! that uses axum/hyper instead of a hand-rolled HTTP parser.  It implements
//! [`Component`] so it slots into the existing comms subsystem lifecycle:
//! `run()` drives the axum event loop; the existing [`CancellationToken`]
//! is wired to axum's graceful shutdown.
//!
//! ## URL layout (identical to channel-http)
//!
//! ```text
//! GET  /api/health
//! GET  /api/tree   — component tree (no private data)
//! POST /api/message
//! GET  /api/sessions
//! GET  /api/sessions/{id}/memory
//! GET  /api/sessions/{id}/files
//! GET  /api/session/{id}
//! GET  /favicon.ico              → 204
//! GET  /                         → root HTML
//! GET  /*path                    → UI backend (SPA fallback)
//! ```

mod api;
mod ui;

use std::sync::Arc;

use axum::{Router, routing::{get, post}, http::StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::error::AppError;
use crate::subsystems::runtime::{Component, ComponentFuture};
#[cfg(feature = "subsystem-ui")]
use crate::subsystems::ui::UiServeHandle;

use super::state::CommsState;

// ── Type alias (mirrors http/mod.rs pattern) ──────────────────────────────────

#[cfg(feature = "subsystem-ui")]
pub(crate) type OptionalUiHandle = Option<UiServeHandle>;
#[cfg(not(feature = "subsystem-ui"))]
pub(crate) type OptionalUiHandle = Option<()>;

// ── Shared request state ──────────────────────────────────────────────────────

/// Axum router state injected into every handler via [`axum::extract::State`].
///
/// Cheap to clone — all fields are reference-counted.
#[derive(Clone)]
pub(crate) struct AxumState {
    /// Channel identifier used in log spans.
    pub channel_id: Arc<str>,
    /// Comms subsystem capabilities (message routing, session queries).
    pub comms: Arc<CommsState>,
    /// Optional UI backend for static-file / SPA serving.
    pub ui: OptionalUiHandle,
}

// ── AxumChannel ───────────────────────────────────────────────────────────────

pub struct AxumChannel {
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
    ui_handle: OptionalUiHandle,
}

impl AxumChannel {
    pub fn new(
        channel_id: impl Into<String>,
        bind_addr: impl Into<String>,
        state: Arc<CommsState>,
        ui_handle: OptionalUiHandle,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            bind_addr: bind_addr.into(),
            state,
            ui_handle,
        }
    }
}

impl Component for AxumChannel {
    fn id(&self) -> &str {
        &self.channel_id
    }

    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture {
        Box::pin(run_axum(
            self.channel_id,
            self.bind_addr,
            self.state,
            self.ui_handle,
            shutdown,
        ))
    }
}

// ── Server loop ───────────────────────────────────────────────────────────────

async fn run_axum(
    channel_id: String,
    bind_addr: String,
    comms: Arc<CommsState>,
    ui_handle: OptionalUiHandle,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let axum_state = AxumState {
        channel_id: Arc::from(channel_id.as_str()),
        comms,
        ui: ui_handle,
    };

    let router = build_router(axum_state);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| AppError::Comms(format!("axum bind failed on {bind_addr}: {e}")))?;

    info!(%channel_id, %bind_addr, "axum channel listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
        .map_err(|e| AppError::Comms(format!("axum server error: {e}")))?;

    info!(%channel_id, "axum channel shut down");
    Ok(())
}

// ── Router ────────────────────────────────────────────────────────────────────

fn build_router(state: AxumState) -> Router {
    Router::new()
        // API routes
        .route("/api/health",                        get(api::health))
        .route("/api/health/refresh",                post(api::health_refresh))
        .route("/api/tree",                           get(api::tree))
        .route("/api/message",                       post(api::message))
        .route("/api/sessions",                      get(api::sessions))
        .route("/api/agents",                        get(api::agents))
        .route("/api/agents/{agent_id}/kg",          get(api::agent_kg))
        .route("/api/sessions/{session_id}/memory",  get(api::session_memory))
        .route("/api/sessions/{session_id}/files",   get(api::session_files))
        .route("/api/session/{session_id}",          get(api::session_detail))
        // UI routes
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .route("/",            get(ui::root))
        .route("/{*path}",     get(ui::serve_path))
        .with_state(state)
}
