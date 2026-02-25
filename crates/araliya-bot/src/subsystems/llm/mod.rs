//! LLM subsystem — routes `llm/*` bus requests to the configured provider.
//!
//! Implements [`BusHandler`] with prefix `"llm"` so the supervisor can
//! register it generically.  Each request is resolved in a spawned task;
//! the supervisor loop is never blocked on I/O.

use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::config::LlmConfig;
use crate::llm::{LlmProvider, ModelRates, ProviderError};
use crate::llm::providers;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::{ComponentInfo, ComponentStatusResponse};
use crate::supervisor::dispatch::BusHandler;
use crate::supervisor::health::HealthReporter;

/// Interval between background provider reachability checks.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);

pub struct LlmSubsystem {
    provider: LlmProvider,
    provider_name: String,
    model_name: String,
    rates: ModelRates,
    reporter: Option<HealthReporter>,
}

impl LlmSubsystem {
    /// Construct the subsystem. `api_key` comes from `LLM_API_KEY` env — never TOML.
    pub fn new(config: &LlmConfig, api_key: Option<String>) -> Result<Self, ProviderError> {
        let provider = providers::build(config, api_key)?;
        let provider_name = config.provider.clone();
        let model_name = match config.provider.as_str() {
            "qwen" => config.qwen.model.clone(),
            _ => config.openai.model.clone(),
        };
        let rates = match config.provider.as_str() {
            "qwen" => ModelRates {
                input_per_million_usd: config.qwen.input_per_million_usd,
                output_per_million_usd: config.qwen.output_per_million_usd,
                cached_input_per_million_usd: config.qwen.cached_input_per_million_usd,
            },
            _ => ModelRates {
                input_per_million_usd: config.openai.input_per_million_usd,
                output_per_million_usd: config.openai.output_per_million_usd,
                cached_input_per_million_usd: config.openai.cached_input_per_million_usd,
            },
        };
        Ok(Self { provider, provider_name, model_name, rates, reporter: None })
    }

    /// Attach a health reporter to this subsystem.
    ///
    /// Call before registering with the supervisor.  The reporter is used by
    /// both the background checker and the `llm/health` bus handler.
    pub fn with_health_reporter(mut self, reporter: HealthReporter) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// Spawn a background task that probes the LLM provider endpoint periodically.
    ///
    /// Uses the reporter set via [`Self::with_health_reporter`].
    /// The task stops when `shutdown` is cancelled.  No-op if no reporter is set.
    pub fn spawn_health_checker(&self, shutdown: CancellationToken) {
        let reporter = match &self.reporter {
            Some(r) => r.clone(),
            None => return,
        };
        let provider = self.provider.clone();
        let model = self.model_name.clone();
        tokio::spawn(async move {
            // Run an immediate check on startup.
            Self::run_check(&provider, &model, &reporter).await;
            let mut interval = tokio::time::interval(HEALTH_CHECK_INTERVAL);
            interval.tick().await; // consume the first (immediate) tick
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        Self::run_check(&provider, &model, &reporter).await;
                    }
                }
            }
        });
    }

    async fn run_check(provider: &LlmProvider, model: &str, reporter: &HealthReporter) {
        match provider.ping().await {
            Ok(()) => {
                debug!(model, "llm provider reachable");
                reporter.set_healthy_with(
                    "ok",
                    Some(serde_json::json!({ "model": model })),
                ).await;
            }
            Err(e) => {
                warn!(model, error = %e, "llm provider unreachable");
                reporter.set_unhealthy_with(
                    format!("provider unreachable: {e}"),
                    Some(serde_json::json!({ "model": model })),
                ).await;
            }
        }
    }
}

impl BusHandler for LlmSubsystem {
    fn prefix(&self) -> &str {
        "llm"
    }

