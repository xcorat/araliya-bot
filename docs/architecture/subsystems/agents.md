# Agents Subsystem

**Status:** v0.2.0 — `AgentPlugin` trait · `AgentsState` capability boundary · `BusHandler` impl · plugin dispatch via `HashMap`.

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

`AgentsSubsystem` implements `BusHandler` with prefix `"agents"`. The supervisor
calls `handle_request` and returns immediately:

- **`echo`** — `EchoPlugin::handle` resolves `reply_tx` inline; zero latency.
- **`basic_chat`** — `BasicChatPlugin::handle` moves `reply_tx` into a
  `tokio::spawn`ed task that calls `AgentsState::complete_via_llm`.

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

## Plugin Architecture

`AgentPlugin` is the extension trait for all agent implementations:

```rust
pub trait AgentPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        channel_id: String,
        content: String,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

Plugins are stored in a `HashMap<String, Box<dyn AgentPlugin>>` inside
`AgentsSubsystem`. Resolution order (by `id()`) maps to the routing priority
table above.

### Capability boundary — `AgentsState`

Plugins receive `Arc<AgentsState>`, not a raw `BusHandle`. Available methods:

| Method | Description |
|--------|-------------|
| `complete_via_llm(channel_id, content)` | Forward to `llm/complete` on the bus; return `BusResult`. |

The raw bus is private to `AgentsState`. Plugins cannot call arbitrary bus
targets.

## Initialisation

`AgentsSubsystem::new(config: AgentsConfig, bus: BusHandle)` — the `BusHandle`
is injected at init and wrapped inside `AgentsState`. Built-in plugins
(`EchoPlugin`, `BasicChatPlugin`) are registered unconditionally; the `enabled`
list controls which ones are reachable via routing.

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
