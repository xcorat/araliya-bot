//! Comms subsystem — manages all external I/O channels.
//!
//! Channels (PTY, HTTP, Telegram…) are spawned as tasks and given shared
//! access to `CommsState` for routing messages through the supervisor bus.

mod state;
pub mod pty;

pub use state::CommsState;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::Config;
use crate::error::AppError;
use crate::supervisor::bus::BusHandle;

/// Start comms channels according to `config`, run until shutdown.
///
/// `bus` is a cloned handle to the supervisor bus, distributed before the
/// supervisor task is spawned.
pub async fn run(
    config: &Config,
    bus: BusHandle,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let state = Arc::new(CommsState::new(bus));

    if config.comms_pty_should_load() {
        info!("loading pty channel");
        pty::run(state, shutdown).await?;
    } else {
        info!("no comms channels configured — waiting for shutdown");
        shutdown.cancelled().await;
    }

    Ok(())
}
