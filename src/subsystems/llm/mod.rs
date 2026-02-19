//! LLM subsystem — routes `llm/*` bus requests to the configured provider.
//!
//! Implements [`BusHandler`] with prefix `"llm"` so the supervisor can
//! register it generically.  Each request is resolved in a spawned task;
//! the supervisor loop is never blocked on I/O.

use tokio::sync::oneshot;
use tracing::debug;

use crate::config::LlmConfig;
use crate::llm::{LlmProvider, ProviderError};
use crate::llm::providers;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

pub struct LlmSubsystem {
    provider: LlmProvider,
}

impl LlmSubsystem {
    pub fn new(config: &LlmConfig) -> Result<Self, ProviderError> {
        let provider = providers::build(&config.provider)?;
        Ok(Self { provider })
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
                debug!(%method, %channel_id, "dispatching to llm provider");
                tokio::spawn(async move {
                    let result = provider
                        .complete(&content)
                        .await
                        .map(|reply| BusPayload::CommsMessage { channel_id, content: reply })
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
