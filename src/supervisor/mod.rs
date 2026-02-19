//! Supervisor — owns the event bus and routes messages between subsystems.

pub mod bus;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::subsystems::agents::AgentsSubsystem;
use crate::subsystems::llm::LlmSubsystem;

use bus::{BusError, BusMessage, ERR_METHOD_NOT_FOUND, SupervisorBus};

/// Run the supervisor message loop until `shutdown` is cancelled.
///
/// The supervisor is a pure router: it reads each message, determines the
/// target subsystem, and hands off ownership of `reply_tx` to that subsystem.
/// It never awaits handler work — subsystems resolve the reply at their own
/// pace (immediately for sync handlers, via `tokio::spawn` for async work).
pub async fn run(mut bus: SupervisorBus, shutdown: CancellationToken, agents: AgentsSubsystem, llm: LlmSubsystem) {
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
                        match subsystem {
                            "agents" => {
                                debug!(%method, "routing to agents subsystem");
                                agents.handle_request(&method, payload, reply_tx);
                            }
                            "llm" => {
                                debug!(%method, "routing to llm subsystem");
                                llm.handle_request(&method, payload, reply_tx);
                            }
                            _ => {
                                warn!(%method, "unhandled request method");
                                let _ = reply_tx.send(Err(BusError::new(
                                    ERR_METHOD_NOT_FOUND,
                                    format!("method not found: {method}"),
                                )));
                            }
                        }
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