    /// Route an `llm/*` request. Ownership of `reply_tx` is moved into a
    /// spawned task — the supervisor loop returns immediately.
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        // On-demand health check: runs a live ping and returns the updated state.
        if method == "llm/health" {
            let provider = self.provider.clone();
            let model = self.model_name.clone();
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                if let Some(ref r) = reporter {
                    Self::run_check(&provider, &model, r).await;
                    let h = r.get_current().await
                        .unwrap_or_else(|| crate::supervisor::health::SubsystemHealth::ok("llm"));
                    let data = serde_json::to_string(&h).unwrap_or_default();
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                } else {
                    let h = crate::supervisor::health::SubsystemHealth::ok("llm");
                    let data = serde_json::to_string(&h).unwrap_or_default();
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                }
            });
            return;
        }

        // llm/status — derived from health reporter.
        if method == "llm/status" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let resp = match reporter {
                    Some(r) => match r.get_current().await {
                        Some(h) if h.healthy => ComponentStatusResponse::running("llm"),
                        Some(h) => ComponentStatusResponse::error("llm", h.message),
                        None => ComponentStatusResponse::running("llm"),
                    },
                    None => ComponentStatusResponse::running("llm"),
                };
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: resp.to_json() }));
            });
            return;
        }

        // llm/{provider_id}/status — same health, scoped to the provider child.
        let provider_status_prefix = format!("llm/{}/status", self.provider_name);
        if method == provider_status_prefix {
            let reporter = self.reporter.clone();
            let provider_id = self.provider_name.clone();
            tokio::spawn(async move {
                let resp = match reporter {
                    Some(r) => match r.get_current().await {
                        Some(h) if h.healthy => ComponentStatusResponse::running(provider_id),
                        Some(h) => ComponentStatusResponse::error(provider_id, h.message),
                        None => ComponentStatusResponse::running(provider_id),
                    },
                    None => ComponentStatusResponse::running(provider_id),
                };
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: resp.to_json() }));
            });
            return;
        }

        // llm/detailed_status — includes provider and model info.
        if method == "llm/detailed_status" {
            let reporter = self.reporter.clone();
            let provider = self.provider_name.clone();
            let model = self.model_name.clone();
            tokio::spawn(async move {
                let base = match reporter {
                    Some(r) => match r.get_current().await {
                        Some(h) if h.healthy => ComponentStatusResponse::running("llm"),
                        Some(h) => ComponentStatusResponse::error("llm", h.message),
                        None => ComponentStatusResponse::running("llm"),
                    },
                    None => ComponentStatusResponse::running("llm"),
                };
                let data = serde_json::json!({
                    "id": base.id,
                    "status": base.status,
                    "state": base.state,
                    "provider": provider,
                    "model": model,
                });
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: data.to_string() }));
            });
            return;
        }

        match payload {
            BusPayload::LlmRequest { channel_id, content } => {
                let provider = self.provider.clone();
                let rates = self.rates.clone();
                debug!(%method, %channel_id, "dispatching to llm provider");
                tokio::spawn(async move {
                    let result = provider
                        .complete(&content)
                        .await
                        .map(|resp| {
                            let cost = resp.usage.as_ref().map(|u| u.cost_usd(&rates));
                            if let (Some(u), Some(c)) = (&resp.usage, cost) {
                                tracing::debug!(
                                    input_tokens = u.input_tokens,
                                    output_tokens = u.output_tokens,
                                    cached_tokens = u.cached_input_tokens,
                                    cost_usd = c,
                                    "llm usage"
                                );
                            }
                            BusPayload::CommsMessage {
                                channel_id,
                                content: resp.text,
                                session_id: None,
                                usage: resp.usage,
                            }
                        })
                        .map_err(|e| BusError::new(-32000, e.to_string()));
                    let _ = reply_tx.send(result);
                });
            }
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unsupported payload for method: {method}"),
                )));
            }
        }
    }

    fn component_info(&self) -> ComponentInfo {
        let provider_id = self.provider_name.as_str();
        let provider_label = format!(
            "{} ({})",
            ComponentInfo::capitalise(provider_id),
            self.model_name
        );
        ComponentInfo::running("llm", "LLM", vec![
            ComponentInfo::leaf(provider_id, &provider_label),
        ])
    }
}
