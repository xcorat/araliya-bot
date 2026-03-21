//! Axum-based HTTP channel — serves API endpoints under `/api/` and
//! delegates all other paths to the UI backend when available.

mod api;
#[cfg(feature = "plugin-webbuilder")]
mod preview;
mod ui;

use std::sync::Arc;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

use araliya_core::error::AppError;
use araliya_core::runtime::{Component, ComponentFuture};
#[cfg(feature = "subsystem-ui")]
use araliya_core::ui::UiServeHandle;

use crate::state::CommsState;

// ── Type alias (mirrors http/mod.rs pattern) ──────────────────────────────────

#[cfg(feature = "subsystem-ui")]
pub(crate) type OptionalUiHandle = Option<UiServeHandle>;
#[cfg(not(feature = "subsystem-ui"))]
pub(crate) type OptionalUiHandle = Option<()>;

// ── Shared request state ──────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct AxumState {
    pub channel_id: Arc<str>,
    pub comms: Arc<CommsState>,
    pub ui: OptionalUiHandle,
    #[cfg(feature = "plugin-webbuilder")]
    pub preview_root: Option<std::path::PathBuf>,
}

// ── AxumChannel ───────────────────────────────────────────────────────────────

pub struct AxumChannel {
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
    ui_handle: OptionalUiHandle,
    #[cfg(feature = "plugin-webbuilder")]
    preview_root: Option<std::path::PathBuf>,
}

impl AxumChannel {
    pub fn new(
        channel_id: impl Into<String>,
        bind_addr: impl Into<String>,
        state: Arc<CommsState>,
        ui_handle: OptionalUiHandle,
        #[cfg(feature = "plugin-webbuilder")] preview_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            bind_addr: bind_addr.into(),
            state,
            ui_handle,
            #[cfg(feature = "plugin-webbuilder")]
            preview_root,
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
            #[cfg(feature = "plugin-webbuilder")]
            self.preview_root,
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
    #[cfg(feature = "plugin-webbuilder")] preview_root: Option<std::path::PathBuf>,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let axum_state = AxumState {
        channel_id: Arc::from(channel_id.as_str()),
        comms,
        ui: ui_handle,
        #[cfg(feature = "plugin-webbuilder")]
        preview_root,
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
    let router = Router::new()
        .route("/api/health", get(api::health))
        .route("/api/health/refresh", post(api::health_refresh))
        .route("/api/tree", get(api::tree))
        .route("/api/message", post(api::message))
        .route("/api/message/stream", post(api::message_stream))
        .route("/api/sessions", get(api::sessions))
        .route("/api/agents", get(api::agents))
        .route("/api/agents/{agent_id}/session", get(api::agent_session))
        .route("/api/agents/{agent_id}/spend", get(api::agent_spend))
        .route("/api/agents/{agent_id}/kg", get(api::agent_kg))
        .route(
            "/api/memory/agents/{agent_id}/kg",
            get(api::memory_agent_kg),
        )
        .route(
            "/api/sessions/{session_id}/memory",
            get(api::session_memory),
        )
        .route("/api/sessions/{session_id}/debug", get(api::session_debug))
        .route("/api/sessions/{session_id}/files", get(api::session_files))
        .route("/api/session/{session_id}", get(api::session_detail))
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .route("/", get(ui::root))
        .route("/{*path}", get(ui::serve_path));

    #[cfg(feature = "plugin-webbuilder")]
    let router = router
        .route("/preview/{session_id}", get(preview::preview_index_handler))
        .route(
            "/preview/{session_id}/{*path}",
            get(preview::preview_handler),
        );

    router.with_state(state)
}
