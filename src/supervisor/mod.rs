//! Supervisor — owns the event bus and routes messages between subsystems.

pub mod bus;
pub mod dispatch;

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use bus::{BusError, BusMessage, ERR_METHOD_NOT_FOUND, SupervisorBus};
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
    shutdown: CancellationToken,
    handlers: Vec<Box<dyn BusHandler>>,
) {
    // Build the dispatch table; panic on duplicate prefixes.
    let mut table: HashMap<String, Box<dyn BusHandler>> = HashMap::new();
    for h in handlers {
        let prefix = h.prefix().to_string();
        if table.insert(prefix.clone(), h).is_some() {
            panic!("duplicate BusHandler prefix registered: {prefix:?}");
        }
    }

    info!(
        handlers = ?table.keys().collect::<Vec<_>>(),
        "supervisor ready"
    );

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
                        let prefix = method.split('/').next().unwrap_or_default();
                        match table.get(prefix) {
                            Some(handler) => {
                                debug!(%method, %prefix, "routing request");
                                handler.handle_request(&method, payload, reply_tx);
                            }
                            None => {
                                warn!(%method, "unhandled request method");
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
                                handler.handle_notification(&method, payload);
                            }
                            None => {
                                debug!(%method, "unhandled notification");
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

