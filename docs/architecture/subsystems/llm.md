# LLM Subsystem

**Status:** v0.6.0 — `LlmResponse.thinking` (reasoning_content for Qwen3/QwQ/DeepSeek-R1) · `LlmUsage.reasoning_tokens` (o-series) · `StreamChunk` enum · `complete_stream()` on all providers · `llm/stream` bus method · Qwen provider · per-session spend accumulation.

---

## Overview

The LLM subsystem is a bus participant that handles all `llm/*` requests. It owns the configured provider and resolves each request asynchronously — the supervisor loop is never blocked on provider I/O.

The Agents subsystem uses the bus to call `llm/complete` rather than holding a direct reference to the provider. Any future subsystem can do the same.

---

## Responsibilities

- Receive `llm/complete`, `llm/instruct`, and `llm/stream` requests via the supervisor bus
- Forward each prompt to the appropriate `LlmProvider` (main or instruction)
- Deserialize token usage and reasoning content from the provider response
- Compute per-call cost using configured pricing rates and log it
- Return the reply as `BusPayload::CommsMessage` (preserving `channel_id`, `usage`, and `thinking`)
- For streaming: return `BusPayload::LlmStreamResult` immediately, then emit `StreamChunk`s asynchronously
- Spawn one task per request so the supervisor loop is non-blocking

---

## Module Layout

```
src/
  llm/
    mod.rs              LlmProvider enum · LlmResponse · LlmUsage · ModelRates · StreamChunk (re-export)
    providers/
      mod.rs            build(config) factory function
      dummy.rs          DummyProvider — returns "[echo] {input}", usage: None
      openai_compatible.rs  reqwest HTTP client; reasoning_content extraction; SSE streaming; StreamChunk
      qwen.rs           QwenProvider — wraps OpenAiCompatibleProvider with Qwen defaults
  subsystems/
    llm/
      mod.rs            LlmSubsystem — handle_request, tokio::spawn per call
```

---

## Types

### `LlmUsage`
```rust
pub struct LlmUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,   // from prompt_tokens_details.cached_tokens
    pub reasoning_tokens: u64,      // from completion_tokens_details.reasoning_tokens (o-series)
}
```

### `ModelRates`
```rust
pub struct ModelRates {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cached_input_per_million_usd: f64,
}
```

### `LlmResponse`
```rust
pub struct LlmResponse {
    pub text: String,
    pub thinking: Option<String>,  // reasoning_content (Qwen3, QwQ, DeepSeek-R1); None for standard models
    pub usage: Option<LlmUsage>,   // None for DummyProvider and keyless endpoints
}
```

`LlmUsage::cost_usd(rates: &ModelRates) -> f64` applies per-million-token pricing.

### `StreamChunk`
```rust
pub enum StreamChunk {
    Thinking(String),        // reasoning_content delta from reasoning models
    Content(String),         // content delta (answer text)
    Done(Option<LlmUsage>),  // end of stream with usage totals
}
```

Re-exported as `crate::llm::StreamChunk` and `crate::supervisor::bus::StreamChunk`.

---

## Provider Abstraction

`LlmProvider` is an enum over concrete implementations. Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency. Adding a backend = new module + new variant + new match arms.

```rust
pub enum LlmProvider {
    Dummy(DummyProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),
    Qwen(QwenProvider),
}

impl LlmProvider {
    /// Buffered completion — waits for the full response.
    pub async fn complete(
        &self,
        content: &str,
        system: Option<&str>,
    ) -> Result<LlmResponse, ProviderError>;

    /// Streaming completion — emits StreamChunks on `tx` as they arrive.
    /// Returns when the stream is finished or on error.
    pub async fn complete_stream(
        &self,
        content: &str,
        system: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), ProviderError>;

    pub async fn ping(&self) -> Result<(), ProviderError>;
}
```

`DummyProvider.complete_stream()` emits a single `Content` chunk then `Done(None)` — useful for tests.

---

## Bus Protocol

### `llm/complete` — main buffered completion

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String, system: Option<String> }`

Used by the response pass of `AgenticLoop` and by simple chat agents. Routes to the main configured provider.

**Reply payload:** `BusPayload::CommsMessage { channel_id, content: reply, session_id: None, usage: Option<LlmUsage>, thinking: Option<String> }`

### `llm/instruct` — instruction pass (SLM router)

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String, system: Option<String> }`

Used by the instruction pass of `AgenticLoop` when `use_instruction_llm = true`. Routes to the instruction provider (`[llm.instruction]`). **Falls back to the main provider** when no instruction provider is configured.

The instruction pass expects structured JSON output from the model. Use a model tuned for structured output or apply few-shot examples in the prompt (`config/prompts/agentic_instruct.txt`).

### `llm/stream` — streaming completion

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String, system: Option<String> }`

**Immediate reply:** `BusPayload::LlmStreamResult { rx: StreamReceiver }` — the receiver is returned *before* generation begins. The caller then reads `StreamChunk`s from `rx` as the provider emits them.

`StreamReceiver` is a newtype over `mpsc::Receiver<StreamChunk>`. It is in-process only — it implements `Serialize` as a unit value and `Deserialize` as an error to satisfy the `BusPayload: Serialize + Deserialize` bounds, but it is never serialized over a wire.

Bypasses session history — used by the SSE endpoint for direct streaming to HTTP clients.

---

## Request Lifecycle

### Buffered (`llm/complete`, `llm/instruct`)

```
supervisor receives Request { method: "llm/complete", payload: LlmRequest { .. }, reply_tx }
  → llm.handle_request(method, payload, reply_tx)       // supervisor returns immediately
    → tokio::spawn {
        provider.complete(&content, system).await
          → Ok(LlmResponse { text, thinking, usage })
              → log input_tokens, output_tokens, cached_tokens, reasoning_tokens, cost_usd  [DEBUG]
              → reply_tx.send(Ok(CommsMessage { channel_id, content: text, thinking, usage }))
          → Err(e)
              → reply_tx.send(Err(BusError { .. }))
      }
