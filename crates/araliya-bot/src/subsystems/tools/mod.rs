use tokio::sync::oneshot;

use crate::config::NewsmailAggregatorConfig;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::ComponentInfo;
use crate::supervisor::dispatch::BusHandler;
use crate::supervisor::health::HealthReporter;

#[cfg(feature = "plugin-gmail-tool")]
mod gmail;
#[cfg(feature = "plugin-gmail-tool")]
mod newsmail_aggregator;

pub struct ToolsSubsystem {
    newsmail_defaults: NewsmailAggregatorConfig,
    reporter: Option<HealthReporter>,
}

impl ToolsSubsystem {
    pub fn new(newsmail_defaults: NewsmailAggregatorConfig) -> Self {
        Self { newsmail_defaults, reporter: None }
    }

    /// Attach a health reporter.  Reports healthy at startup; individual tool
    /// failures are surfaced per-execution, not via subsystem health state.
    pub fn with_health_reporter(mut self, reporter: HealthReporter) -> Self {
        let r = reporter.clone();
        tokio::spawn(async move { r.set_healthy().await });
        self.reporter = Some(reporter);
        self
    }
}

impl Default for ToolsSubsystem {
    fn default() -> Self {
        Self::new(NewsmailAggregatorConfig {
            label_ids: vec!["INBOX".to_string()],
            n_last: 10,
            tsec_last: None,
            q: None,
        })
    }
}

impl BusHandler for ToolsSubsystem {
    fn prefix(&self) -> &str {
        "tools"
    }

    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        if method == "tools/health" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let h = match reporter {
                    Some(r) => r.get_current().await
                        .unwrap_or_else(|| crate::supervisor::health::SubsystemHealth::ok("tools")),
                    None => crate::supervisor::health::SubsystemHealth::ok("tools"),
                };
                let data = serde_json::to_string(&h).unwrap_or_default();
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
            });
            return;
        }

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

    fn component_info(&self) -> ComponentInfo {
        let mut children: Vec<ComponentInfo> = vec![];
        #[cfg(feature = "plugin-gmail-tool")]
        {
            children.push(ComponentInfo::leaf("gmail", "Gmail"));
            children.push(ComponentInfo::leaf("newsmail_aggregator", "Newsmail Aggregator"));
        }
        children.sort_by(|a, b| a.id.cmp(&b.id));
        ComponentInfo::running("tools", "Tools", children)
    }
}
