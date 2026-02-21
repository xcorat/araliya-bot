//! Supervisor transport adapters.
//!
//! Transport adapters are internal to the supervisor and translate external
//! I/O protocols (stdio, HTTP) into supervisor control or bus calls.

pub mod stdio;
#[cfg(unix)]
pub mod uds;

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;

use crate::supervisor::bus::BusHandle;
use crate::supervisor::control::ControlHandle;

/// Start supervisor-owned transport adapters.
pub fn start(
    control: ControlHandle,
    bus: BusHandle,
    shutdown: CancellationToken,
    interactive_enabled: bool,
    socket_path: PathBuf,
) {
    stdio::start(control.clone(), bus, shutdown.clone(), interactive_enabled);

    #[cfg(unix)]
    uds::start(control, socket_path, shutdown);

    #[cfg(not(unix))]
    {
        let _ = (control, socket_path, shutdown);
    }
}
