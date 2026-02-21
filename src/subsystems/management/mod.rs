//! Management subsystem — supervisor-facing management bus handler.
//!
//! Exposes `manage/http/get` on the supervisor bus. The HTTP comms channel
//! uses this route to fetch management health/status data via the supervisor
//! control plane.

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::control::{ControlCommand, ControlHandle, ControlResponse};
use crate::supervisor::dispatch::BusHandler;

/// Static info collected at startup and included in the health response.
#[derive(Debug, Clone)]
pub struct ManagementInfo {
    pub bot_id: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_timeout_seconds: u64,
}

pub struct ManagementSubsystem {
    control: ControlHandle,
    bus: BusHandle,
    info: ManagementInfo,
}

impl ManagementSubsystem {
    pub fn new(control: ControlHandle, bus: BusHandle, info: ManagementInfo) -> Self {
        Self { control, bus, info }
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
        let bus = self.bus.clone();
        let info = self.info.clone();
        tokio::spawn(async move {
            let result = match control.request(ControlCommand::Status).await {
                Ok(Ok(ControlResponse::Status { uptime_ms, handlers })) => {
                    let subsystems: Vec<_> = handlers
                        .iter()
                        .map(|handler| {
                            serde_json::json!({
                                "id": handler,
                                "name": handler,
                                "status": "running",
                                "state": "loaded",
                                "details": {
                                    "handler": handler
                                }
                            })
                        })
                        .collect();

                    // Query cron subsystem for active schedules (best-effort).
                    let cron_schedules = match bus
                        .request("cron/list", BusPayload::CronList)
                        .await
                    {
                        Ok(Ok(BusPayload::CronListResult { entries })) => {
                            entries
                                .iter()
                                .map(|e| {
                                    serde_json::json!({
                                        "schedule_id": e.schedule_id,
                                        "target_method": e.target_method,
                                        "spec": format!("{:?}", e.spec),
                                        "next_fire_unix_ms": e.next_fire_unix_ms,
                                    })
                                })
                                .collect::<Vec<_>>()
                        }
                        _ => vec![],
                    };

                    let body = serde_json::json!({
                        "status": "ok",
                        "uptime_ms": uptime_ms,
                        "main_process": {
                            "id": "supervisor",
                            "name": "Supervisor",
                            "status": "running",
                            "uptime_ms": uptime_ms,
                            "details": {
                                "handler_count": handlers.len(),
                                "cron_active": cron_schedules.len(),
                                "cron_schedules": cron_schedules,
                            }
                        },
                        "subsystems": subsystems,
                        "bot_id": info.bot_id,
                        "llm_provider": info.llm_provider,
                        "llm_model": info.llm_model,
                        "llm_timeout_seconds": info.llm_timeout_seconds,
                        "enabled_tools": [],
                        "max_tool_rounds": 0,
                        "session_count": 0,
                    });
                    Ok(BusPayload::CommsMessage {
                        channel_id: "manage-http".to_string(),
                        content: body.to_string(),
                        session_id: None,
                        usage: None,
                    })
                }
                Ok(Ok(_)) => Err(BusError::new(-32000, "unexpected control response for status")),
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

