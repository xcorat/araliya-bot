//! Thin management adapter boundary.
//!
//! This module is the attachment point for external control transports that
//! target the supervisor-internal control plane (`supervisor::control`).

use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::supervisor::control::ControlHandle;

/// Start management transport adapters.
///
/// For now these are intentionally thin stubs: wiring exists, but concrete
/// stdio/http protocol handling is added in follow-up changes.
pub fn start(control: ControlHandle, shutdown: CancellationToken) {
    start_stdio_adapter(control.clone(), shutdown.clone());
    start_http_adapter(control, shutdown);
}

fn start_stdio_adapter(_control: ControlHandle, _shutdown: CancellationToken) {
    info!("management stdio adapter: stub (not yet enabled)");
}

fn start_http_adapter(_control: ControlHandle, _shutdown: CancellationToken) {
    info!("management http adapter: stub (not yet enabled)");
}
