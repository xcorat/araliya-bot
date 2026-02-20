//! Supervisor transport adapters.
//!
//! Transport adapters are internal to the supervisor and translate external
//! I/O protocols (stdio, HTTP) into supervisor control or bus calls.

pub mod stdio;

use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::supervisor::bus::BusHandle;
use crate::supervisor::control::ControlHandle;

/// Start supervisor-owned transport adapters.
pub fn start(control: ControlHandle, bus: BusHandle, shutdown: CancellationToken) {
    stdio::start(control.clone(), bus, shutdown.clone());
    start_http_adapter(control, shutdown);
}

fn start_http_adapter(_control: ControlHandle, _shutdown: CancellationToken) {
    info!("supervisor http adapter: stub (not yet enabled)");
}
