# LLM Subsystem

**Status:** v0.2.0-alpha — provider pool (all `[llm.providers.*]` built at startup) · runtime provider switch (`llm/set_default`) · per-request `provider_override` / `model_override` on `LlmRequest` · symbolic route hints (`[llm.routes]`) · `llm/list_providers` · unknown `api_type` catch-all (defaults to `chat_completions` + warn) · `LlmResponse.thinking` · `LlmUsage.reasoning_tokens` · `StreamChunk` enum · `complete_stream()` on all providers · `llm/stream` bus method · `api_type`-based adapter selection · OpenAI Responses API provider · per-session spend accumulation.

---

## Overview

The LLM subsystem is a bus participant that handles all `llm/*` requests. It owns a **pool** of named providers (all `[llm.providers.*]` entries built at startup) and resolves each request asynchronously — the supervisor loop is never blocked on provider I/O.

The Agents subsystem uses the bus to call `llm/complete` rather than holding a direct reference to any provider. Any future subsystem can do the same. The active provider can be switched at runtime via `llm/set_default`; per-request overrides allow any caller to target a specific provider or model without changing global state.

---

## Responsibilities

- Build all `[llm.providers.*]` entries into a live pool at startup
- Receive `llm/complete`, `llm/instruct`, `llm/stream`, `llm/list_providers`, and `llm/set_default` requests via the supervisor bus
- Resolve which provider + model to use for each request (active default → `provider_override` → route hint)
- Forward each prompt to the resolved `LlmProvider`
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
      mod.rs            build_from_provider(cfg, api_key) factory function; ApiType enum
      dummy.rs          DummyProvider — returns "[echo] {input}", usage: None
      chat_completions.rs   ChatCompletionsProvider — reqwest HTTP client; /v1/chat/completions; SSE streaming; reasoning_content extraction
      responses.rs      OpenAiResponsesProvider — /v1/responses wire format; reasoning_effort; SSE streaming
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
    ChatCompletions(ChatCompletionsProvider),
    OpenAiResponses(OpenAiResponsesProvider),
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

`LlmRequest` carries two optional override fields available on every method that accepts it:

| Field | Type | Description |
|---|---|---|
| `provider_override` | `Option<String>` | Named pool key (e.g. `"codex"`) or a route hint (`"hint:reasoning"`). Bypasses the active default. |
| `model_override` | `Option<String>` | Overrides the provider's configured model for this single request. |

---

### `llm/complete` — buffered completion

**Request:** `BusPayload::LlmRequest { channel_id, content, system, provider_override, model_override }`

Used by the response pass of `AgenticLoop` and by simple chat agents. Routes to the resolved provider.

**Reply:** `BusPayload::CommsMessage { channel_id, content: reply, session_id: None, usage, thinking }`

---

### `llm/instruct` — instruction pass (SLM router)

**Request:** `BusPayload::LlmRequest { channel_id, content, system, .. }`

Used by the instruction pass of `AgenticLoop` when `use_instruction_llm = true`. Routes to the instruction provider (`[llm.instruction]`). **Falls back to the active default** when no instruction provider is configured. `provider_override` / `model_override` are ignored on this method.

The instruction pass expects structured JSON output from the model. Use a model tuned for structured output or apply few-shot examples in the prompt (`config/agents/agentic-chat/instruct.md`).

---

### `llm/stream` — streaming completion

**Request:** `BusPayload::LlmRequest { channel_id, content, system, provider_override, model_override }`

**Immediate reply:** `BusPayload::LlmStreamResult { rx: StreamReceiver }` — the receiver is returned *before* generation begins. The caller then reads `StreamChunk`s from `rx` as the provider emits them.

`StreamReceiver` is a newtype over `mpsc::Receiver<StreamChunk>`. It is in-process only — never serialized over a wire.

Bypasses session history — used by the SSE endpoint for direct streaming to HTTP clients.

---

### `llm/list_providers` — enumerate pool

**Request:** any payload (ignored)

**Reply:** `BusPayload::JsonResponse`

```json
{
  "active": "openai",
  "providers": [
    { "name": "openai", "model": "gpt-5-nano", "active": true },
    { "name": "local",  "model": "qwen2.5-instruct", "active": false }
  ],
  "routes": [
    { "hint": "fast", "provider": "openai", "model": null }
  ]
}
```

---

### `llm/set_default` — switch active provider at runtime

**Request:** `BusPayload::JsonRequest { data: "{\"provider\": \"local\"}" }`

Atomically updates the active default provider (protected by `Arc<RwLock<String>>`). Health checks and subsequent `llm/complete` requests immediately target the new provider. No restart needed.

**Reply on success:** `{ "ok": true, "previous": "openai", "active": "local" }`

**Reply on error:** `BusError` with a message listing available provider names.

---

### `llm/{name}/status` — provider-scoped status

Returns `ComponentStatusResponse` for the named provider. Currently reports the active provider's health state.

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

`DummyProvider` requires no API key. Returns `"[echo] {input}"` with `usage: None`. Supports `complete_stream()` for test coverage. Selected by `api_type = "dummy"` or when `default = "dummy"` with no providers entry.

`ChatCompletionsProvider` handles the `/v1/chat/completions` wire format. It works with OpenAI's hosted API, Ollama, LM Studio, llama.cpp, Qwen-compatible servers, and any other OpenAI-compatible endpoint. Reads `OPENAI_API_KEY` from env/.env for authentication; local servers that do not require a key are simply configured without it. Extracts `reasoning_content` and `reasoning_tokens`. Supports full SSE streaming via `complete_stream()`. Sends `max_completion_tokens` (not the deprecated `max_tokens`) in the request body — required by gpt-5-series and later OpenAI models. Omits `temperature` automatically for the `gpt-5` family. Selected by `api_type = "chat_completions"`.

