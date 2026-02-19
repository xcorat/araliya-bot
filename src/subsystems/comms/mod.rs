//! Comms subsystem — manages all external I/O channels.
//!
//! # Architecture
//!
//! Each channel (PTY, HTTP, Telegram…) is an independent task spawned by
//! [`start`].  Channels share an `Arc<`[`CommsState`]`>` which provides:
//!
//! - A **typed outbound API** (`send_message`) — channels cannot address the
//!   supervisor bus directly.
//! - An **intra-subsystem event queue** (`report_event`) for signalling back
//!   to the subsystem manager (e.g. shutdown, session lifecycle).
//!
//! # Starting
//!
//! [`start`] is a synchronous function that spawns all channels immediately
//! and returns a [`SubsystemHandle`].  The caller can `.await` the handle to
//! block until all channels exit, or hold it and continue working
//! — channels run either way.

mod state;
pub mod pty;

pub use state::{CommsEvent, CommsState};

use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::config::Config;
use crate::error::AppError;
use crate::supervisor::bus::BusHandle;
use crate::subsystems::runtime::SubsystemHandle;

// ── Channel trait ───────────────────────────────────────────────────────────

/// A comms channel that runs as an independent task.
///
/// Implementors receive `Arc<CommsState>` at spawn time from the subsystem
/// manager — they do not store it themselves.  This keeps state ownership
/// clear: the subsystem creates and owns `CommsState`; channels only borrow
/// a reference-counted handle to it while they run.
pub trait Channel: Send + 'static {
    /// Stable identifier for this channel instance (e.g. `"pty0"`).
    fn id(&self) -> &str;

    /// Consume the channel and return its async run-loop as a boxed future.
    fn run(
        self: Box<Self>,
        state: Arc<CommsState>,
        shutdown: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>>;
}

// ── start ───────────────────────────────────────────────────────────────────

/// Spawn all configured comms channels and return a [`SubsystemHandle`].
///
/// Channels start immediately.  If any channel exits with an error, the
/// shared `shutdown` token is cancelled so siblings stop cooperatively.
/// The handle resolves when all channels have exited.
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
    let (event_tx, mut event_rx) = mpsc::channel::<CommsEvent>(32);
    let state = Arc::new(CommsState::new(bus, event_tx));

    // Build the channel list from config.
    let mut channels: Vec<Box<dyn Channel>> = Vec::new();

    if config.comms_pty_should_load() {
        info!("loading pty channel");
        channels.push(Box::new(pty::PtyChannel::new("pty0")));
    }

    let handle = tokio::spawn(async move {
        if channels.is_empty() {
            info!("no comms channels configured — waiting for shutdown");
            shutdown.cancelled().await;
            return Ok(());
        }

        let mut set: JoinSet<Result<(), AppError>> = JoinSet::new();

        for channel in channels {
            let id = channel.id().to_string();
            let state = state.clone();
            let shutdown = shutdown.clone();
            debug!(channel = %id, "spawning channel task");
            set.spawn(channel.run(state, shutdown));
        }

        let mut first_err: Option<AppError> = None;

        loop {
            tokio::select! {
                // A channel task finished.
                Some(res) = set.join_next() => {
                    match res {
                        Err(e) => {
                            error!("channel task panicked: {e}");
                            shutdown.cancel();
                            first_err.get_or_insert_with(|| AppError::Comms(format!("channel panicked: {e}")));
                        }
                        Ok(Err(e)) => {
                            error!("channel error: {e}");
                            shutdown.cancel();
                            first_err.get_or_insert(e);
                        }
                        Ok(Ok(())) => {}
                    }
                    if set.is_empty() { break; }
                }

                // An intra-subsystem event from a running channel.
                Some(event) = event_rx.recv() => {
                    match event {
                        CommsEvent::ChannelShutdown { ref channel_id } => {
                            debug!(channel_id, "channel reported shutdown");
                        }
                        CommsEvent::SessionStarted { ref channel_id } => {
                            debug!(channel_id, "channel session started");
                        }
                    }
                }

                else => break,
            }
        }

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    });

    SubsystemHandle::from_handle(handle)
}
