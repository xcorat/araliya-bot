//! Shared state for the Comms subsystem — capability boundary for channels.
//!
//! Channels receive an `Arc<CommsState>` and are restricted to the typed
//! methods below.  The raw [`BusHandle`] is private; channels cannot call
//! arbitrary bus methods or supervisor internals.
//!
//! # Intra-subsystem events
//!
//! [`CommsState::report_event`] lets a running channel signal the comms
//! subsystem manager (e.g. "I shut down", "new session started") without
//! going through the supervisor bus.  The manager owns the receiver end.

use tokio::sync::mpsc;
use tracing::warn;

use crate::error::AppError;
use crate::supervisor::bus::{BusHandle, BusPayload};

// ── Events ────────────────────────────────────────────────────────────────────

/// Events a channel sends back to the comms subsystem manager.
#[derive(Debug)]
pub enum CommsEvent {
    /// Channel has stopped (clean exit or EOF).
    ChannelShutdown { channel_id: String },
    /// A new session/connection was established on the channel.
    SessionStarted { channel_id: String },
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Shared state passed as `Arc<CommsState>` to every channel task.
pub struct CommsState {
    /// Supervisor bus — private so channels can't call arbitrary methods.
    bus: BusHandle,
    /// Back-channel to the comms subsystem manager.
    event_tx: mpsc::Sender<CommsEvent>,
}

impl CommsState {
    pub fn new(bus: BusHandle, event_tx: mpsc::Sender<CommsEvent>) -> Self {
        Self { bus, event_tx }
    }

    /// Send `content` from `channel_id` to the agents subsystem and await the
    /// reply string.
    ///
    /// This is the primary outbound path for all comms channels.  Channels
    /// do not need to know about the supervisor bus protocol.
    pub async fn send_message(
        &self,
        channel_id: &str,
        content: String,
    ) -> Result<String, AppError> {
        let payload = BusPayload::CommsMessage {
            channel_id: channel_id.to_string(),
            content,
        };

        match self.bus.request("agents", payload).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agent error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content: reply, .. })) => Ok(reply),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Report an event to the comms subsystem manager.
    ///
    /// Non-blocking: drops the event and logs a warning if the manager is not
    /// keeping up (channel full) or has already exited (closed).
    pub fn report_event(&self, event: CommsEvent) {
        if let Err(e) = self.event_tx.try_send(event) {
            warn!("comms event dropped: {e}");
        }
    }
}
