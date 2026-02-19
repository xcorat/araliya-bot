//! Supervisor — owns the event bus and routes messages between subsystems.

pub mod bus;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::subsystems::agents::AgentsSubsystem;

use bus::{BusError, BusMessage, ERR_METHOD_NOT_FOUND, SupervisorBus};

/// Run the supervisor message loop until `shutdown` is cancelled.
pub async fn run(mut bus: SupervisorBus, shutdown: CancellationToken, agents: AgentsSubsystem) {
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
                    Some(BusMessage::Request { method, payload, reply_tx, .. }) => {
                        let subsystem = method.split('/').next().unwrap_or_default();
                        let result = match subsystem {
                            "agents" => {
                                debug!(%method, "routing request to agents subsystem");
                                agents.handle_request(&method, payload)
                            }
                            _ => {
                                warn!(%method, "unhandled request method");
                                Err(BusError::new(
                                    ERR_METHOD_NOT_FOUND,
                                    format!("method not found: {method}"),
                                ))
                            }
                        };
                        let _ = reply_tx.send(result);
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
