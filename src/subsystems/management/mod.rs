//! Management subsystem — supervisor-facing management bus handler.
//!
//! Exposes on the supervisor bus:
//! - `manage/http/get` — health/status JSON (used by HTTP `/health`).
//! - `manage/http/tree` — component tree JSON for HTTP (e.g. GET /api/tree); no private data.
//! - `manage/tree` — same tree for control/CLI consumers.

use std::sync::{Arc, OnceLock};

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::ComponentInfo;
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
    /// Populated by `comms::start()` once the channel list is known.
    comms_info: Arc<OnceLock<ComponentInfo>>,
}

impl ManagementSubsystem {
    pub fn new(
        control: ControlHandle,
        bus: BusHandle,
        info: ManagementInfo,
        comms_info: Arc<OnceLock<ComponentInfo>>,
    ) -> Self {
        Self { control, bus, info, comms_info }
    }
}

fn tree_comms_message(content: String, channel_id: &str) -> BusPayload {
    BusPayload::CommsMessage {
        channel_id: channel_id.to_string(),
        content,
        session_id: None,
        usage: None,
    }
}

fn control_status_error(e: impl std::fmt::Display) -> BusError {
    BusError::new(-32000, format!("{e}"))
}

impl BusHandler for ManagementSubsystem {
    fn prefix(&self) -> &str {
        "manage"
    }

    fn component_info(&self) -> ComponentInfo {
        // The management subsystem itself is a leaf — no children beyond the
        // comms tree which is injected separately at the supervisor level.
        ComponentInfo::leaf("manage", "Management")
    }

    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        const HTTP_GET: &str = "manage/http/get";
        const HTTP_TREE: &str = "manage/http/tree";
        const TREE: &str = "manage/tree";

        let is_tree = matches!(method, HTTP_TREE | TREE);
        if !matches!(method, HTTP_GET | HTTP_TREE | TREE) {
            let _ = reply_tx.send(Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("method not found: {method}"),
            )));
            return;
        }

        if !matches!(payload, BusPayload::Empty) {
            let _ = reply_tx.send(Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("unsupported payload for method: {method}"),
            )));
            return;
        }

        let control = self.control.clone();
        let bus = self.bus.clone();
        let info = self.info.clone();
        let comms_info = self.comms_info.clone();
        let channel_id = if method == HTTP_TREE {
            "manage-http-tree"
        } else if method == TREE {
            "manage-tree"
        } else {
            "manage-http"
        };

        tokio::spawn(async move {
            let status = match control.request(ControlCommand::Status).await {
                Ok(Ok(ControlResponse::Status { uptime_ms, handlers })) => (uptime_ms, handlers),
                Ok(Ok(_)) => {
                    let _ = reply_tx.send(Err(control_status_error("unexpected control response for status")));
                    return;
                }
                Ok(Err(e)) => {
                    let _ = reply_tx.send(Err(control_status_error(format!("control error: {e:?}"))));
                    return;
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(control_status_error(format!("control transport error: {e}"))));
                    return;
                }
            };

            if is_tree {
                // Ask the supervisor for the component tree (calls component_info() on each handler).
                let tree_json = match control.request(ControlCommand::ComponentTree).await {
                    Ok(Ok(ControlResponse::ComponentTree { mut tree_json })) => {
                        // Inject the comms node if available (comms is not a BusHandler,
                        // so the supervisor cannot call component_info() on it directly).
                        if let Some(comms) = comms_info.get() {
                            if let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&tree_json) {
                                if let Some(children) = root.get_mut("children").and_then(|c| c.as_array_mut()) {
                                    if let Ok(comms_val) = serde_json::to_value(comms) {
                                        children.push(comms_val);
                                        children.sort_by(|a, b| {
                                            a.get("id").and_then(|v| v.as_str())
                                                .cmp(&b.get("id").and_then(|v| v.as_str()))
                                        });
                                    }
                                }
                                tree_json = serde_json::to_string(&root)
                                    .unwrap_or_else(|_| "{}".to_string());
                            }
                        }
                        tree_json
                    }
                    _ => {
                        // Fallback: build a minimal tree from the status handler list.
                        let (uptime_ms, ref handlers) = status;
                        let mut children: Vec<serde_json::Value> = handlers.iter().map(|id| {
                            serde_json::json!({
                                "id": id,
                                "name": ComponentInfo::capitalise(id),
                                "status": "running",
                                "state": "on",
                                "children": [],
                            })
                        }).collect();
                        if let Some(comms) = comms_info.get() {
                            if let Ok(v) = serde_json::to_value(comms) {
                                children.push(v);
                                children.sort_by(|a, b| {
                                    a.get("id").and_then(|v| v.as_str())
                                        .cmp(&b.get("id").and_then(|v| v.as_str()))
                                });
                            }
                        }
                        let root = serde_json::json!({
                            "id": "supervisor",
                            "name": "Supervisor",
                            "status": "running",
                            "state": "on",
                            "uptime_ms": uptime_ms,
                            "children": children,
                        });
                        serde_json::to_string(&root).unwrap_or_else(|_| "{}".to_string())
                    }
                };
                let _ = reply_tx.send(Ok(tree_comms_message(tree_json, channel_id)));
                return;
            }

            // manage/http/get: health body with cron and info
            let (uptime_ms, handlers) = status;
            let subsystems: Vec<_> = handlers
                .iter()
                .map(|handler| {
                    serde_json::json!({
                        "id": handler,
                        "name": handler,
                        "status": "running",
                        "state": "loaded",
                        "details": { "handler": handler }
                    })
                })
                .collect();

            let cron_schedules = match bus.request("cron/list", BusPayload::CronList).await {
                Ok(Ok(BusPayload::CronListResult { entries })) => entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "schedule_id": e.schedule_id,
                            "target_method": e.target_method,
                            "spec": format!("{:?}", e.spec),
                            "next_fire_unix_ms": e.next_fire_unix_ms,
                        })
                    })
                    .collect::<Vec<_>>(),
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
            let _ = reply_tx.send(Ok(tree_comms_message(body.to_string(), channel_id)));
        });
    }
}
