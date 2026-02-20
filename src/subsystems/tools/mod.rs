use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

#[cfg(feature = "plugin-gmail-tool")]
mod gmail;

pub struct ToolsSubsystem;

impl ToolsSubsystem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolsSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

impl BusHandler for ToolsSubsystem {
    fn prefix(&self) -> &str {
        "tools"
    }

    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        if method != "tools/execute" {
            let _ = reply_tx.send(Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("method not found: {method}"),
            )));
            return;
        }

        match payload {
            BusPayload::ToolRequest {
                tool,
                action,
                args_json,
                channel_id: _,
                session_id: _,
            } => {
                #[cfg(feature = "plugin-gmail-tool")]
                if tool == "gmail" && action == "read_latest" {
                    tokio::spawn(async move {
                        let query = serde_json::from_str::<serde_json::Value>(&args_json)
                            .ok()
                            .and_then(|v| v.get("query").and_then(|q| q.as_str()).map(|s| s.to_string()));

                        match gmail::read_latest(query.as_deref()).await {
                            Ok(summary) => {
                                let data_json = serde_json::to_string(&summary)
                                    .unwrap_or_else(|_| "{}".to_string());
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "gmail".to_string(),
                                    action: "read_latest".to_string(),
                                    ok: true,
                                    data_json: Some(data_json),
                                    error: None,
                                }));
                            }
                            Err(e) => {
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "gmail".to_string(),
                                    action: "read_latest".to_string(),
                                    ok: false,
                                    data_json: None,
                                    error: Some(e),
                                }));
                            }
                        }
                    });
                    return;
                }

                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("tool/action not found: {tool}/{action}"),
                )));
            }
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected ToolRequest payload",
                )));
            }
        }
    }
}
