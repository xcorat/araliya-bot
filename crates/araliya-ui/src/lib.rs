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

use araliya_core::config::Config;
pub use araliya_core::ui::{ServeResponse, UiServe, UiServeHandle};

// ── start ─────────────────────────────────────────────────────────────────────

/// Build the configured UI backend and return its serve handle.
///
/// Returns `None` if no UI backend is enabled.
pub fn start(config: &Config) -> Option<UiServeHandle> {
    #[cfg(feature = "ui-svui")]
    {
        if config.ui_svui_should_load() {
            let static_dir = config.ui.svui.static_dir.clone();
            let backend = svui::SvuiBackend::new(static_dir, Some("/ui".to_owned()));
            info!("ui: svui backend loaded");
            return Some(Arc::new(backend));
        }
    }

    let _ = config; // suppress unused warning when no UI features are compiled
    info!("ui: no backend enabled");
    None
}
