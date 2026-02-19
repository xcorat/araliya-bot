//! Supervisor — owns the event bus and routes messages between subsystems.
//!
//! Currently a stub: each inbound `CommsMessage` is echoed back as-is.
//! When the Agents subsystem is added, replace the `reply_tx.send(msg.content)`
//! line with a dispatch to the agent handler.

pub mod bus;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use bus::SupervisorBus;

/// Run the supervisor message loop until `shutdown` is cancelled.
pub async fn run(mut bus: SupervisorBus, shutdown: CancellationToken) {
    info!("supervisor ready");

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                info!("supervisor shutting down");
                break;
            }

            msg = bus.comms_rx.recv() => {
                match msg {
                    Some(m) => {
                        debug!(content = %m.content, "supervisor received message");
                        // Stub: echo the message back. Replace with agent dispatch later.
                        if m.reply_tx.send(m.content).is_err() {
                            warn!("comms channel dropped reply receiver before reply was sent");
                        }
                    }
                    None => {
                        info!("comms channel closed, supervisor exiting");
                        break;
                    }
                }
            }
        }
    }
}
