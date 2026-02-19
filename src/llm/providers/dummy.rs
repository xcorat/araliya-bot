//! Dummy LLM provider — echoes input back prefixed with `[echo]`.
//! Used for testing the full bus round-trip without a real API key.

use crate::llm::ProviderError;

#[derive(Debug, Clone)]
pub struct DummyProvider;

impl DummyProvider {
    pub async fn complete(&self, content: &str) -> Result<String, ProviderError> {
        Ok(format!("[echo] {content}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn complete_prefixes_echo() {
        let p = DummyProvider;
        assert_eq!(p.complete("hello").await.unwrap(), "[echo] hello");
    }

    #[tokio::test]
    async fn complete_empty_input() {
        let p = DummyProvider;
        assert_eq!(p.complete("").await.unwrap(), "[echo] ");
    }
}