```

### Streaming (`llm/stream`)

```
supervisor receives Request { method: "llm/stream", payload: LlmRequest { .. }, reply_tx }
  → llm.handle_request(method, payload, reply_tx)
    → tokio::spawn {
        let (tx, rx) = mpsc::channel(64);
        reply_tx.send(Ok(LlmStreamResult { rx: StreamReceiver(rx) }))  // immediate
        provider.complete_stream(&content, system, tx).await
          // provider emits: Thinking(..) → ... → Content(..) → ... → Done(usage)
          // when tx is dropped, rx sees None → stream closed
      }
```

---

## Spend Accumulation

After each LLM call in `SessionChatPlugin`, if the response carries `usage` and the session is disk-backed, `SessionHandle::accumulate_spend(usage, &state.llm_rates)` is called. This reads/updates/writes `sessions/{id}/spend.json`:

```json
{
  "total_input_tokens": 1240,
  "total_output_tokens": 380,
  "total_cached_tokens": 0,
  "total_cost_usd": 0.000694,
  "last_updated": "2026-02-21T10:59:42Z"
}
```

`AgentsState.llm_rates` is populated from config at startup via `AgentsSubsystem::with_llm_rates(rates)`.

---

## Thinking / Reasoning Content

Reasoning models expose their chain-of-thought separately from their final answer:

| Model family | Thinking exposed? | How |
|---|---|---|
| Qwen3 / QwQ / DeepSeek-R1 | YES | `choices[0].message.reasoning_content` (buffered) or `delta.reasoning_content` (stream) |
| OpenAI o3 / o4-mini | count only | `completion_tokens_details.reasoning_tokens` — content hidden |
| Standard (GPT-4o, Qwen2.5, etc.) | no | `thinking` is `None` |

`LlmResponse.thinking` carries the reasoning text when present. It flows through `BusPayload::CommsMessage.thinking` → `CommsReply.thinking` → `"thinking"` field in the JSON API response → `ChatMessage.thinking` in the frontend, where it is rendered in a collapsible "Reasoning" block.

`reasoning_tokens` in `LlmUsage` is always populated (0 when not reported).

---

## Current Providers

`DummyProvider` requires no API key. Returns `"[echo] {input}"` with `usage: None`. Supports `complete_stream()` for test coverage.

`OpenAiCompatibleProvider` uses `[llm.openai]` settings plus `LLM_API_KEY` from env/.env. Extracts `reasoning_content` and `reasoning_tokens`. Supports full SSE streaming via `complete_stream()`.

`QwenProvider` wraps `OpenAiCompatibleProvider` with Qwen-specific defaults and endpoint handling. Full streaming and reasoning content support.

---

## Configuration

```toml
[llm]
default = "openai"

[llm.openai]
api_base_url = "https://api.openai.com/v1/chat/completions"
model = "gpt-5-nano"
temperature = 0.2
timeout_seconds = 60
# Token pricing — USD per 1 million tokens. Defaults to 0.0 when not set.
input_per_million_usd = 1.10
output_per_million_usd = 4.40
cached_input_per_million_usd = 0.275

# Optional: separate small model for the agentic instruction pass.
# Provider sub-sections ([llm.instruction.openai] / [llm.instruction.qwen])
# are optional — absent fields are inherited from [llm.openai] / [llm.qwen].
# [llm.instruction]
# provider = "openai"
# [llm.instruction.openai]
# model = "gpt-5-nano"
# temperature = 0.1
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active provider. Supported: `"dummy"`, `"openai"`, `"qwen"`. |
| `llm.openai.api_base_url` | string | OpenAI endpoint | Chat completions URL. Set to a local server for Ollama / LM Studio. |
| `llm.openai.model` | string | `"gpt-5-nano"` | Model name sent in the request body. |
| `llm.openai.temperature` | float | `0.2` | Sampling temperature (silently omitted for `gpt-5` family). |
| `llm.openai.timeout_seconds` | integer | `60` | Per-request HTTP timeout. |
| `llm.openai.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.openai.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.openai.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |
| `llm.instruction.provider` | string | — | Provider for `llm/instruct` requests. Inherits connection from main provider when sub-section is absent. |
| `llm.instruction.openai.model` | string | inherited | Override model for the instruction pass (e.g. smaller/cheaper model). |
| `llm.instruction.openai.temperature` | float | inherited | Override temperature for the instruction pass (lower = more deterministic JSON). |

Pricing fields default to `0.0` so cost is silently omitted rather than wrong when not configured.

---

## Adding a Real Provider

1. Create `src/llm/providers/{name}.rs` — implement `complete()` and `complete_stream()`.
2. Add a variant to `LlmProvider` in `src/llm/mod.rs`.
3. Add match arms to `LlmProvider::complete`, `complete_stream`, and `ping`.
4. Add a match arm to `providers::build(config)` in `src/llm/providers/mod.rs`.
5. Update `[llm] default = "{name}"` in `config/default.toml`.
6. Pass secrets via environment variable or `.env` (never in config files).

---

## Planned Provider Support

| Provider | Auth | Notes |
|----------|------|-------|
| OpenAI-compatible | `LLM_API_KEY` | Implemented (`default = "openai"`) |
| Qwen | `LLM_API_KEY` | Implemented (`default = "qwen"`) |
| Dummy | none | Implemented (`default = "dummy"`) |
| Anthropic | `ANTHROPIC_API_KEY` | Planned |
