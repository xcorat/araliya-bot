//! Shared state for the Comms subsystem, passed as `Arc<CommsState>` to channels.

use crate::supervisor::bus::BusHandle;

/// Shared state for comms channel tasks.
///
/// Each channel (PTY, HTTP, Telegram…) receives a clone of `Arc<CommsState>`
/// and uses `bus` to send messages to the supervisor for routing.
pub struct CommsState {
    /// Handle to the supervisor bus.
    pub bus: BusHandle,
}

impl CommsState {
    pub fn new(bus: BusHandle) -> Self {
        Self { bus }
    }
}
