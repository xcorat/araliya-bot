//! LLM provider implementations.
//!
//! `build(config, api_key)` is the factory — called at startup.
//! `api_type` in each provider config selects the wire adapter.

pub mod dummy;
pub mod chat_completions;
pub mod openai_responses;


use araliya_core::config::{ApiType, LlmConfig, ProviderConfig};

use crate::{LlmProvider, ProviderError};

/// Construct a `LlmProvider` from the active provider in `config`.
///
/// `api_key` comes from `OPENAI_API_KEY` env — never from TOML.
pub fn build(config: &LlmConfig, api_key: Option<String>) -> Result<LlmProvider, ProviderError> {
    if config.default == "dummy" {
        return Ok(LlmProvider::Dummy(dummy::DummyProvider));
    }
    let cfg = config
        .providers
        .get(&config.default)
        .ok_or_else(|| ProviderError::UnknownProvider(config.default.clone()))?;
    build_from_provider(cfg, api_key)
}

/// Construct a `LlmProvider` directly from a `ProviderConfig`.
///
/// Used for instruction-pass providers and any code that needs to build a
/// provider without going through the full `LlmConfig`.
pub fn build_from_provider(
    cfg: &ProviderConfig,
    api_key: Option<String>,
) -> Result<LlmProvider, ProviderError> {
    match cfg.api_type {
        ApiType::Dummy => Ok(LlmProvider::Dummy(dummy::DummyProvider)),
        ApiType::ChatCompletions => {
            let p = chat_completions::ChatCompletionsProvider::new(
                cfg.api_base_url.clone(),
                cfg.model.clone(),
                cfg.temperature,
                cfg.timeout_seconds,
                api_key,
                cfg.max_tokens,
            )?;
            Ok(LlmProvider::ChatCompletions(p))
        }
        ApiType::OpenAiResponses => {
            let p = openai_responses::OpenAiResponsesProvider::new(
                cfg.api_base_url.clone(),
                cfg.model.clone(),
                cfg.reasoning_effort
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                cfg.timeout_seconds,
                api_key,
                cfg.max_tokens,
            )?;
            Ok(LlmProvider::OpenAiResponses(p))
        }
    }
}
