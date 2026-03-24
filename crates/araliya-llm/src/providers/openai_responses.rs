//! OpenAI Responses API provider.
//!
//! Used by Codex models (`gpt-5.3-codex` etc.) which are served at
//! `POST /v1/responses` rather than the Chat Completions endpoint.
//!
//! Wire format:
//! ```text
//! request:  { model, input, instructions?, max_output_tokens?, reasoning, stream }
//! response: { output: [{ content: [{ type: "output_text", text }] }], usage }
//! ```

use std::time::Instant;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::{LlmResponse, LlmTiming, LlmUsage, ProviderError, StreamChunk};

// ── Provider struct ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OpenAiResponsesProvider {
    client: Client,
    api_base_url: String,
    model: String,
    reasoning_effort: String,
    #[allow(dead_code)]
    timeout_seconds: u64,
    api_key: Option<String>,
    max_tokens: usize,
}

impl OpenAiResponsesProvider {
    pub fn new(
        api_base_url: String,
        model: String,
        reasoning_effort: String,
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
            reasoning_effort,
            timeout_seconds,
            api_key,
            max_tokens,
        })
    }

    pub async fn complete(
        &self,
        content: &str,
        system: Option<&str>,
        max_tokens_override: Option<usize>,
    ) -> Result<LlmResponse, ProviderError> {
        let effective_max = max_tokens_override.or(if self.max_tokens > 0 {
            Some(self.max_tokens)
        } else {
            None
        });

        let payload = ResponsesRequest {
            model: self.model.clone(),
            input: content.to_string(),
            instructions: system.map(|s| s.to_string()),
            max_output_tokens: effective_max.map(|m| m as u32),
            reasoning: ReasoningOptions {
                effort: self.reasoning_effort.clone(),
            },
            stream: false,
        };

        debug!(model = %payload.model, "sending Responses API request");

        let mut req = self.client.post(&self.api_base_url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let req_start = Instant::now();
        let response = req.send().await.map_err(|e| {
            error!(url = %self.api_base_url, error = %e, "Responses API HTTP request failed");
            ProviderError::Request(e.to_string())
        })?;

        let response = check_status(response).await?;

        let parsed = response
            .json::<ResponsesResponse>()
            .await
            .map_err(|e| {
                error!(error = %e, "failed to deserialize Responses API response");
                ProviderError::Request(format!("failed to parse response body: {e}"))
            })?;

        let text = extract_text(&parsed.output)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::Request("empty or missing output_text in response".into()))?;

        let usage = parsed.usage.map(|u| LlmUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cached_input_tokens: u
                .input_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            reasoning_tokens: u
                .output_tokens_details
                .and_then(|d| d.reasoning_tokens)
                .unwrap_or(0),
        });

        Ok(LlmResponse {
            text,
            thinking: None,
            usage,
            timing: Some(LlmTiming {
                ttft_ms: None,
                total_ms: req_start.elapsed().as_millis() as u64,
            }),
        })
    }

    pub async fn complete_stream(
        &self,
        content: &str,
        system: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
        max_tokens_override: Option<usize>,
    ) -> Result<(), ProviderError> {
        let effective_max = max_tokens_override.or(if self.max_tokens > 0 {
            Some(self.max_tokens)
        } else {
            None
        });

        let payload = ResponsesRequest {
            model: self.model.clone(),
            input: content.to_string(),
            instructions: system.map(|s| s.to_string()),
            max_output_tokens: effective_max.map(|m| m as u32),
            reasoning: ReasoningOptions {
                effort: self.reasoning_effort.clone(),
            },
            stream: true,
        };

        debug!(model = %payload.model, "sending streaming Responses API request");

        let mut req = self.client.post(&self.api_base_url).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let req_start = Instant::now();
        let response = req.send().await.map_err(|e| {
            error!(url = %self.api_base_url, error = %e, "Responses API streaming request failed");
            ProviderError::Request(e.to_string())
        })?;

        let response = check_status(response).await?;

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut ttft_ms: Option<u64> = None;

        while let Some(chunk) = stream.next().await {
            let bytes =
                chunk.map_err(|e| ProviderError::Request(format!("stream read error: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(newline) = buf.find('\n') {
                let line = buf[..newline].trim().to_string();
                buf = buf[newline + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }
                let json_str = match line.strip_prefix("data: ") {
                    Some(s) => s,
                    None => continue,
                };

                let chunk_val: serde_json::Value = match serde_json::from_str(json_str) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, "failed to parse Responses API SSE chunk");
                        continue;
                    }
                };

                // Final usage chunk
                if let Some(usage_val) = chunk_val.get("usage").filter(|v| !v.is_null()) {
                    let usage = LlmUsage {
                        input_tokens: usage_val["input_tokens"].as_u64().unwrap_or(0),
                        output_tokens: usage_val["output_tokens"].as_u64().unwrap_or(0),
                        cached_input_tokens: usage_val["input_tokens_details"]["cached_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                        reasoning_tokens: usage_val["output_tokens_details"]["reasoning_tokens"]
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

                // Text delta: Responses API SSE uses output[].content[].text
                if let Some(delta) = chunk_val["delta"]["text"].as_str().filter(|s| !s.is_empty()) {
                    if ttft_ms.is_none() {
                        ttft_ms = Some(req_start.elapsed().as_millis() as u64);
                    }
                    let _ = tx.send(StreamChunk::Content(delta.to_string())).await;
                }
            }
        }

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

    pub async fn ping(&self) -> Result<(), ProviderError> {
        let mut req = self
            .client
            .head(&self.api_base_url)
            .timeout(std::time::Duration::from_secs(5));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.send().await.map(|_| ()).map_err(|e| {
            ProviderError::Request(format!("ping failed: {e}"))
        })
    }
}

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    reasoning: ReasoningOptions,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ReasoningOptions {
    effort: String,
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    #[serde(default)]
    output: Vec<OutputItem>,
    #[serde(default)]
    usage: Option<ResponsesUsage>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(default)]
    content: Vec<ContentItem>,
}

#[derive(Debug, Deserialize)]
struct ContentItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    input_tokens_details: Option<InputTokensDetails>,
    #[serde(default)]
    output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Debug, Deserialize)]
struct InputTokensDetails {
    #[serde(default)]
    cached_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct OutputTokensDetails {
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn extract_text(output: &[OutputItem]) -> Option<String> {
    let text: String = output
        .iter()
        .flat_map(|item| item.content.iter())
        .filter(|c| c.kind == "output_text")
        .filter_map(|c| c.text.as_deref())
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() { None } else { Some(text.trim().to_string()) }
}

async fn check_status(
    response: reqwest::Response,
) -> Result<reqwest::Response, ProviderError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    // Try to parse OpenAI error envelope
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = v["error"]["message"].as_str() {
            return Err(ProviderError::Request(format!("HTTP {status}: {msg}")));
        }
    }
    Err(ProviderError::Request(format!("HTTP {status}: {body}")))
}
