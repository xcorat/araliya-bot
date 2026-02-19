//! Shared state for the Comms subsystem, passed as `Arc<CommsState>` to channels.

use tokio::sync::mpsc;

use crate::supervisor::bus::CommsMessage;

/// Shared state for comms channel tasks.
///
/// Each channel (PTY, HTTP, Telegram…) receives a clone of `Arc<CommsState>`
/// and uses `comms_tx` to send inbound messages to the supervisor for routing.
pub struct CommsState {
    /// Sender to submit inbound messages into the supervisor bus.
    pub comms_tx: mpsc::Sender<CommsMessage>,
}

impl CommsState {
    pub fn new(comms_tx: mpsc::Sender<CommsMessage>) -> Self {
        Self { comms_tx }
    }
}
