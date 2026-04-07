//! LLM subsystem — routes `llm/*` bus requests to a pool of named providers.
//!
//! Implements [`BusHandler`] with prefix `"llm"` so the supervisor can
//! register it generically.  Each request is resolved in a spawned task;
//! the supervisor loop is never blocked on I/O.
//!
//! # Provider pool
//!
//! All `[llm.providers.*]` entries from config are built at startup and kept
//! alive in a `HashMap<String, ProviderEntry>`.  The `active` key selects
//! the current default provider; callers can override per-request via the
//! `provider_override` / `model_override` fields in `LlmRequest`, or use
//! symbolic route hints configured in `[llm.routes]`.
//!
//! # Bus methods
//!
//! | Method                     | Payload       | Purpose                                |
//! |----------------------------|---------------|----------------------------------------|
//! | `llm/health`               | —             | Live ping of the active provider       |
//! | `llm/status`               | —             | Derived from health reporter           |
//! | `llm/{id}/status`          | —             | Provider-scoped status                 |
//! | `llm/detailed_status`      | —             | Provider + model info                  |
//! | `llm/list_providers`       | —             | All named providers and their models   |
//! | `llm/set_default`          | `JsonRequest` | Switch active provider at runtime      |
//! | `llm/instruct`             | `LlmRequest`  | Instruction-pass completion (1024 max) |
//! | `llm/stream`               | `LlmRequest`  | Streaming completion                   |
//! | `llm/complete` (default)   | `LlmRequest`  | Standard completion                    |

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use araliya_core::config::{LlmConfig, RouteConfig};
use araliya_core::obs::ObservabilityHandle;
use araliya_llm::providers;
use araliya_llm::{LlmProvider, ModelRates, ProviderError};
use tokio::sync::mpsc;

use araliya_core::bus::component::{ComponentInfo, ComponentStatusResponse};
use araliya_core::bus::dispatch::BusHandler;
use araliya_core::bus::health::HealthReporter;
use araliya_core::bus::message::{
    BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND, StreamReceiver,
};

/// Interval between background provider reachability checks.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);

// ── Provider entry ──────────────────────────────────────────────────────────

/// A named provider in the pool, with its config-level metadata.
#[derive(Clone)]
struct ProviderEntry {
    provider: LlmProvider,
    model: String,
    #[allow(dead_code)] // Used for cost tracking (future).
    rates: ModelRates,
}

// ── LlmSubsystem ───────────────────────────────────────────────────────────

pub struct LlmSubsystem {
    /// All named providers from `[llm.providers.*]`, keyed by name.
    pool: HashMap<String, ProviderEntry>,
    /// Current active provider name — mutable at runtime via `llm/set_default`.
    active: Arc<RwLock<String>>,
    /// Name of the instruction-pass provider (if configured).
    instruction_name: Option<String>,
    /// Symbolic route hints → (provider, optional model) from `[llm.routes]`.
    routes: HashMap<String, RouteConfig>,
    reporter: Option<HealthReporter>,
    obs: Option<ObservabilityHandle>,
}

impl LlmSubsystem {
    /// Construct the subsystem, building all configured providers into a pool.
    ///
    /// `api_key` comes from `OPENAI_API_KEY` env — never TOML.
    pub fn new(config: &LlmConfig, api_key: Option<String>) -> Result<Self, ProviderError> {
        let mut pool = HashMap::new();

        // Build every named provider.
        for (name, pcfg) in &config.providers {
            let provider = providers::build_from_provider(pcfg, api_key.clone())?;
            pool.insert(
                name.clone(),
                ProviderEntry {
                    provider,
                    model: pcfg.model.clone(),
                    rates: ModelRates {
                        input_per_million_usd: pcfg.input_per_million_usd,
                        output_per_million_usd: pcfg.output_per_million_usd,
                        cached_input_per_million_usd: pcfg.cached_input_per_million_usd,
                    },
                },
            );
        }

        // If the default is "dummy" and not explicitly in providers, synthesize it.
        if config.default == "dummy" && !pool.contains_key("dummy") {
            pool.insert(
                "dummy".to_string(),
                ProviderEntry {
                    provider: LlmProvider::Dummy(araliya_llm::providers::dummy::DummyProvider),
                    model: "dummy".to_string(),
                    rates: ModelRates::default(),
                },
            );
        }

        // Validate instruction provider reference.
        if let Some(ref instr_name) = config.instruction
            && !pool.contains_key(instr_name)
        {
            return Err(ProviderError::UnknownProvider(instr_name.clone()));
        }

        info!(
            active = %config.default,
            pool_size = pool.len(),
            routes = config.routes.len(),
            "llm subsystem initialised"
        );

        Ok(Self {
            pool,
            active: Arc::new(RwLock::new(config.default.clone())),
            instruction_name: config.instruction.clone(),
            routes: config.routes.clone(),
            reporter: None,
            obs: None,
        })
    }

