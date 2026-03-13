//! Dummy LLM provider — echoes input back prefixed with `[echo]`.
//! Used for testing the full bus round-trip without a real API key.

use tokio::sync::mpsc;

use crate::llm::{LlmResponse, ProviderError, StreamChunk};

#[derive(Debug, Clone)]
pub struct DummyProvider;

impl DummyProvider {
    pub async fn complete(
        &self,
        content: &str,
        _system: Option<&str>,
        _max_tokens_override: Option<usize>,
    ) -> Result<LlmResponse, ProviderError> {
        Ok(LlmResponse {
            text: format!("[echo] {content}"),
            thinking: None,
            usage: None,
        })
    }

    pub async fn complete_stream(
        &self,
        content: &str,
        _system: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
        _max_tokens_override: Option<usize>,
    ) -> Result<(), ProviderError> {
        let _ = tx.send(StreamChunk::Content(format!("[echo] {content}"))).await;
        let _ = tx.send(StreamChunk::Done(None)).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn complete_prefixes_echo() {
        let p = DummyProvider;
        assert_eq!(
            p.complete("hello", None, None).await.unwrap().text,
            "[echo] hello"
        );
    }

    #[tokio::test]
    async fn complete_empty_input() {
        let p = DummyProvider;
        // `_system` param is intentionally unused
        assert_eq!(p.complete("", None, None).await.unwrap().text, "[echo] ");
    }

    #[tokio::test]
    async fn complete_usage_is_none() {
        let p = DummyProvider;
        assert!(p.complete("test", None, None).await.unwrap().usage.is_none());
    }
}
