//! Supervisor — owns the event bus and routes messages between subsystems.

pub mod adapters;
pub mod bus;
pub mod component_info;
pub mod control;
pub mod dispatch;

use std::collections::HashMap;
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};

use bus::{BusError, BusMessage, ERR_METHOD_NOT_FOUND, SupervisorBus};
use component_info::ComponentInfo;
use control::{ControlCommand, ControlError, ControlMessage, ControlResponse, SupervisorControl};
use dispatch::BusHandler;

/// Run the supervisor message loop until `shutdown` is cancelled.
///
/// The supervisor is a pure router: it reads each message, determines the
/// target subsystem by the first `/`-delimited method segment, and hands
/// off ownership of `reply_tx` to the matching [`BusHandler`].
///
/// # Panics
///
/// Panics on startup if two handlers share the same prefix — a programming
/// error that must be caught before the process enters its run loop.
pub async fn run(
    mut bus: SupervisorBus,
    mut control: SupervisorControl,
    shutdown: CancellationToken,
    handlers: Vec<Box<dyn BusHandler>>,
) {
    // Build the dispatch table; panic on duplicate prefixes.
    let mut table: HashMap<String, Box<dyn BusHandler>> = HashMap::new();
    for h in handlers {
        let prefix = h.prefix().to_string();
        debug!(%prefix, "registering bus handler");
        if table.insert(prefix.clone(), h).is_some() {
            panic!("duplicate BusHandler prefix registered: {prefix:?}");
        }
    }

    info!(
        handlers = ?table.keys().collect::<Vec<_>>(),
        "supervisor ready"
    );

    let started_at = Instant::now();

    let sorted_handler_ids = |table: &HashMap<String, Box<dyn BusHandler>>| {
        let mut ids = table.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids
    };

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                info!("supervisor shutting down");
                break;
            }

            control_msg = control.rx.recv() => {
                match control_msg {
                    Some(ControlMessage::Request { command, reply_tx }) => {
                        let uptime_ms = started_at.elapsed().as_millis() as u64;
                        let result = match command {
                            ControlCommand::Health => {
                                Ok(ControlResponse::Health { uptime_ms })
                            }
                            ControlCommand::Status => {
                                Ok(ControlResponse::Status {
                                    uptime_ms,
                                    handlers: sorted_handler_ids(&table),
                                })
                            }
                            ControlCommand::SubsystemsList => {
                                Ok(ControlResponse::Subsystems {
                                    handlers: sorted_handler_ids(&table),
                                })
                            }
                            ControlCommand::ComponentTree => {
                                // Collect per-handler component info via direct fn call (not bus).
                                let mut children: Vec<ComponentInfo> = {
                                    let mut ids: Vec<&String> = table.keys().collect();
                                    ids.sort();
                                    ids.iter().map(|k| table[*k].component_info()).collect()
                                };
                                children.sort_by(|a, b| a.id.cmp(&b.id));
                                let root = ComponentInfo {
                                    id: "supervisor".to_string(),
                                    name: "Supervisor".to_string(),
                                    status: "running".to_string(),
                                    state: component_info::ComponentStatus::On,
                                    uptime_ms: Some(uptime_ms),
                                    children,
                                };
                                let tree_json = serde_json::to_string(&root).unwrap_or_else(|_| "{}".to_string());
                                Ok(ControlResponse::ComponentTree { tree_json })
                            }
                            ControlCommand::Shutdown => {
                                info!("control requested supervisor shutdown");
                                shutdown.cancel();
                                Ok(ControlResponse::Ack {
                                    message: "shutdown requested".to_string(),
                                })
                            }
                            ControlCommand::SubsystemEnable { id } => {
                                Err(ControlError::NotImplemented {
                                    message: format!("subsystem enable not implemented: {id}"),
                                })
                            }
                            ControlCommand::SubsystemDisable { id } => {
                                Err(ControlError::NotImplemented {
                                    message: format!("subsystem disable not implemented: {id}"),
                                })
                            }
                        };
                        let _ = reply_tx.send(result);
                    }
                    Some(ControlMessage::Notification { command }) => {
                        if matches!(command, ControlCommand::Shutdown) {
                            info!("control notification requested supervisor shutdown");
                            shutdown.cancel();
                        } else {
                            debug!(?command, "control notification ignored in MVP");
                        }
                    }
                    None => {
                        info!("control channel closed");
                    }
                }
            }

            msg = bus.rx.recv() => {
                match msg {
                    Some(BusMessage::Request { id, method, payload, reply_tx }) => {
                        let prefix = method.split('/').next().unwrap_or_default();
                        match table.get(prefix) {
                            Some(handler) => {
                                debug!(%id, %method, %prefix, "routing request");
                                trace!(%id, %method, payload = ?payload, "request payload");
                                handler.handle_request(&method, payload, reply_tx);
                            }
                            None => {
                                warn!(%id, %method, "unhandled request method — replying with error");
                                let _ = reply_tx.send(Err(BusError::new(
                                    ERR_METHOD_NOT_FOUND,
                                    format!("method not found: {method}"),
                                )));
                            }
                        }
                    }
                    Some(BusMessage::Notification { method, payload }) => {
                        let prefix = method.split('/').next().unwrap_or_default();
                        match table.get(prefix) {
                            Some(handler) => {
                                debug!(%method, "routing notification");
                                trace!(%method, payload = ?payload, "notification payload");
                                handler.handle_notification(&method, payload);
                            }
                            None => {
                                debug!(%method, "unhandled notification — no handler for prefix");
                            }
                        }
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

