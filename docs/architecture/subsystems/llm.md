# LLM Subsystem

**Status:** v0.3.0 — `DummyProvider` and `OpenAiCompatibleProvider` implemented.

---

## Overview

The LLM subsystem is a bus participant that handles all `llm/*` requests. It owns the configured provider and resolves each request asynchronously — the supervisor loop is never blocked on provider I/O.

The Agents subsystem uses the bus to call `llm/complete` rather than holding a direct reference to the provider. Any future subsystem can do the same.

---

## Responsibilities

- Receive `llm/complete` requests via the supervisor bus
- Forward the prompt to the configured `LlmProvider`
- Return the reply as `BusPayload::CommsMessage` (preserving `channel_id`)
- Spawn one task per request so the supervisor loop is non-blocking

---

## Module Layout

```
src/
  llm/
    mod.rs              LlmProvider enum + ProviderError
    providers/
      mod.rs            build(name) factory function
      dummy.rs          DummyProvider — returns "[echo] {input}"
  subsystems/
    llm/
      mod.rs            LlmSubsystem — handle_request, tokio::spawn per call
```

---

## Provider Abstraction

`LlmProvider` is an enum over concrete implementations. Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency. Adding a backend = new module + new variant + new `complete` arm + new `build()` match arm.

```rust
pub enum LlmProvider {
    Dummy(DummyProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),  // reqwest-based; configurable base URL
}

impl LlmProvider {
    pub async fn complete(&self, content: &str) -> Result<String, ProviderError>;
}
```

---

## Bus Protocol

**Request method:** `"llm/complete"`

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String }`

`channel_id` is threaded through so the reply can be associated with the originating channel without extra bookkeeping by the caller.

**Reply payload:** `BusPayload::CommsMessage { channel_id, content: reply }`

---

## Request Lifecycle

```
supervisor receives Request { method: "llm/complete", payload: LlmRequest { .. }, reply_tx }
  → llm.handle_request(method, payload, reply_tx)       // supervisor returns immediately
    → tokio::spawn {
        provider.complete(&content).await
          → Ok(reply)  → reply_tx.send(Ok(CommsMessage { channel_id, content: reply }))
          → Err(e)     → reply_tx.send(Err(BusError { .. }))
      }
```

---

## Current Providers

`DummyProvider` requires no API key. It returns `"[echo] {input}"` synchronously.

`OpenAiCompatibleProvider` uses `[llm.openai]` settings plus `LLM_API_KEY` from env/.env.

**Use:** verifying the full PTY → agents → bus → LLM → bus → PTY round-trip without an API key (when PTY runtime loading is re-enabled).

---

## Configuration

```toml
[llm]
default = "openai"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active provider. Supported now: `"dummy"`, `"openai"`. |

---

## Adding a Real Provider

1. Create `src/llm/providers/{name}.rs` — implement `async fn complete(&self, content: &str) -> Result<String, ProviderError>`.
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
