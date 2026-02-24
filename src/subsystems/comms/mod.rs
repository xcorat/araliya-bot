//! Comms subsystem — manages all external I/O channels.
//!
//! # Architecture
//!
//! Each channel (PTY, HTTP, Telegram…) implements [`runtime::Component`] and
//! is spawned as an independent concurrent task by [`start`] via
//! [`runtime::spawn_components`].  Channels capture their shared
//! [`Arc<CommsState>`] at construction time — no state is passed through the
//! generic `Component::run` signature.
//!
//! An intra-subsystem [`mpsc`] channel lets running channels signal the
//! comms manager (lifecycle events, session tracking).  This is drained in a
//! short-lived background task that dies naturally when all channel senders
//! are dropped.
//!
//! # Starting
//!
//! [`start`] is synchronous — it returns a [`SubsystemHandle`] as soon as
//! the tasks are spawned.  The caller decides when (or whether) to await it.

mod state;
pub mod pty;
pub mod http;
#[cfg(feature = "channel-axum")]
pub mod axum_channel;
#[cfg(feature = "channel-telegram")]
pub mod telegram;

pub use state::{CommsEvent, CommsState};

use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[cfg(feature = "subsystem-ui")]
use crate::subsystems::ui::UiServeHandle;

use crate::config::Config;
use crate::supervisor::bus::BusHandle;
use crate::supervisor::component_info::ComponentInfo;
use crate::subsystems::runtime::{Component, SubsystemHandle, spawn_components};

// ── start ───────────────────────────────────────────────────────────────────

/// Spawn all configured comms channels and return a [`SubsystemHandle`].
///
/// When a UI backend is active, its [`UiServeHandle`] is passed to the HTTP
/// channel so non-API requests can be served by the UI subsystem.
///
/// Channels start immediately.  If any channel exits with an error the shared
/// `shutdown` token is cancelled so siblings stop cooperatively.  The handle
/// resolves when all channels have exited.
///
/// # Non-blocking
///
/// This function is synchronous — it returns as soon as the tasks are
/// spawned.  The caller decides when (or whether) to await the handle.
pub fn start(
    config: &Config,
    bus: BusHandle,
    shutdown: CancellationToken,
    #[cfg(feature = "subsystem-ui")] ui_handle: Option<UiServeHandle>,
    comms_info: Arc<OnceLock<ComponentInfo>>,
) -> SubsystemHandle {
    // Intra-subsystem event channel: channels → manager.
    let (event_tx, event_rx) = mpsc::channel::<CommsEvent>(32);
    let state = Arc::new(CommsState::new(bus, event_tx));

    // Build the component list from config.
    // Each channel captures Arc<CommsState> at construction; the generic
    // Component::run signature only needs the shutdown token.
    let mut components: Vec<Box<dyn Component>> = Vec::new();

    #[cfg(feature = "channel-pty")]
    {
        let pty_requested = config.comms_pty_should_load();
        let stdio_managed = crate::supervisor::adapters::stdio::stdio_control_active();

        if pty_requested && !stdio_managed {
            info!("loading pty channel");
            components.push(Box::new(pty::PtyChannel::new("pty0", state.clone())));
        } else if pty_requested && stdio_managed {
            info!("pty channel disabled: stdio management adapter is active (virtual /chat route enabled)");
        }
    }

    #[cfg(feature = "channel-telegram")]
    {
        if config.comms_telegram_should_load() {
            info!("loading telegram channel");
            components.push(Box::new(telegram::TelegramChannel::new("telegram0", state.clone())));
        }
    }

    #[cfg(feature = "channel-http")]
    {
        if config.comms_http_should_load() {
            info!(bind = %config.comms.http.bind, "loading http channel");
            // If the UI subsystem is active, hand the serve handle to the
            // HTTP channel so non-API paths are served by the UI backend.
            #[cfg(feature = "subsystem-ui")]
            let ui = ui_handle.clone();
            #[cfg(not(feature = "subsystem-ui"))]
            let ui: Option<()> = None;
            components.push(Box::new(http::HttpChannel::new(
                "http0",
                config.comms.http.bind.clone(),
                state.clone(),
                ui,
            )));
        }
    }
    // Config requests the HTTP channel but the feature was not compiled in.
    #[cfg(not(feature = "channel-http"))]
    if config.comms_http_should_load() {
        tracing::warn!(
            "config has [comms.http] enabled = true but this binary was compiled \
             without the `channel-http` feature — channel will not start. \
             Rebuild with `--features channel-http` or set enabled = false."
        );
    }

    #[cfg(feature = "channel-axum")]
    {
        if config.comms_axum_should_load() {
            info!(bind = %config.comms.axum_channel.bind, "loading axum channel");
            #[cfg(feature = "subsystem-ui")]
            let ui = ui_handle.clone();
            #[cfg(not(feature = "subsystem-ui"))]
            let ui: Option<()> = None;
            components.push(Box::new(axum_channel::AxumChannel::new(
                "axum0",
                config.comms.axum_channel.bind.clone(),
                state.clone(),
                ui,
            )));
        }
    }
    // Config requests the axum channel but the feature was not compiled in.
    #[cfg(not(feature = "channel-axum"))]
    if config.comms_axum_should_load() {
        tracing::warn!(
            "config has [comms.axum_channel] enabled = true but this binary was compiled \
             without the `channel-axum` feature — channel will not start. \
             Rebuild with `--features channel-axum` or set enabled = false."
        );
    }

    if components.is_empty() {
        info!("no comms channels configured — waiting for shutdown");
    }

    // Snapshot the channel list into the component-info slot for the management tree.
    // Each Component exposes its id(); we use that as both the node id and display name.
    {
        let channel_children: Vec<ComponentInfo> = components
            .iter()
            .map(|c| ComponentInfo::leaf(c.id(), &ComponentInfo::capitalise(c.id())))
            .collect();
        let _ = comms_info.set(ComponentInfo::running("comms", "Comms", channel_children));
    }

    // Spawn a background event drain: consumes CommsEvent until all channel
    // senders are dropped (i.e. all channels have exited).  Errors are
    // non-fatal — this task is monitoring-only and does not affect lifecycle.
    tokio::spawn(async move {
        let mut rx = event_rx;
        while let Some(event) = rx.recv().await {
            match event {
                CommsEvent::ChannelShutdown { ref channel_id } => {
                    debug!(channel_id, "channel reported shutdown");
                }
                CommsEvent::SessionStarted { ref channel_id } => {
                    debug!(channel_id, "channel session started");
                }
            }
        }
    });

    // Delegate component lifecycle (JoinSet + error propagation + shutdown
    // cancellation) entirely to the generic runtime helper.
    spawn_components(components, shutdown)
}