    /// Attach a health reporter to this subsystem.
    pub fn with_health_reporter(mut self, reporter: HealthReporter) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// Attach an observability handle for structured LLM event emissions.
    pub fn with_observability(mut self, obs: ObservabilityHandle) -> Self {
        self.obs = Some(obs);
        self
    }

    /// Spawn a background task that probes the active provider endpoint periodically.
    ///
    /// The task stops when `shutdown` is cancelled.  No-op if no reporter is set.
    pub fn spawn_health_checker(&self, shutdown: CancellationToken) {
        let reporter = match &self.reporter {
            Some(r) => r.clone(),
            None => return,
        };
        let active = self.active.clone();
        let pool = self.pool.clone();
        tokio::spawn(async move {
            // Immediate check on startup.
            if let Some((name, entry)) = Self::active_entry_from(&active, &pool) {
                Self::run_check(&name, &entry.provider, &entry.model, &reporter).await;
            }
            let mut interval = tokio::time::interval(HEALTH_CHECK_INTERVAL);
            interval.tick().await; // consume the first (immediate) tick
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        if let Some((name, entry)) = Self::active_entry_from(&active, &pool) {
                            Self::run_check(&name, &entry.provider, &entry.model, &reporter).await;
                        }
                    }
                }
            }
        });
    }

    // ── Internal helpers ────────────────────────────────────────────────────

    /// Read the current active provider name.
    fn active_name(&self) -> String {
        self.active.read().unwrap().clone()
    }

    /// Look up the active entry from an `Arc<RwLock>` + pool (for use in spawned tasks).
    fn active_entry_from(
        active: &Arc<RwLock<String>>,
        pool: &HashMap<String, ProviderEntry>,
    ) -> Option<(String, ProviderEntry)> {
        let name = active.read().unwrap().clone();
        pool.get(&name).cloned().map(|e| (name, e))
    }

    /// Resolve which provider + model to use for a request.
    ///
    /// Resolution order:
    /// 1. Explicit `provider_override` from the request payload.
    /// 2. Route hint (if provider_override starts with `"hint:"`).
    /// 3. Active default provider.
    ///
    /// `model_override` from the request always wins over the provider's
    /// configured model when present.
    fn resolve_provider(
        &self,
        provider_override: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<(LlmProvider, String), BusError> {
        let (entry, resolved_model) =
            match provider_override {
                // Route hint: "hint:<name>" resolves through the routes table.
                Some(hint_str) if hint_str.starts_with("hint:") => {
                    let hint = &hint_str[5..];
                    let route = self.routes.get(hint).ok_or_else(|| {
                        BusError::new(-32001, format!("unknown route hint: {hint}"))
                    })?;
                    let entry = self.pool.get(&route.provider).ok_or_else(|| {
                        BusError::new(
                            -32001,
                            format!(
                                "route '{}' references unknown provider '{}'",
                                hint, route.provider
                            ),
                        )
                    })?;
                    let model = route.model.as_deref().unwrap_or(&entry.model);
                    (entry.clone(), model.to_string())
                }
                // Explicit provider name.
                Some(name) => {
                    let entry = self.pool.get(name).ok_or_else(|| {
                        BusError::new(-32001, format!("unknown provider: {name}"))
                    })?;
                    (entry.clone(), entry.model.clone())
                }
                // Active default.
                None => {
                    let name = self.active_name();
                    let entry = self.pool.get(&name).ok_or_else(|| {
                        BusError::new(-32001, format!("active provider '{}' not in pool", name))
                    })?;
                    (entry.clone(), entry.model.clone())
                }
            };
        // model_override from the request always wins.
        let final_model = model_override
            .map(|m| m.to_string())
            .unwrap_or(resolved_model);
        Ok((entry.provider, final_model))
    }

    /// Resolve the instruction-pass provider (falls back to active default).
    fn instruction_provider(&self) -> LlmProvider {
        if let Some(ref name) = self.instruction_name
            && let Some(entry) = self.pool.get(name)
        {
            return entry.provider.clone();
        }
        // Fall back to active default.
        let name = self.active_name();
        self.pool
            .get(&name)
            .map(|e| e.provider.clone())
            .unwrap_or(LlmProvider::Dummy(
                araliya_llm::providers::dummy::DummyProvider,
            ))
    }

    async fn run_check(provider_name: &str, provider: &LlmProvider, model: &str, reporter: &HealthReporter) {
        match provider.ping().await {
            Ok(()) => {
                debug!(model, "llm provider reachable");
                reporter
                    .set_healthy_with("ok", Some(serde_json::json!({ "provider": provider_name, "model": model })))
                    .await;
            }
            Err(e) => {
                warn!(model, error = %e, "llm provider unreachable");
                reporter
                    .set_unhealthy_with(
                        format!("provider unreachable: {e}"),
                        Some(serde_json::json!({ "provider": provider_name, "model": model })),
                    )
                    .await;
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
    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        // ── llm/health ──────────────────────────────────────────────────────
        if method == "llm/health" {
            let active = self.active.clone();
            let pool = self.pool.clone();
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                if let Some(ref r) = reporter {
                    if let Some((name, entry)) = Self::active_entry_from(&active, &pool) {
                        Self::run_check(&name, &entry.provider, &entry.model, r).await;
                    }
                    let h = r
                        .get_current()
                        .await
                        .unwrap_or_else(|| araliya_core::bus::health::SubsystemHealth::ok("llm"));
                    let data = serde_json::to_string(&h).unwrap_or_default();
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                } else {
                    let h = araliya_core::bus::health::SubsystemHealth::ok("llm");
                    let data = serde_json::to_string(&h).unwrap_or_default();
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                }
            });
            return;
        }

        // ── llm/status ──────────────────────────────────────────────────────
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
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: resp.to_json(),
                }));
            });
            return;
        }

        // ── llm/{provider_id}/status ────────────────────────────────────────
        {
            let active_name = self.active_name();
            let provider_status_method = format!("llm/{}/status", active_name);
            if method == provider_status_method {
                let reporter = self.reporter.clone();
                tokio::spawn(async move {
                    let resp = match reporter {
                        Some(r) => match r.get_current().await {
                            Some(h) if h.healthy => ComponentStatusResponse::running(active_name),
                            Some(h) => ComponentStatusResponse::error(active_name, h.message),
                            None => ComponentStatusResponse::running(active_name),
                        },
                        None => ComponentStatusResponse::running(active_name),
                    };
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                        data: resp.to_json(),
                    }));
                });
                return;
            }
        }

        // ── llm/detailed_status ─────────────────────────────────────────────
        if method == "llm/detailed_status" {
            let reporter = self.reporter.clone();
            let active_name = self.active_name();
            let model = self
                .pool
                .get(&active_name)
                .map(|e| e.model.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let pool_names: Vec<String> = self.pool.keys().cloned().collect();
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
                    "provider": active_name,
                    "model": model,
                    "pool": pool_names,
                });
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: data.to_string(),
                }));
            });
            return;
        }

        // ── llm/list_providers ──────────────────────────────────────────────
        // Returns all named providers with their model and active status.
        if method == "llm/list_providers" {
            let active_name = self.active_name();
            let entries: Vec<serde_json::Value> = self
                .pool
                .iter()
                .map(|(name, entry)| {
                    serde_json::json!({
                        "name": name,
                        "model": entry.model,
                        "active": name == &active_name,
                    })
                })
                .collect();
            let routes: Vec<serde_json::Value> = self
                .routes
                .iter()
                .map(|(hint, route)| {
                    serde_json::json!({
                        "hint": hint,
                        "provider": route.provider,
                        "model": route.model,
                    })
                })
                .collect();
            let data = serde_json::json!({
                "providers": entries,
                "routes": routes,
                "active": active_name,
            });
            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: data.to_string(),
            }));
            return;
        }

        // ── llm/set_default ─────────────────────────────────────────────────
        // Switch the active provider at runtime. Expects JsonRequest with a
        // "provider" field naming a key in the pool.
        if method == "llm/set_default" {
            if let BusPayload::JsonRequest { data } = payload {
                match serde_json::from_str::<serde_json::Value>(&data) {
                    Ok(v) => {
                        if let Some(name) = v.get("provider").and_then(|p| p.as_str()) {
                            if self.pool.contains_key(name) {
                                let prev = self.active_name();
                                *self.active.write().unwrap() = name.to_string();
                                info!(from = %prev, to = %name, "switched active llm provider");
                                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                                    data: serde_json::json!({
                                        "ok": true,
                                        "previous": prev,
                                        "active": name,
                                    })
                                    .to_string(),
                                }));
                            } else {
                                let _ = reply_tx.send(Err(BusError::new(
                                    -32001,
                                    format!(
                                        "unknown provider '{name}'; available: {:?}",
                                        self.pool.keys().collect::<Vec<_>>()
                                    ),
                                )));
                            }
                        } else {
                            let _ = reply_tx.send(Err(BusError::new(
                                -32001,
                                "missing 'provider' field in request".to_string(),
                            )));
                        }
                    }
                    Err(e) => {
                        let _ =
                            reply_tx.send(Err(BusError::new(-32001, format!("invalid JSON: {e}"))));
                    }
                }
            } else {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    "llm/set_default requires JsonRequest payload".to_string(),
                )));
            }
            return;
        }

        // ── llm/instruct ────────────────────────────────────────────────────
        // Instruction-pass completion; uses the instruction provider if configured,
        // otherwise falls back to the active default.
        if method == "llm/instruct" {
            if let BusPayload::LlmRequest {
                channel_id,
                content,
                system,
                ..
            } = payload
            {
                let provider = self.instruction_provider();
                debug!(%channel_id, "dispatching to instruction llm provider");
                tokio::spawn(async move {
                    let result = provider
                        .complete(&content, system.as_deref(), Some(1024))
                        .await
                        .map(|resp| {
                            if let Some(u) = &resp.usage {
                                tracing::debug!(
                                    input_tokens = u.input_tokens,
                                    output_tokens = u.output_tokens,
                                    cached_tokens = u.cached_input_tokens,
                                    "llm instruct usage"
                                );
                            }
                            BusPayload::CommsMessage {
                                channel_id,
                                content: resp.text,
                                session_id: None,
                                usage: resp.usage,
                                timing: resp.timing,
                                thinking: resp.thinking,
                            }
                        })
                        .map_err(|e| BusError::new(-32000, e.to_string()));
                    let _ = reply_tx.send(result);
                });
            } else {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    "llm/instruct requires LlmRequest payload".to_string(),
                )));
            }
            return;
        }

        // ── llm/stream ─────────────────────────────────────────────────────
        if method == "llm/stream" {
            if let BusPayload::LlmRequest {
                channel_id,
                content,
                system,
                provider_override,
                model_override,
            } = payload
            {
                match self.resolve_provider(provider_override.as_deref(), model_override.as_deref())
                {
                    Ok((provider, _model)) => {
                        debug!(%method, %channel_id, "dispatching streaming to llm provider");
                        tokio::spawn(async move {
                            let (tx, rx) = mpsc::channel(64);
                            let _ = reply_tx.send(Ok(BusPayload::LlmStreamResult {
                                rx: StreamReceiver(rx),
                            }));
                            if let Err(e) = provider
                                .complete_stream(&content, system.as_deref(), tx, None)
                                .await
                            {
                                warn!(error = %e, "streaming LLM provider error");
                            }
                        });
                    }
                    Err(e) => {
                        let _ = reply_tx.send(Err(e));
                    }
                }
            } else {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    "llm/stream requires LlmRequest payload".to_string(),
                )));
            }
            return;
        }

        // ── Default: llm/complete (and any other method) ────────────────────
        match payload {
            BusPayload::LlmRequest {
                channel_id,
                content,
                system,
                provider_override,
                model_override,
            } => {
                match self.resolve_provider(provider_override.as_deref(), model_override.as_deref())
                {
                    Ok((provider, _model)) => {
                        debug!(%method, %channel_id, "dispatching to llm provider");
                        tokio::spawn(async move {
                            let result = provider
                                .complete(&content, system.as_deref(), None)
                                .await
                                .map(|resp| {
                                    if let Some(u) = &resp.usage {
                                        tracing::debug!(
                                            input_tokens = u.input_tokens,
                                            output_tokens = u.output_tokens,
                                            cached_tokens = u.cached_input_tokens,
                                            "llm usage"
                                        );
                                    }
                                    BusPayload::CommsMessage {
                                        channel_id,
                                        content: resp.text,
                                        session_id: None,
                                        usage: resp.usage,
                                        timing: resp.timing,
                                        thinking: resp.thinking,
                                    }
                                })
                                .map_err(|e| BusError::new(-32000, e.to_string()));
                            let _ = reply_tx.send(result);
                        });
                    }
                    Err(e) => {
                        let _ = reply_tx.send(Err(e));
                    }
                }
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
        let active_name = self.active_name();
        let active_model = self
            .pool
            .get(&active_name)
            .map(|e| e.model.as_str())
            .unwrap_or("unknown");
        let active_label = format!(
            "{} ({})",
            ComponentInfo::capitalise(&active_name),
            active_model,
        );
        // Show each provider in the pool as a child node.
        let children: Vec<ComponentInfo> = self
            .pool
            .keys()
            .map(|name| {
                let entry = &self.pool[name];
                let label = format!(
                    "{} ({}){}",
                    ComponentInfo::capitalise(name),
                    entry.model,
                    if name == &active_name { " *" } else { "" },
                );
                ComponentInfo::leaf(name, &label)
            })
            .collect();

        // If only one provider, keep the old flat shape.
        if children.len() <= 1 {
            return ComponentInfo::running(
                "llm",
                "LLM",
                vec![ComponentInfo::leaf(&active_name, &active_label)],
            );
        }
        ComponentInfo::running("llm", "LLM", children)
    }
}
