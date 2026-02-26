//! Dummy LLM provider — echoes input back prefixed with `[echo]`.
//! Used for testing the full bus round-trip without a real API key.

use crate::llm::{LlmResponse, ProviderError};

#[derive(Debug, Clone)]
pub struct DummyProvider;

impl DummyProvider {
    pub async fn complete(&self, content: &str, _system: Option<&str>) -> Result<LlmResponse, ProviderError> {
        Ok(LlmResponse {
            text: format!("[echo] {content}"),
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn complete_prefixes_echo() {
        let p = DummyProvider;
        assert_eq!(p.complete("hello", None).await.unwrap().text, "[echo] hello");
    }

    #[tokio::test]
    async fn complete_empty_input() {
        let p = DummyProvider;
        // `_system` param is intentionally unused
        assert_eq!(p.complete("", None).await.unwrap().text, "[echo] ");
    }

    #[tokio::test]
    async fn complete_usage_is_none() {
        let p = DummyProvider;
        assert!(p.complete("test", None).await.unwrap().usage.is_none());
    }
}
