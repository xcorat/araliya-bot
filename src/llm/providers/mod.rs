//! LLM provider implementations.
//!
//! `build(name)` is the factory — called at startup with the configured
//! provider name.  Adding a new backend = new module + new match arm.

pub mod dummy;

use crate::llm::{LlmProvider, ProviderError};

/// Construct a `LlmProvider` by name.
pub fn build(name: &str) -> Result<LlmProvider, ProviderError> {
    match name {
        "dummy" => Ok(LlmProvider::Dummy(dummy::DummyProvider)),
        _ => Err(ProviderError::UnknownProvider(name.to_string())),
    }
}
