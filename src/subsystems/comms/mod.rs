//! Comms subsystem — manages all external I/O channels.
//!
//! Channels (PTY, HTTP, Telegram…) are spawned as tasks and given shared
//! access to `CommsState` for routing messages through the supervisor bus.

mod state;
pub mod pty;

pub use state::CommsState;

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::Config;
use crate::error::AppError;
use crate::supervisor::bus::CommsMessage;

/// Start comms channels according to `config`, run until shutdown.
///
/// `comms_tx` is the sender side of the supervisor bus — pre-cloned by main
/// before the bus receiver is moved into the supervisor task.
pub async fn run(
    config: &Config,
    comms_tx: mpsc::Sender<CommsMessage>,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let state = Arc::new(CommsState::new(comms_tx));

    if config.comms_pty_should_load() {
        info!("loading pty channel");
        pty::run(state, shutdown).await?;
    } else {
        info!("no comms channels configured — waiting for shutdown");
        shutdown.cancelled().await;
    }

    Ok(())
}
