# Agents Subsystem

**Status:** Implemented — `basic_chat` routes to LLM subsystem via bus; `echo` fallback; channel mapping.

---

## Overview

The Agents subsystem receives agent-targeted requests from the supervisor bus and routes each message to an agent. Agent handlers are non-blocking: each handler receives ownership of `reply_tx` and resolves it in its own time — synchronously for simple agents, via `tokio::spawn` for agents that perform I/O.

---

## Agents

| Agent | Behaviour |
|-------|-----------|
| `basic_chat` | Calls `llm/complete` on the bus and forwards the reply. Default agent. |
| `echo` | Returns the input unchanged. Used as safety fallback when `enabled` is empty. |

---

## Routing

Agents are resolved in this priority order:

1. Explicit `{agent_id}` from the method path
2. Channel mapping: `channel_id → agent_id` in `[agents.channel_map]`
3. Default agent: first entry in `agents.enabled` (falls back to `echo` if `enabled` is empty)

---

## Method Grammar

- `agents` — default agent, default action
- `agents/{agent_id}` — explicit agent, default action
- `agents/{agent_id}/{action}` — explicit agent + action (`{action}` accepted but not yet differentiated)

---

## Handle Request Contract

```rust
pub fn handle_request(
    &self,
    method: &str,
    payload: BusPayload,
    reply_tx: oneshot::Sender<BusResult>,
)
```

The supervisor passes `reply_tx` ownership here and returns immediately. The handler resolves it:

- **`echo`** — calls `reply_tx.send(Ok(...))` inline; zero latency.
- **`basic_chat`** — moves `reply_tx` into a `tokio::spawn`ed task; the task sends the reply when the LLM subsystem responds.

---

## basic_chat Flow

```
handle_request("agents", CommsMessage { channel_id, content }, reply_tx)
  → resolve agent → "basic_chat"
  → tokio::spawn {
      bus.request("llm/complete", LlmRequest { channel_id, content }).await
        → LlmSubsystem.handle_request → DummyProvider::complete
        ← Ok(CommsMessage { channel_id, content: "[echo] {input}" })
      reply_tx.send(Ok(CommsMessage { .. }))
    }
```

---

## Initialisation

`AgentsSubsystem::new(config: AgentsConfig, bus: BusHandle)` — the `BusHandle` is injected at init, consistent with the capability-passing pattern. Agents that need bus access use it; agents that do not (echo) ignore it.

---

## Config

```toml
[agents]
enabled = ["basic_chat"]

[agents.channel_map]
# pty0 = "echo"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.enabled` | array\<string\> | `["basic_chat"]` | Ordered enabled agents. First entry is the default fallback. |
| `agents.channel_map` | map\<string,string\> | `{}` | Optional `channel_id → agent_id` routing overrides. |
