//! Shared LLM types used across crates (bus payloads, agent state, etc.).
//!
//! These types are defined here rather than in the LLM crate because they appear
//! in bus message payloads and agent state — both of which live outside the LLM
//! subsystem.

use serde::{Deserialize, Serialize};

// ── Stream chunk ──────────────────────────────────────────────────────────────

/// A single chunk emitted during a streaming completion.
///
/// Chunks arrive in two phases: first all `Thinking` deltas (reasoning content),
/// then all `Content` deltas (final answer), then one `Done` with usage totals.
/// Providers that do not support streaming emit exactly one `Content` + one `Done`.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A delta from the model's internal reasoning phase (`reasoning_content`).
    Thinking(String),
    /// A delta from the model's visible output (`content`).
    Content(String),
    /// End of stream; carries token usage and wall-clock timing if available.
    Done {
        usage: Option<LlmUsage>,
        timing: Option<LlmTiming>,
    },
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
