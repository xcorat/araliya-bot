# Agents Subsystem

**Status:** v0.4.0 — `Agent` trait (with `session_id`) · `AgentsState` capability boundary · `BusHandler` impl · agent dispatch · **`ChatCore` composition layer** · `SessionChatPlugin` with memory integration and session reload · session query handlers (`agents/sessions`, `agents/sessions/detail`).

---

## Overview

The Agents subsystem receives agent-targeted requests from the supervisor bus and routes each message to an agent. Agent handlers are non-blocking: each handler receives ownership of `reply_tx` and resolves it in its own time — synchronously for simple agents, via `tokio::spawn` for agents that perform I/O.

---

## Agents

| Agent | Behaviour |
|-------|-----------|
| `basic_chat` | Calls `ChatCore::basic_complete` → `llm/complete` on the bus. |
| `chat` | Session-aware chat via `SessionChatPlugin`. Creates or reloads a memory session (via `session_id`), appends user/assistant turns to a Markdown transcript, and injects recent history as LLM context. Returns `session_id` in the reply. Default agent. Configured with `memory = ["basic_session"]`. |
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

- **`echo`** — `EchoAgent::handle` resolves `reply_tx` inline; zero latency.
- **`basic_chat`** — `BasicChatPlugin::handle` moves `reply_tx` into a
  `tokio::spawn`ed task that calls `AgentsState::complete_via_llm`.
- **`chat`** — `SessionChatPlugin::handle` spawns a task that initialises a
  memory session on first use, appends to transcript, builds context, calls
  `ChatCore::basic_complete`, and appends the LLM reply.

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

## Agent Architecture

`Agent` is the extension trait for all agent implementations:

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

Agents are stored in a `HashMap<String, Box<dyn Agent>>` inside
`AgentsSubsystem`. Resolution order (by `id()`) maps to the routing priority
table above.

> **Naming convention:** `Agent` for autonomous actors in the agents subsystem;
> `Plugin` is reserved for capability extensions in the future tools subsystem.

### Chat-family composition (`ChatCore`)

Chat-family agents (`basic_chat`, `chat`, and future variants) share logic
through composition rather than inheritance:

```
src/subsystems/agents/chat/
├── mod.rs           # feature-gated re-exports
├── core.rs          # ChatCore — shared building blocks
├── basic_chat.rs    # BasicChatPlugin (thin wrapper over ChatCore)
└── session_chat.rs  # SessionChatPlugin (ChatCore + future extensions)
```

`ChatCore` is a stateless struct providing composable methods:

```rust
impl ChatCore {
    pub async fn basic_complete(state, channel_id, content) -> BusResult;
    // Future: prompt_template(), inject_memory(), tool_dispatch(), ...
}
```

Each chat agent calls `ChatCore` methods and layers its own behaviour on top.
This avoids code duplication while allowing progressive enhancement:

```
ChatCore::basic_complete()        ← shared logic
    ↑                    ↑
BasicChatPlugin     SessionChatPlugin  (core + session/memory/tools)
                         ↑
                  AdvancedChatPlugin   (future — further extensions)
```

### Capability boundary — `AgentsState`

Agents receive `Arc<AgentsState>`, not a raw `BusHandle`. Available methods:

| Method | Description |
|--------|-------------|
| `complete_via_llm(channel_id, content)` | Forward to `llm/complete` on the bus; return `BusResult`. |
| `memory` (field) | `Option<Arc<MemorySystem>>` — create/load sessions. Only present when `subsystem-memory` feature is enabled. |
| `agent_memory` (field) | `HashMap<String, Vec<String>>` — per-agent memory store requirements from config. |

The raw bus is private to `AgentsState`. Agents cannot call arbitrary bus
targets.

## Session queries

The agents subsystem intercepts two bus methods before agent routing:

| Method | Payload | Response |
|--------|---------|----------|
| `agents/sessions` | `Empty` | `JsonResponse` — JSON array of all sessions (id, created_at, store_types, last_agent) |
| `agents/sessions/detail` | `SessionQuery { session_id }` | `JsonResponse` — session metadata + full transcript |

These are handled directly by `AgentsSubsystem` (not routed to individual agents). When the `subsystem-memory` feature is disabled, sessions returns `[]` and detail returns an error.

## Initialisation

`AgentsSubsystem::new(config: AgentsConfig, bus: BusHandle, memory: Option<Arc<MemorySystem>>)` — the `BusHandle`
is injected at init and wrapped inside `AgentsState`. The optional `MemorySystem` is passed through when the `subsystem-memory` feature is enabled. Built-in agents
(`EchoAgent`, `BasicChatPlugin`, `SessionChatPlugin`) are registered behind
Cargo feature gates; the `enabled` list controls which ones are reachable via
routing.

---

## Config

```toml
[agents]
default = "chat"

[agents.routing]
# pty0 = "echo"

[agents.chat]
memory = ["basic_session"]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.default` | string | `"chat"` | Which agent handles unrouted messages. |
| `agents.routing` | map\<string,string\> | `{}` | Optional `channel_id → agent_id` routing overrides. |
| `agents.{id}.enabled` | bool | `true` | Set to `false` to disable without removing the section. |
| `agents.{id}.memory` | array\<string\> | `[]` | Memory store types this agent requires (e.g. `["basic_session"]`). |
