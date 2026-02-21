# LLM Subsystem

**Status:** v0.5.0 — `LlmResponse` + `LlmUsage` + `ModelRates` · token usage deserialized from OpenAI wire format (incl. cached tokens) · cost computed per-call · per-session spend accumulated to `spend.json` · pricing rates in config.

---

## Overview

The LLM subsystem is a bus participant that handles all `llm/*` requests. It owns the configured provider and resolves each request asynchronously — the supervisor loop is never blocked on provider I/O.

The Agents subsystem uses the bus to call `llm/complete` rather than holding a direct reference to the provider. Any future subsystem can do the same.

---

## Responsibilities

- Receive `llm/complete` requests via the supervisor bus
- Forward the prompt to the configured `LlmProvider`
- Deserialize token usage from the provider response
- Compute per-call cost using configured pricing rates and log it
- Return the reply as `BusPayload::CommsMessage` (preserving `channel_id` and `usage`)
- Spawn one task per request so the supervisor loop is non-blocking

---

## Module Layout

```
src/
  llm/
    mod.rs              LlmProvider enum · LlmResponse · LlmUsage · ModelRates
    providers/
      mod.rs            build(name) factory function
      dummy.rs          DummyProvider — returns "[echo] {input}", usage: None
      openai_compatible.rs  reqwest HTTP client; deserializes usage + cached tokens
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
    pub usage: Option<LlmUsage>,   // None for DummyProvider and keyless endpoints
}
```

`LlmUsage::cost_usd(rates: &ModelRates) -> f64` applies per-million-token pricing.

---

## Provider Abstraction

`LlmProvider` is an enum over concrete implementations. Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency. Adding a backend = new module + new variant + new `complete` arm + new `build()` match arm.

```rust
pub enum LlmProvider {
    Dummy(DummyProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),
}

impl LlmProvider {
    pub async fn complete(&self, content: &str) -> Result<LlmResponse, ProviderError>;
}
```

---

## Bus Protocol

**Request method:** `"llm/complete"`

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String }`

`channel_id` is threaded through so the reply can be associated with the originating channel without extra bookkeeping by the caller.

**Reply payload:** `BusPayload::CommsMessage { channel_id, content: reply, session_id: None, usage: Option<LlmUsage> }`

`usage` is `None` when the provider does not report token counts.

---

## Request Lifecycle

```
supervisor receives Request { method: "llm/complete", payload: LlmRequest { .. }, reply_tx }
  → llm.handle_request(method, payload, reply_tx)       // supervisor returns immediately
    → tokio::spawn {
        provider.complete(&content).await
          → Ok(LlmResponse { text, usage })
              → log input_tokens, output_tokens, cached_tokens, cost_usd  [DEBUG]
              → reply_tx.send(Ok(CommsMessage { channel_id, content: text, usage }))
          → Err(e)
              → reply_tx.send(Err(BusError { .. }))
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

## Current Providers

`DummyProvider` requires no API key. It returns `"[echo] {input}"` with `usage: None`.

`OpenAiCompatibleProvider` uses `[llm.openai]` settings plus `LLM_API_KEY` from env/.env. It deserializes the OpenAI `usage` object including `prompt_tokens_details.cached_tokens`.

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
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active provider. Supported: `"dummy"`, `"openai"`. |
| `llm.openai.api_base_url` | string | OpenAI endpoint | Chat completions URL. Set to a local server for Ollama / LM Studio. |
| `llm.openai.model` | string | `"gpt-4o-mini"` | Model name sent in the request body. |
| `llm.openai.temperature` | float | `0.2` | Sampling temperature (silently omitted for `gpt-5` family). |
| `llm.openai.timeout_seconds` | integer | `60` | Per-request HTTP timeout. |
| `llm.openai.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.openai.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.openai.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |

Pricing fields default to `0.0` so cost is silently omitted rather than wrong when not configured.

---

## Adding a Real Provider

1. Create `src/llm/providers/{name}.rs` — implement `async fn complete(&self, content: &str) -> Result<LlmResponse, ProviderError>`.
2. Add a variant to `LlmProvider` in `src/llm/mod.rs`.
3. Add a match arm to `LlmProvider::complete`.
4. Add a match arm to `providers::build(name)` in `src/llm/providers/mod.rs`.
5. Update `[llm] default = "{name}"` in `config/default.toml`.
6. Pass secrets via environment variable or `.env` (never in config files).

---

## Planned Provider Support

| Provider | Auth | Notes |
|----------|------|-------|
| OpenAI-compatible | `LLM_API_KEY` | Implemented (`default = "openai"`) |
| Dummy | none | Implemented (`default = "dummy"`) |
| Anthropic | `ANTHROPIC_API_KEY` | Planned |
