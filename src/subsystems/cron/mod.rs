//! Cron subsystem — background timer service that emits scheduled bus events.
//!
//! Registers as a [`BusHandler`] with prefix `"cron"`.  Other subsystems
//! schedule events by sending bus requests:
//!
//! - `cron/schedule` — register a one-shot or repeating timer.
//! - `cron/cancel`   — remove an active schedule by ID.
//! - `cron/list`     — list all active schedules.
//!
//! When a timer fires, the cron service emits the configured `target_method`
//! as a bus notification.  The supervisor routes it by prefix like any other
//! notification — no special routing logic required.
//!
//! # Implementation
//!
//! The subsystem spawns a single background tokio task that uses
//! `tokio::time::sleep_until` to park until the next deadline.  There is no
//! polling:  the task wakes only when a timer fires, a command arrives on
//! its internal channel, or shutdown is requested.

mod service;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::supervisor::bus::{
    BusError, BusHandle, BusPayload, BusResult, CronScheduleSpec, ERR_METHOD_NOT_FOUND,
};
use crate::supervisor::dispatch::BusHandler;

use service::{CronCommand, CronService};

/// Application error code for malformed cron requests.
const ERR_BAD_REQUEST: i32 = -32600;

/// Cron subsystem — owns a background timer task and exposes scheduling via
/// the supervisor bus.
pub struct CronSubsystem {
    /// Send commands to the background timer task.
    cmd_tx: mpsc::Sender<CronCommand>,
}

impl CronSubsystem {
    /// Create the cron subsystem. Spawns the background timer task immediately.
    pub fn new(
        bus: BusHandle,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let svc = CronService::new(bus, cmd_rx, shutdown);
        tokio::spawn(svc.run());
        debug!("cron subsystem started");
        Self { cmd_tx }
    }
}

impl BusHandler for CronSubsystem {
    fn prefix(&self) -> &str {
        "cron"
    }

    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        let cmd_tx = self.cmd_tx.clone();

        match method {
            "cron/schedule" => {
                let (target_method, payload_json, spec) = match payload {
                    BusPayload::CronSchedule {
                        target_method,
                        payload_json,
                        spec,
                    } => (target_method, payload_json, spec),
                    _ => {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "cron/schedule requires CronSchedule payload",
                        )));
                        return;
                    }
                };

                // Validate the spec minimally.
                if let CronScheduleSpec::Interval { every_secs } = &spec {
                    if *every_secs == 0 {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "interval every_secs must be > 0",
                        )));
                        return;
                    }
                }

                tokio::spawn(async move {
                    let (ack_tx, ack_rx) = oneshot::channel();
                    let cmd = CronCommand::Schedule {
                        target_method,
                        payload_json,
                        spec,
                        reply: ack_tx,
                    };
                    if cmd_tx.send(cmd).await.is_err() {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "cron service not running",
                        )));
                        return;
                    }
                    match ack_rx.await {
                        Ok(id) => {
                            let _ = reply_tx.send(Ok(BusPayload::CronScheduleResult {
                                schedule_id: id,
                            }));
                        }
                        Err(_) => {
                            let _ = reply_tx.send(Err(BusError::new(
                                ERR_BAD_REQUEST,
                                "cron service dropped reply",
                            )));
                        }
                    }
                });
            }

            "cron/cancel" => {
                let schedule_id = match payload {
                    BusPayload::CronCancel { schedule_id } => schedule_id,
                    _ => {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "cron/cancel requires CronCancel payload",
                        )));
                        return;
                    }
                };

                tokio::spawn(async move {
                    let (ack_tx, ack_rx) = oneshot::channel();
                    let cmd = CronCommand::Cancel {
                        schedule_id,
                        reply: ack_tx,
                    };
                    if cmd_tx.send(cmd).await.is_err() {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "cron service not running",
                        )));
                        return;
                    }
                    match ack_rx.await {
                        Ok(ok) => {
                            if ok {
                                let _ = reply_tx.send(Ok(BusPayload::Empty));
                            } else {
                                let _ = reply_tx.send(Err(BusError::new(
                                    ERR_BAD_REQUEST,
                                    "schedule not found",
                                )));
                            }
                        }
                        Err(_) => {
                            let _ = reply_tx.send(Err(BusError::new(
                                ERR_BAD_REQUEST,
                                "cron service dropped reply",
                            )));
                        }
                    }
                });
            }

            "cron/list" => {
                tokio::spawn(async move {
                    let (ack_tx, ack_rx) = oneshot::channel();
                    let cmd = CronCommand::List { reply: ack_tx };
                    if cmd_tx.send(cmd).await.is_err() {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_BAD_REQUEST,
                            "cron service not running",
                        )));
                        return;
                    }
                    match ack_rx.await {
                        Ok(entries) => {
                            let _ = reply_tx.send(Ok(BusPayload::CronListResult { entries }));
                        }
                        Err(_) => {
                            let _ = reply_tx.send(Err(BusError::new(
                                ERR_BAD_REQUEST,
                                "cron service dropped reply",
                            )));
                        }
                    }
                });
            }

            _ => {
                warn!(method, "cron: unknown method");
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("cron method not found: {method}"),
                )));
            }
        }
    }
}
