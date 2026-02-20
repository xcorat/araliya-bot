//! UI subsystem — manages display-oriented interface providers.
//!
//! # Architecture
//!
//! Unlike comms or agents, the UI subsystem does **not** run independent
//! tasks.  Instead it constructs a [`UiServeHandle`] — a trait-object that
//! comms channels (currently the HTTP channel) call synchronously to serve
//! static assets or rendered pages.
//!
//! Each UI backend (e.g. *svui*) implements [`UiServe`] and is selected at
//! startup based on config.  Only one backend is active at a time.
//!
//! # Starting
//!
//! [`start`] inspects config, builds the active backend (if any), and returns
//! an `Option<UiServeHandle>`.  The caller passes this handle to the comms
//! subsystem so the HTTP channel can delegate non-API requests to it.

#[cfg(feature = "ui-svui")]
pub mod svui;

use std::sync::Arc;

use tracing::info;

use crate::config::Config;

// ── UiServe trait ─────────────────────────────────────────────────────────────

/// Response returned by [`UiServe::serve`].
pub struct ServeResponse {
    /// HTTP status line, e.g. `"200 OK"` or `"404 Not Found"`.
    pub status: &'static str,
    /// MIME content type, e.g. `"text/html; charset=utf-8"`.
    pub content_type: &'static str,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// A UI backend that can serve HTTP requests for static assets / pages.
///
/// Implementations capture their configuration (static dir, etc.) at
/// construction time.  [`UiServe::serve`] is called per-request by the HTTP
/// channel and must be cheap (no async, no bus round-trips).
pub trait UiServe: Send + Sync {
    /// Serve a request for `path` (e.g. `"/"`, `"/index.html"`, `"/assets/app.js"`).
    ///
    /// Returns `Some(response)` if this backend handles the path, or `None`
    /// to let the caller fall through to its own 404.
    fn serve(&self, path: &str) -> Option<ServeResponse>;
}

/// Shared handle passed to comms channels.
pub type UiServeHandle = Arc<dyn UiServe>;

// ── start ─────────────────────────────────────────────────────────────────────

/// Build the configured UI backend and return its serve handle.
///
/// Returns `None` if no UI backend is enabled.
pub fn start(config: &Config) -> Option<UiServeHandle> {
    #[cfg(feature = "ui-svui")]
    {
        if config.ui_svui_should_load() {
            let static_dir = config.ui.svui.static_dir.clone();
            let backend = svui::SvuiBackend::new(static_dir);
            info!("ui: svui backend loaded");
            return Some(Arc::new(backend));
        }
    }

    let _ = config; // suppress unused warning when no UI features are compiled
    info!("ui: no backend enabled");
    None
}
