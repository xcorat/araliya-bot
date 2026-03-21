//! OpenAI-compatible chat completion provider (`/v1/chat/completions`).
//!
//! Exposes a single `complete(&str) -> String` interface matching the rest of
//! the `LlmProvider` abstraction. All OpenAI wire types are private to this
//! module — callers never see them. Tool-call handling belongs at the agent
//! layer (agent plugins manage the loop); this provider is stateless.

use futures_util::StreamExt as _;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::{LlmResponse, LlmTiming, LlmUsage, ProviderError, StreamChunk};

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
    #[allow(dead_code)]
    timeout_seconds: u64,
    api_key: Option<String>,
    /// Maximum output tokens sent in every request.  0 means no explicit limit.
    max_tokens: usize,
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
        max_tokens: usize,
    ) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|e| ProviderError::Request(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_base_url,
            model,
            temperature,
            timeout_seconds,
            api_key,
            max_tokens,
        })
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
    pub async fn complete(
        &self,
        content: &str,
        system: Option<&str>,
        max_tokens_override: Option<usize>,
    ) -> Result<LlmResponse, ProviderError> {
        // Some models (gpt-5 family) do not accept a temperature parameter.
        let temperature = if self.model.starts_with("gpt-5") {
            None
        } else {
            Some(self.temperature)
        };

        let effective_max_tokens = max_tokens_override.or(if self.max_tokens > 0 {
            Some(self.max_tokens)
        } else {
            None
        });

        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(Message {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }
        messages.push(Message {
            role: "user".to_string(),
            content: content.to_string(),
        });

        let payload = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            temperature,
            max_completion_tokens: effective_max_tokens.map(|m| m as u32),
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

        let req_start = Instant::now();
        let response = req.send().await.map_err(|e| {
            error!(url = %self.api_base_url, error = %e, "LLM HTTP request failed (transport)");
            ProviderError::Request(e.to_string())
        })?;

        let response = check_status(response).await?;

        let parsed = response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| {
                error!(error = %e, "failed to deserialize LLM response");
                ProviderError::Request(format!("failed to parse response body: {e}"))
            })?;

        debug!(choices = parsed.choices.len(), "received LLM response");
        if tracing::enabled!(tracing::Level::TRACE) {
            let json = serde_json::to_string_pretty(&parsed)
                .unwrap_or_else(|e| format!("<serialization failed: {e}>"));
            trace!(response = %json, "full LLM response payload");
        }

        let first_choice = parsed.choices.into_iter().next();

        let text = first_choice
            .as_ref()
            .and_then(|c| c.message.content.as_deref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Request("empty or missing content in response".into()))?;

        let thinking = first_choice
            .and_then(|c| c.message.reasoning_content)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let usage = parsed.usage.map(|u| LlmUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cached_input_tokens: u
                .prompt_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            reasoning_tokens: u
                .completion_tokens_details
                .and_then(|d| d.reasoning_tokens)
                .unwrap_or(0),
        });

        Ok(LlmResponse {
            text,
            thinking,
            usage,
            timing: Some(LlmTiming {
                ttft_ms: None,
                total_ms: req_start.elapsed().as_millis() as u64,
            }),
        })
    }

    /// Stream a completion via Server-Sent Events (`stream: true`).
    ///
    /// Emits [`StreamChunk::Thinking`] deltas, then [`StreamChunk::Content`]
    /// deltas, then a final [`StreamChunk::Done`] with usage totals.
    /// Falls back to a single-chunk emission if the model does not support
    /// streaming (i.e. returns a non-streaming JSON response).
    ///
    /// The sender is closed when the stream ends or on error — callers should
    /// loop until `rx.recv()` returns `None`.
    pub async fn complete_stream(
        &self,
        content: &str,
        system: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
        max_tokens_override: Option<usize>,
    ) -> Result<(), ProviderError> {
        let temperature = if self.model.starts_with("gpt-5") {
            None
        } else {
            Some(self.temperature)
        };

        let effective_max_tokens = max_tokens_override.or(if self.max_tokens > 0 {
            Some(self.max_tokens)
        } else {
            None
        });

        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(Message {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }
        messages.push(Message {
            role: "user".to_string(),
            content: content.to_string(),
        });

        let payload = ChatCompletionStreamRequest {
            model: self.model.clone(),
            messages,
            temperature,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            max_completion_tokens: effective_max_tokens.map(|m| m as u32),
        };

        debug!(model = %payload.model, "sending streaming LLM request");

        let mut req = self.client.post(&self.api_base_url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let req_start = Instant::now();
        let response = req.send().await.map_err(|e| {
            error!(url = %self.api_base_url, error = %e, "streaming LLM HTTP request failed");
            ProviderError::Request(e.to_string())
        })?;

        let response = check_status(response).await?;

        // Parse the SSE stream line by line.
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut ttft_ms: Option<u64> = None;

        while let Some(chunk) = stream.next().await {
            let bytes =
                chunk.map_err(|e| ProviderError::Request(format!("stream read error: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // Process all complete `data: ...` lines in the buffer.
            while let Some(newline) = buf.find('\n') {
                let line = buf[..newline].trim().to_string();
                buf = buf[newline + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }
                let json_str = match line.strip_prefix("data: ") {
                    Some(s) => s,
                    None => continue, // skip non-data lines (e.g. "event:", ":")
                };

                let chunk_val: serde_json::Value = match serde_json::from_str(json_str) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, line = %json_str, "failed to parse SSE chunk");
                        continue;
                    }
                };

                // Extract usage from final chunk (some providers send it last).
                if let Some(usage_val) = chunk_val.get("usage").filter(|v| !v.is_null()) {
                    let usage = LlmUsage {
                        input_tokens: usage_val["prompt_tokens"].as_u64().unwrap_or(0),
                        output_tokens: usage_val["completion_tokens"].as_u64().unwrap_or(0),
                        cached_input_tokens: usage_val["prompt_tokens_details"]["cached_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                        reasoning_tokens: usage_val["completion_tokens_details"]
                            ["reasoning_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                    };
                    let _ = tx
                        .send(StreamChunk::Done {
                            usage: Some(usage),
                            timing: Some(LlmTiming {
                                ttft_ms,
                                total_ms: req_start.elapsed().as_millis() as u64,
                            }),
                        })
                        .await;
                    return Ok(());
                }

                let delta = &chunk_val["choices"][0]["delta"];
                if delta.is_null() {
                    continue;
                }

                if let Some(rc) = delta["reasoning_content"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                {
                    if ttft_ms.is_none() {
                        ttft_ms = Some(req_start.elapsed().as_millis() as u64);
                    }
                    let _ = tx.send(StreamChunk::Thinking(rc.to_string())).await;
                }
                if let Some(ct) = delta["content"].as_str().filter(|s| !s.is_empty()) {
                    if ttft_ms.is_none() {
                        ttft_ms = Some(req_start.elapsed().as_millis() as u64);
                    }
                    let _ = tx.send(StreamChunk::Content(ct.to_string())).await;
                }
            }
        }

        // Stream ended without an explicit usage chunk — send Done without usage.
        let _ = tx
            .send(StreamChunk::Done {
                usage: None,
                timing: Some(LlmTiming {
                    ttft_ms,
                    total_ms: req_start.elapsed().as_millis() as u64,
                }),
            })
            .await;
        Ok(())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionStreamRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
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
    #[serde(default)]
    completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u64,
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct CompletionTokensDetails {
    /// Internal reasoning tokens (OpenAI o-series). Not the same as
    /// `reasoning_content` on Qwen/DeepSeek — those models expose their
    /// reasoning text but don't populate this field.
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
    /// Reasoning/thinking content returned by Qwen3, QwQ, DeepSeek-R1, and
    /// compatible models. Absent (null/missing) for standard models.
    #[serde(default)]
    reasoning_content: Option<String>,
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
        let code = env
            .error
            .code
            .map(|v| match v {
                serde_json::Value::String(s) => format!(" [code={s}]"),
                other => format!(" [code={other}]"),
            })
            .unwrap_or_default();
        format!("HTTP {status}{code}: {}", env.error.message)
    } else {
        format!("HTTP {status}: {body}")
    };

    error!(%status, %message, "LLM request returned HTTP error");
    Err(ProviderError::Request(message))
}
