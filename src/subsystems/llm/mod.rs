//! LLM subsystem — routes `llm/*` bus requests to the configured provider.
//!
//! Implements [`BusHandler`] with prefix `"llm"` so the supervisor can
//! register it generically.  Each request is resolved in a spawned task;
//! the supervisor loop is never blocked on I/O.

use tokio::sync::oneshot;
use tracing::debug;

use crate::config::LlmConfig;
use crate::llm::{LlmProvider, ModelRates, ProviderError};
use crate::llm::providers;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

pub struct LlmSubsystem {
    provider: LlmProvider,
    rates: ModelRates,
}

impl LlmSubsystem {
    /// Construct the subsystem. `api_key` comes from `LLM_API_KEY` env — never TOML.
    pub fn new(config: &LlmConfig, api_key: Option<String>) -> Result<Self, ProviderError> {
        let provider = providers::build(config, api_key)?;
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
        Ok(Self { provider, rates })
    }
}

impl BusHandler for LlmSubsystem {
    fn prefix(&self) -> &str {
        "llm"
    }

    /// Route an `llm/*` request. Ownership of `reply_tx` is moved into a
    /// spawned task — the supervisor loop returns immediately.
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
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
}
