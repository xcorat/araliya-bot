//! Backward-compatible forwarding module.
//!
//! Adapter implementations are now supervisor-internal (`supervisor::adapters`).

use tokio_util::sync::CancellationToken;

use crate::supervisor::bus::BusHandle;
use crate::supervisor::control::ControlHandle;

pub use crate::supervisor::adapters::stdio::stdio_control_active;

pub fn start(control: ControlHandle, bus: BusHandle, shutdown: CancellationToken) {
    crate::supervisor::adapters::start(control, bus, shutdown);
}
