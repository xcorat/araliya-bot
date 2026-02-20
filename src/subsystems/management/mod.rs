//! Management subsystem — supervisor-facing management bus handler.
//!
//! Exposes `manage/http/get` on the supervisor bus. The HTTP comms channel
//! uses this route to fetch management health/status data via the supervisor
//! control plane.

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::control::{ControlCommand, ControlHandle, ControlResponse};
use crate::supervisor::dispatch::BusHandler;

pub struct ManagementSubsystem {
    control: ControlHandle,
}

impl ManagementSubsystem {
    pub fn new(control: ControlHandle) -> Self {
        Self { control }
    }
}

impl BusHandler for ManagementSubsystem {
    fn prefix(&self) -> &str {
        "manage"
    }

    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        if method != "manage/http/get" {
            let _ = reply_tx.send(Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("method not found: {method}"),
            )));
            return;
        }

        match payload {
            BusPayload::Empty => {}
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unsupported payload for method: {method}"),
                )));
                return;
            }
        }

        let control = self.control.clone();
        tokio::spawn(async move {
            let result = match control.request(ControlCommand::Health).await {
                Ok(Ok(ControlResponse::Health { uptime_ms })) => {
                    Ok(BusPayload::CommsMessage {
                        channel_id: "manage-http".to_string(),
                        content: format!("{{\"status\":\"ok\",\"uptime_ms\":{uptime_ms}}}"),
                    })
                }
                Ok(Ok(_)) => Err(BusError::new(
                    -32000,
                    "unexpected control response for health",
                )),
                Ok(Err(e)) => Err(BusError::new(
                    -32000,
                    format!("control error: {e:?}"),
                )),
                Err(e) => Err(BusError::new(
                    -32000,
                    format!("control transport error: {e}"),
                )),
            };

            let _ = reply_tx.send(result);
        });
    }
}

