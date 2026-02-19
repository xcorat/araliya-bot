//! Supervisor event bus — typed channel pairs between comms and supervisor.

use tokio::sync::{mpsc, oneshot};

/// A message from a comms channel, with a reply slot for the response.
pub struct CommsMessage {
    /// The raw message content received from the channel.
    pub content: String,
    /// Send the reply back through this sender.
    pub reply_tx: oneshot::Sender<String>,
}

/// Owns the supervisor-side channel ends.
pub struct SupervisorBus {
    /// Supervisor receives inbound messages here.
    pub comms_rx: mpsc::Receiver<CommsMessage>,
    /// Cloneable sender given to comms channels to submit messages.
    pub comms_tx: mpsc::Sender<CommsMessage>,
}

impl SupervisorBus {
    pub fn new(buffer: usize) -> Self {
        let (comms_tx, comms_rx) = mpsc::channel(buffer);
        Self { comms_rx, comms_tx }
    }
}
