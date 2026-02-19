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
#[cfg(feature = "channel-telegram")]
pub mod telegram;

pub use state::{CommsEvent, CommsState};

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::config::Config;
use crate::supervisor::bus::BusHandle;
use crate::subsystems::runtime::{Component, SubsystemHandle, spawn_components};

// ── start ───────────────────────────────────────────────────────────────────

/// Spawn all configured comms channels and return a [`SubsystemHandle`].
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
        if config.comms_pty_should_load() {
            info!("loading pty channel");
            components.push(Box::new(pty::PtyChannel::new("pty0", state.clone())));
        }
    }

    #[cfg(feature = "channel-telegram")]
    {
        if config.comms_telegram_should_load() {
            info!("loading telegram channel");
            components.push(Box::new(telegram::TelegramChannel::new("telegram0", state.clone())));
        }
    }

    if components.is_empty() {
        info!("no comms channels configured — waiting for shutdown");
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
