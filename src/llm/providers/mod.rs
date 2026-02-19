//! LLM provider implementations.
//!
//! `build(config, api_key)` is the factory — called at startup.
//! Adding a new backend = new module + new match arm.

pub mod dummy;
pub mod openai_compatible;

use crate::config::LlmConfig;
use crate::llm::{LlmProvider, ProviderError};

/// Construct a `LlmProvider` from config and an optional API key.
///
/// `api_key` is sourced from `LLM_API_KEY` env (never TOML) and is `None`
/// for keyless local models.
pub fn build(config: &LlmConfig, api_key: Option<String>) -> Result<LlmProvider, ProviderError> {
    match config.provider.as_str() {
        "dummy" => Ok(LlmProvider::Dummy(dummy::DummyProvider)),
        "openai" | "openai-compatible" => {
            let oai = &config.openai;
            let p = openai_compatible::OpenAiCompatibleProvider::new(
                oai.api_base_url.clone(),
                oai.model.clone(),
                oai.temperature,
                oai.timeout_seconds,
                api_key,
            )?;
            Ok(LlmProvider::OpenAiCompatible(p))
        }
        _ => Err(ProviderError::UnknownProvider(config.provider.clone())),
    }
}
