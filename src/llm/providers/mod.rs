//! LLM provider implementations.
//!
//! `build(config, api_key)` is the factory — called at startup.
//! Adding a new backend = new module + new match arm.

pub mod dummy;
pub mod openai_compatible;
pub mod qwen;

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
        "qwen" => {
            let q = &config.qwen;
            // #region agent log
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open("/data/araliya/project/araliya-bot/.cursor/debug.log") {
                use std::io::Write;
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                let line = format!("{{\"location\":\"providers/mod.rs:build\",\"message\":\"llm provider built\",\"data\":{{\"provider\":\"qwen\",\"api_base_url\":\"{}\",\"model\":\"{}\",\"timeout_seconds\":{}}},\"timestamp\":{},\"hypothesisId\":\"H4\"}}\n",
                    q.api_base_url.replace('\\', "\\\\").replace('"', "\\\""),
                    q.model.replace('\\', "\\\\").replace('"', "\\\""),
                    q.timeout_seconds, ts);
                let _ = f.write_all(line.as_bytes());
            }
            // #endregion
            let p = qwen::QwenProvider::new(
                q.api_base_url.clone(),
                q.model.clone(),
                q.temperature,
                q.timeout_seconds,
                api_key,
            )?;
            Ok(LlmProvider::Qwen(p))
        }
        _ => Err(ProviderError::UnknownProvider(config.provider.clone())),
    }
}