`OpenAiResponsesProvider` handles the `/v1/responses` wire format used by Codex and OpenAI reasoning models. Accepts a `reasoning_effort` field (`"none"`, `"low"`, `"medium"`, `"high"`). Supports SSE streaming. Selected by `api_type = "openai_responses"`.

---

## Configuration

All `[llm.providers.*]` entries are built into a live pool at startup. Provider names are user-defined keys; `api_type` selects the wire adapter. The active default is set by `llm.default` and can be changed at runtime via `llm/set_default`.

```toml
[llm]
default = "openai"           # active provider (must match a key in [llm.providers.*])
# instruction = "fast"       # optional: provider for llm/instruct requests

[llm.providers.openai]
api_type = "chat_completions"
api_base_url = "https://api.openai.com/v1/chat/completions"
model = "gpt-5-nano"
temperature = 0.2
timeout_seconds = 600
# Token pricing — USD per 1 million tokens (defaults to 0.0)
input_per_million_usd = 0.05
output_per_million_usd = 0.40
cached_input_per_million_usd = 0.005

[llm.providers.codex]
api_type = "openai_responses"
model = "gpt-5.3-codex"
reasoning_effort = "none"    # "none" | "low" | "medium" | "high"
timeout_seconds = 600

[llm.providers.local]
api_type = "chat_completions"
api_base_url = "http://127.0.0.1:8081/v1/chat/completions"
model = "qwen2.5-instruct"
temperature = 0.2
timeout_seconds = 60
max_tokens = 8192

# ── Route hints ──────────────────────────────────────────────────────────────
# Symbolic names agents can request via provider_override = "hint:<name>".
# Decouples agent code from concrete provider/model choices.

[llm.routes.fast]
provider = "openai"
model = "gpt-5-nano"         # optional — defaults to provider's configured model

[llm.routes.reasoning]
provider = "codex"
# model not set → uses codex's configured model
```

`api_type` values:

| `api_type` | Endpoint | Use for |
|---|---|---|
| `"chat_completions"` | `/v1/chat/completions` | OpenAI, Ollama, LM Studio, llama.cpp, Qwen, any OpenAI-compatible server |
| `"openai_responses"` | `/v1/responses` | Codex models (`gpt-5.3-codex`), OpenAI reasoning models |
| `"dummy"` | none | Testing without an API key |
| *(unknown string)* | `/v1/chat/completions` | Falls through to `chat_completions` + warning — lets new OpenAI-compatible providers be added via config without recompiling |

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active provider on startup. Use `"dummy"` with no providers entry for keyless testing. |
| `llm.instruction` | string | none | Provider for `llm/instruct`. Falls back to `default` when absent. |
| `llm.routes.<hint>.provider` | string | — | Pool key this hint resolves to. |
| `llm.routes.<hint>.model` | string | none | Optional model override for this hint. |
| `llm.providers.<name>.api_type` | string | `"chat_completions"` | Wire adapter selector. Unknown values fall through to `chat_completions` with a warning. |
| `llm.providers.<name>.api_base_url` | string | adapter default | Endpoint URL. Override for local servers. |
| `llm.providers.<name>.model` | string | — | Model name sent in the request body. |
| `llm.providers.<name>.temperature` | float | `0.2` | Sampling temperature. Automatically omitted for `gpt-5` family models. |
| `llm.providers.<name>.reasoning_effort` | string | `"none"` | For `openai_responses`: `"none"` / `"low"` / `"medium"` / `"high"`. |
| `llm.providers.<name>.timeout_seconds` | integer | `60` | Per-request HTTP timeout in seconds. |
| `llm.providers.<name>.max_tokens` | integer | `0` | Maximum output tokens (0 = no limit). |
| `llm.providers.<name>.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.providers.<name>.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.providers.<name>.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |

Pricing fields default to `0.0` so cost is silently omitted rather than wrong when not configured.

---

## Adding a Provider via Config (no recompile)

Any OpenAI-compatible endpoint can be added as a new named provider without recompiling. Unknown `api_type` strings fall through to `chat_completions` with a warning:

```toml
[llm.providers.my_endpoint]
api_type = "chat_completions"   # or omit — same effect
api_base_url = "https://my-openai-compat.example.com/v1/chat/completions"
model = "my-model"
timeout_seconds = 60
```

Set `llm.default = "my_endpoint"` or use `llm/set_default` at runtime to activate it.

## Adding a New Wire Protocol (requires recompile)

For protocols with a genuinely different wire format (e.g. Anthropic `/v1/messages`):

1. Create `crates/araliya-llm/src/providers/{name}.rs` — implement `complete()`, `complete_stream()`, and `ping()`.
2. Add a variant to `LlmProvider` in `crates/araliya-llm/src/lib.rs`.
3. Add match arms to `LlmProvider::complete`, `complete_stream`, and `ping`.
4. Add a new `ApiType` variant and a corresponding `build_from_provider` match arm in `crates/araliya-llm/src/providers/mod.rs`.
5. Add a provider entry in `[llm.providers.*]` in `config/default.toml` (or a profile overlay).
6. Pass secrets via environment variable or `.env` (never in config files).

---

## Planned Provider Support

| Provider / adapter | Auth | Notes |
|---|---|---|
| `api_type = "chat_completions"` | `OPENAI_API_KEY` | Implemented — works with OpenAI, Ollama, LM Studio, llama.cpp, Qwen, and any OpenAI-compatible server |
| `api_type = "openai_responses"` | `OPENAI_API_KEY` | Implemented — Codex and OpenAI reasoning models |
| `api_type = "dummy"` | none | Implemented — no API key required |
| Anthropic (`/v1/messages`) | `ANTHROPIC_API_KEY` | Planned |
