//! OpenAI-compatible chat completion provider (`/v1/chat/completions`).
//!
//! Exposes a single `complete(&str) -> String` interface matching the rest of
//! the `LlmProvider` abstraction. All OpenAI wire types are private to this
//! module — callers never see them. Tool-call handling belongs at the agent
//! layer (agent plugins manage the loop); this provider is stateless.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, trace};

use crate::llm::{LlmResponse, LlmUsage, ProviderError};

// ── Public provider ───────────────────────────────────────────────────────────

/// Adapter for any HTTP endpoint implementing `/v1/chat/completions`.
///
/// Covers OpenAI, OpenAI-compatible local servers (Ollama, LM Studio…),
/// and hosted alternatives. Constructed once at startup, then cheaply cloned
/// because `reqwest::Client` is an `Arc` internally.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    client: Client,
    api_base_url: String,
    model: String,
    temperature: f32,
    timeout_seconds: u64,
    api_key: Option<String>,
}

impl OpenAiCompatibleProvider {
    /// Build a provider from config values and an optional API key.
    ///
    /// `api_key` is `None` for keyless local models. When present it is sent
    /// as `Authorization: Bearer <key>` on every request.
    pub fn new(
        api_base_url: String,
        model: String,
        temperature: f32,
        timeout_seconds: u64,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|e| ProviderError::Request(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { client, api_base_url, model, temperature, timeout_seconds, api_key })
    }

    /// Lightweight reachability probe.
    ///
    /// Sends a HEAD request to the configured endpoint.  Any HTTP response
    /// (including 4xx) means the server is reachable.  Only a transport-level
    /// failure (connection refused, timeout) is treated as unreachable.
    ///
    /// Uses a hard 5-second timeout regardless of the LLM timeout config.
    pub async fn ping(&self) -> Result<(), ProviderError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| ProviderError::Request(format!("failed to build ping client: {e}")))?;
        let mut req = client.head(&self.api_base_url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.send()
            .await
            .map(|_| ())
            .map_err(|e| ProviderError::Request(format!("unreachable: {e}")))
    }

    /// Send `content` as the user message and optionally `system` as the system prompt.
    ///
    /// History management and tool-call loops are intentionally the agent's
    /// responsibility — this method is one round-trip only.
    pub async fn complete(&self, content: &str, system: Option<&str>) -> Result<LlmResponse, ProviderError> {
        // Some models (gpt-5 family) do not accept a temperature parameter.
        let temperature = if self.model.starts_with("gpt-5") {
            None
        } else {
            Some(self.temperature)
        };

        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(Message { role: "system".to_string(), content: sys.to_string() });
        }
        messages.push(Message { role: "user".to_string(), content: content.to_string() });

        let payload = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            temperature,
        };

        debug!(
            model = %payload.model,
            temperature = ?payload.temperature,
            content_len = content.len(),
            "sending LLM request"
        );
        if tracing::enabled!(tracing::Level::TRACE) {
            let json = serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|e| format!("<serialization failed: {e}>"));
            trace!(payload = %json, "full LLM request payload");
        }

        let mut req = self.client.post(&self.api_base_url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        // #region agent log
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open("/data/araliya/project/araliya-bot/.cursor/debug.log") {
            use std::io::Write;
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
            let line = format!("{{\"location\":\"openai_compatible.rs:complete\",\"message\":\"llm request start\",\"data\":{{\"url\":\"{}\",\"model\":\"{}\",\"timeout_seconds\":{}}},\"timestamp\":{},\"hypothesisId\":\"H1\"}}\n",
                self.api_base_url.replace('\\', "\\\\").replace('"', "\\\""),
                self.model.replace('\\', "\\\\").replace('"', "\\\""),
                self.timeout_seconds, ts);
            let _ = f.write_all(line.as_bytes());
        }
        // #endregion

        let response = req.send().await.map_err(|e| {
            // #region agent log
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open("/data/araliya/project/araliya-bot/.cursor/debug.log") {
                use std::io::Write;
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                let err_msg = e.to_string().replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
                let line = format!("{{\"location\":\"openai_compatible.rs:send_err\",\"message\":\"llm request failed\",\"data\":{{\"url\":\"{}\",\"error\":\"{}\",\"is_timeout\":{}}},\"timestamp\":{},\"hypothesisId\":\"H1\"}}\n",
                    self.api_base_url.replace('\\', "\\\\").replace('"', "\\\""),
                    err_msg,
                    e.is_timeout(), ts);
                let _ = f.write_all(line.as_bytes());
            }
            // #endregion
            error!(url = %self.api_base_url, error = %e, "LLM HTTP request failed (transport)");
            ProviderError::Request(e.to_string())
        })?;

        let response = check_status(response).await?;

        let parsed = response.json::<ChatCompletionResponse>().await.map_err(|e| {
            error!(error = %e, "failed to deserialize LLM response");
            ProviderError::Request(format!("failed to parse response body: {e}"))
        })?;

        debug!(choices = parsed.choices.len(), "received LLM response");
        if tracing::enabled!(tracing::Level::TRACE) {
            let json = serde_json::to_string_pretty(&parsed)
                .unwrap_or_else(|e| format!("<serialization failed: {e}>"));
            trace!(response = %json, "full LLM response payload");
        }

        let text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Request("empty or missing content in response".into()))?;

        let usage = parsed.usage.map(|u| LlmUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cached_input_tokens: u.prompt_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
        });

        Ok(LlmResponse { text, usage })
    }
}

// ── Private wire types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<UsageData>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct UsageData {
    prompt_tokens: u64,
    completion_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

// Error envelope used by OpenAI and compatible APIs.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
    #[serde(default)]
    code: Option<serde_json::Value>,
}

/// Consume the response and return it if successful, or a structured error.
async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read error body>".to_string());

    let message = if let Ok(env) = serde_json::from_str::<ErrorEnvelope>(&body) {
        let code = env.error.code.map(|v| match v {
            serde_json::Value::String(s) => format!(" [code={s}]"),
            other => format!(" [code={other}]"),
        }).unwrap_or_default();
        format!("HTTP {status}{code}: {}", env.error.message)
    } else {
        format!("HTTP {status}: {body}")
    };

    error!(%status, %message, "LLM request returned HTTP error");
    Err(ProviderError::Request(message))
}
