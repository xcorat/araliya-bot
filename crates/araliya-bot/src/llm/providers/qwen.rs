//! Qwen chat completion provider.
//!
//! Wraps the generic OpenAI-compatible provider and uses `[llm.qwen]` config
//! so local Qwen-style endpoints can be selected explicitly with `llm.default = "qwen"`.

use crate::llm::{LlmResponse, ProviderError};

use super::openai_compatible::OpenAiCompatibleProvider;

#[derive(Debug, Clone)]
pub struct QwenProvider {
    inner: OpenAiCompatibleProvider,
}

impl QwenProvider {
    pub fn new(
        api_base_url: String,
        model: String,
        temperature: f32,
        timeout_seconds: u64,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        let inner = OpenAiCompatibleProvider::new(
            api_base_url,
            model,
            temperature,
            timeout_seconds,
            api_key,
        )?;
        Ok(Self { inner })
    }

    pub async fn complete(&self, content: &str) -> Result<LlmResponse, ProviderError> {
        self.inner.complete(content).await
    }

    pub async fn ping(&self) -> Result<(), ProviderError> {
        self.inner.ping().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_provider() {
        let provider = QwenProvider::new(
            "http://127.0.0.1:8081/v1/chat/completions".to_string(),
            "qwen2.5-instruct".to_string(),
            0.2,
            5,
            None,
        );
        assert!(provider.is_ok());
    }
}
