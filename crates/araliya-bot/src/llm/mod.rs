//! LLM provider abstraction.
//!
//! `LlmProvider` is an enum over concrete provider implementations.
//! Add a new variant + module in `providers/` for each additional backend.
//!
//! Provider instances are shared immutable capabilities — clone them freely.
//! Async is delegated to the underlying provider; the `complete` method is
//! `async fn` on the enum so callers need no trait-object machinery.

pub mod providers;

pub use providers::openai_compatible::StreamChunk;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("provider request failed: {0}")]
    Request(String),
}

// ── Timing ────────────────────────────────────────────────────────────────────

/// Wall-clock latency for a single LLM completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTiming {
    /// Milliseconds from the start of the HTTP request until the first content
    /// token was received (streaming only; `None` for non-streaming completions).
    pub ttft_ms: Option<u64>,
    /// Total milliseconds from the start of the HTTP request until the last
    /// byte was received (or the response was fully parsed for non-streaming).
    pub total_ms: u64,
}

// ── Usage / cost ──────────────────────────────────────────────────────────────

/// Token counts returned by the provider for a single completion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Tokens that matched the provider-side prompt cache (billed at a lower rate).
    pub cached_input_tokens: u64,
    /// Internal reasoning tokens consumed before producing the visible output
    /// (OpenAI o-series only). Zero for models that expose reasoning via
    /// `reasoning_content` instead (Qwen3, DeepSeek-R1).
    #[serde(default)]
    pub reasoning_tokens: u64,
}

/// Per-model pricing rates (USD per 1 million tokens).
#[derive(Debug, Clone, Default)]
pub struct ModelRates {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cached_input_per_million_usd: f64,
}

impl LlmUsage {
    /// Compute the cost in USD for this usage given the model's rates.
    pub fn cost_usd(&self, rates: &ModelRates) -> f64 {
        (self.input_tokens as f64 / 1_000_000.0) * rates.input_per_million_usd
            + (self.output_tokens as f64 / 1_000_000.0) * rates.output_per_million_usd
            + (self.cached_input_tokens as f64 / 1_000_000.0) * rates.cached_input_per_million_usd
    }
}

// ── Response ──────────────────────────────────────────────────────────────────

/// Combined result of a single LLM completion: the assistant text and token usage.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    /// Internal chain-of-thought produced by reasoning models (Qwen3, QwQ,
    /// DeepSeek-R1, …). `None` for standard models or when the model did not
    /// return a `reasoning_content` field.
    pub thinking: Option<String>,
    /// `None` for providers that do not report token counts (e.g. `DummyProvider`).
    pub usage: Option<LlmUsage>,
    /// Wall-clock latency for this completion.
    /// `None` for providers that do not measure timing (e.g. `DummyProvider`).
    pub timing: Option<LlmTiming>,
}

// ── Provider enum ─────────────────────────────────────────────────────────────

/// All available provider backends.
///
/// Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency.
/// Adding a backend = new module + new variant + new `complete` arm.
///
/// # Architecture note
///
/// This is a **stateless one-shot text completer**: sends a single user message
/// and returns the assistant text. Conversation history, tool-call loops, and
/// multi-turn state are the responsibility of agent plugins — not providers.
#[derive(Debug, Clone)]
pub enum LlmProvider {
    Dummy(providers::dummy::DummyProvider),
    OpenAiCompatible(providers::openai_compatible::OpenAiCompatibleProvider),
    Qwen(providers::qwen::QwenProvider),
}

impl LlmProvider {
    /// Send `content` as the user message (and optional `system` as the system prompt)
    /// to the provider and return the response including token usage.
    pub async fn complete(
        &self,
        content: &str,
        system: Option<&str>,
        max_tokens_override: Option<usize>,
    ) -> Result<LlmResponse, ProviderError> {
        match self {
            LlmProvider::Dummy(p) => p.complete(content, system, max_tokens_override).await,
            LlmProvider::OpenAiCompatible(p) => {
                p.complete(content, system, max_tokens_override).await
            }
            LlmProvider::Qwen(p) => p.complete(content, system, max_tokens_override).await,
        }
    }

    /// Stream `content` as the user message to the provider.
    ///
    /// Emits [`StreamChunk`]s through `tx` and closes the sender when done.
    /// For `Dummy`, emits a single `Content` chunk then `Done`.
    /// For OpenAI-compatible providers, emits real SSE deltas.
    pub async fn complete_stream(
        &self,
        content: &str,
        system: Option<&str>,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
        max_tokens_override: Option<usize>,
    ) -> Result<(), ProviderError> {
        match self {
            LlmProvider::Dummy(p) => {
                p.complete_stream(content, system, tx, max_tokens_override)
                    .await
            }
            LlmProvider::OpenAiCompatible(p) => {
                p.complete_stream(content, system, tx, max_tokens_override)
                    .await
            }
            LlmProvider::Qwen(p) => {
                p.complete_stream(content, system, tx, max_tokens_override)
                    .await
            }
        }
    }

    /// Lightweight reachability probe (HEAD request or no-op for dummy).
    ///
    /// Returns `Ok(())` if the provider endpoint is reachable, `Err` otherwise.
    /// Any HTTP response code is treated as reachable; only transport failures
    /// (connection refused, timeout) are errors.
    pub async fn ping(&self) -> Result<(), ProviderError> {
        match self {
            LlmProvider::Dummy(_) => Ok(()),
            LlmProvider::OpenAiCompatible(p) => p.ping().await,
            LlmProvider::Qwen(p) => p.ping().await,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_usd_zero_usage() {
        let usage = LlmUsage::default();
        let rates = ModelRates {
            input_per_million_usd: 1.10,
            output_per_million_usd: 4.40,
            cached_input_per_million_usd: 0.275,
        };
        assert_eq!(usage.cost_usd(&rates), 0.0);
    }

    #[test]
    fn cost_usd_normal() {
        let usage = LlmUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        };
        let rates = ModelRates {
            input_per_million_usd: 1.10,
            output_per_million_usd: 4.40,
            cached_input_per_million_usd: 0.0,
        };
        let expected = 1.10 + 4.40 * 0.5;
        let diff = (usage.cost_usd(&rates) - expected).abs();
        assert!(
            diff < 1e-9,
            "expected {expected}, got {}",
            usage.cost_usd(&rates)
        );
    }

    #[test]
    fn cost_usd_with_cache() {
        let usage = LlmUsage {
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 1_000_000,
            reasoning_tokens: 0,
        };
        let rates = ModelRates {
            input_per_million_usd: 0.0,
            output_per_million_usd: 0.0,
            cached_input_per_million_usd: 0.275,
        };
        let diff = (usage.cost_usd(&rates) - 0.275).abs();
        assert!(diff < 1e-9);
    }
}
