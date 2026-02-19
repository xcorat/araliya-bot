//! Supervisor — owns the event bus and routes messages between subsystems.
//!
//! Currently a stub: each inbound `CommsMessage` is echoed back as-is.
//! When the Agents subsystem is added, replace the `reply_tx.send(msg.content)`
//! line with a dispatch to the agent handler.

pub mod bus;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use bus::{BusError, BusMessage, BusPayload, ERR_METHOD_NOT_FOUND, SupervisorBus};

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

            msg = bus.rx.recv() => {
                match msg {
                    Some(BusMessage::Request { method, payload: BusPayload::CommsMessage { channel_id, content }, reply_tx, .. }) if method == "comms/pty/rx" => {
                        debug!(%channel_id, content = %content, "supervisor received comms message");
                        // Stub: echo back. Replace with agent dispatch later.
                        let _ = reply_tx.send(Ok(BusPayload::CommsMessage { channel_id, content }));
                    }
                    Some(BusMessage::Request { method, reply_tx, .. }) => {
                        warn!(%method, "unhandled request method");
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_METHOD_NOT_FOUND,
                            format!("method not found: {method}"),
                        )));
                    }
                    Some(BusMessage::Notification { method, .. }) => {
                        debug!(%method, "notification received");
                    }
                    None => {
                        info!("bus closed, supervisor exiting");
                        break;
                    }
                }
            }
        }
    }
}
