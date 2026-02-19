//! LLM provider abstraction.
//!
//! `LlmProvider` is an enum over concrete provider implementations.
//! Add a new variant + module in `providers/` for each additional backend.
//!
//! Provider instances are shared immutable capabilities — clone them freely.
//! Async is delegated to the underlying provider; the `complete` method is
//! `async fn` on the enum so callers need no trait-object machinery.

pub mod providers;

use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("provider request failed: {0}")]
    Request(String),
}

// ── Provider enum ─────────────────────────────────────────────────────────────

/// All available provider backends.
///
/// Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency.
/// Adding a backend = new module + new variant + new `complete` arm.
#[derive(Debug, Clone)]
pub enum LlmProvider {
    Dummy(providers::dummy::DummyProvider),
}

impl LlmProvider {
    /// Send `content` to the provider and return its text reply.
    pub async fn complete(&self, content: &str) -> Result<String, ProviderError> {
        match self {
            LlmProvider::Dummy(p) => p.complete(content).await,
        }
    }
}
