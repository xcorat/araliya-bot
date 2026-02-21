#![cfg_attr(test, allow(unused_variables))]
use tokio::sync::oneshot;

use crate::config::NewsmailAggregatorConfig;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

#[cfg(feature = "plugin-gmail-tool")]
mod gmail;
#[cfg(feature = "plugin-gmail-tool")]
mod newsmail_aggregator;

pub struct ToolsSubsystem {
    newsmail_defaults: NewsmailAggregatorConfig,
}

impl ToolsSubsystem {
    pub fn new(newsmail_defaults: NewsmailAggregatorConfig) -> Self {
        Self { newsmail_defaults }
    }
}

impl Default for ToolsSubsystem {
    fn default() -> Self {
        Self::new(NewsmailAggregatorConfig {
            mailbox: "inbox".to_string(),
            n_last: 10,
            tsec_last: None,
        })
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

                #[cfg(feature = "plugin-gmail-tool")]
                if tool == "newsmail_aggregator" && action == "get" {
                    let defaults = self.newsmail_defaults.clone();
                    tokio::spawn(async move {
                        match newsmail_aggregator::get(defaults, &args_json).await {
                            Ok(items) => {
                                let data_json = serde_json::to_string(&items)
                                    .unwrap_or_else(|_| "[]".to_string());
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "newsmail_aggregator".to_string(),
                                    action: "get".to_string(),
                                    ok: true,
                                    data_json: Some(data_json),
                                    error: None,
                                }));
                            }
                            Err(e) => {
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "newsmail_aggregator".to_string(),
                                    action: "get".to_string(),
                                    ok: false,
                                    data_json: None,
                                    error: Some(e),
                                }));
                            }
                        }
                    });
                    return;
                }

                #[cfg(feature = "plugin-gmail-tool")]
                if tool == "newsmail_aggregator" && action == "healthcheck" {
                    let defaults = self.newsmail_defaults.clone();
                    tokio::spawn(async move {
                        match newsmail_aggregator::healthcheck(defaults).await {
                            Ok(result) => {
                                let data_json = serde_json::to_string(&result)
                                    .unwrap_or_else(|_| "{}".to_string());
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "newsmail_aggregator".to_string(),
                                    action: "healthcheck".to_string(),
                                    ok: true,
                                    data_json: Some(data_json),
                                    error: None,
                                }));
                            }
                            Err(e) => {
                                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                                    tool: "newsmail_aggregator".to_string(),
                                    action: "healthcheck".to_string(),
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
