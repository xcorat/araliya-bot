# Comms Subsystem

**Status:** v0.8.0 — concurrent channel tasks · `CommsState` capability boundary · intra-subsystem event queue · `start()` returns `SubsystemHandle` · PTY runtime is conditional when stdio management is active · HTTP channel split into `http/` module · **Axum channel (`channel-axum`) with full `/api/` surface including `POST /api/message/stream` SSE endpoint · `stream_direct()` on `CommsState` for direct LLM streaming · `thinking` field threaded through `CommsReply` and JSON responses · Telegram channel (teloxide)**.

---

## Overview

The Comms subsystem manages all external I/O for the bot. It provides multiple transport layers (PTY, HTTP, Telegram) and hosts pluggable **channel plugins** for additional messaging services (Slack, Discord, etc.).

Channels are plugins *within* Comms. They only handle send/recv of messages — session logic and routing lives in the Agents subsystem.

---

## Components

### PTY Layer — Implemented
- Console I/O (stdin/stdout)
- Enabled by config in normal interactive runs
- Auto-disabled when supervisor stdio adapter owns stdio (virtual `/chat` route is used instead)
- Reads lines from stdin, routes each through the supervisor bus via `BusHandle::request`, prints the reply
- Multiple PTY instances are supported: each sends `"agents"` with its own `channel_id` (e.g. `"pty0"`, `"pty1"`); the embedded `oneshot` in each request carries the correct return address independently
- Ctrl-C sends a shutdown signal via `CancellationToken`; all tasks shut down gracefully
- Used for local testing and development

**Source:** `src/subsystems/comms/pty.rs`

### Virtual PTY via Supervisor Stdio Adapter — Implemented
- Lives in `src/supervisor/adapters/stdio.rs` (internal to supervisor)
- Enabled when stdio is non-interactive (management/IPC attachment)
- Performs a minimal slash protocol translation for tty lines:
  - First non-whitespace character **must** be `/`
  - Interactive mode shows a `# ` prompt before each command
  - `/chat <message>` → `BusPayload::CommsMessage { channel_id: "pty0", content }` to `agents`
  - `/health`, `/status`, `/subsys`, `/exit` → supervisor control plane commands
  - `/help` prints protocol usage
- Keeps comms behavior consistent by reusing the virtual PTY channel id (`pty0`)

### HTTP Layer — Implemented (legacy)
- Raw TCP listener with minimal request parsing (no framework dependency)
- Superseded by the Axum channel for new deployments; retained for minimal-feature builds

**Source:** `src/subsystems/comms/http/` (mod.rs — server loop & dispatch, api.rs — API route handlers, ui.rs — welcome page & UI delegation)

### Axum Channel — Implemented (`channel-axum`, default)
- axum/hyper-based HTTP channel on a configurable bind address (default `127.0.0.1:8080`)
- Drop-in replacement for the legacy HTTP channel; enabled by `channel-axum` feature flag (on by default)
- Full `/api/` surface:
  - `GET  /api/health`                          — enriched health JSON
  - `POST /api/health/refresh`                  — trigger live subsystem health re-check
  - `GET  /api/tree`                            — component tree (no private data)
  - `POST /api/message`                         — buffered chat; returns `{"reply", "thinking", "session_id", ...}`
  - `POST /api/message/stream`                  — **SSE streaming**; emits `thinking`, `content`, and `done` events
  - `GET  /api/sessions`                        — session list
  - `GET  /api/agents`                          — agent list
  - `GET  /api/agents/{agent_id}/kg`            — knowledge graph for agent's KGDocStore (reads from agent fs directly)
  - `GET  /api/memory/agents/{agent_id}/kg`     — knowledge graph via memory bus handler (`memory/kg_graph`)
  - `GET  /api/session/{session_id}`            — session detail (metadata + transcript)
  - `GET  /api/sessions/{session_id}/memory`    — working memory
  - `GET  /api/sessions/{session_id}/debug`     — per-turn debug data
  - `GET  /api/sessions/{session_id}/files`     — session file list
- Non-API paths delegated to `UiServeHandle` when the UI subsystem is enabled; otherwise 404

#### SSE Streaming (`POST /api/message/stream`)

Calls `CommsState::stream_direct()` which issues `llm/stream` on the bus and returns an `mpsc::Receiver<StreamChunk>`. The receiver is converted to a `futures::Stream` via `stream::unfold` and served as `text/event-stream`:

```
event: thinking
data: {"delta": "..."}   ← reasoning_content chunks (Qwen3, DeepSeek-R1, QwQ)

event: content
data: {"delta": "..."}   ← answer token deltas

event: done
data: {"usage": {...}}   ← final usage totals; stream closes
```

Bypasses session history (direct LLM call). The frontend uses this endpoint for all sends, pre-creating an assistant message and updating it reactively as chunks arrive.

**Source:** `src/subsystems/comms/axum_channel/` (mod.rs — router, state, server loop; api.rs — all API handlers; ui.rs — SPA fallback)

### Telegram Channel — Implemented
- Connects to Telegram Bot API via `teloxide`
- Enabled by Cargo feature `channel-telegram` and config `comms.telegram.enabled = true`
- Requires `TELEGRAM_BOT_TOKEN` env var; gracefully exits if missing
- Receives text messages, routes through `CommsState::send_message`, replies in-chat
- Shutdown via shared `CancellationToken` (`select!` on dispatcher + shutdown signal)

**Source:** `src/subsystems/comms/telegram.rs`

### Channel Plugins — Planned
- Pluggable, loadable/unloadable at runtime
- Each channel handles: receive inbound message → publish to event bus, subscribe to responses → deliver outbound message
- Planned channels: Slack, Discord, Email, SMS, WebChat

---

## Architecture

### Module layout

```
src/
  subsystems/
    runtime.rs          — Component trait, SubsystemHandle, spawn_components
    comms/
      mod.rs            — start(config, bus, shutdown, [ui_handle]) → SubsystemHandle
      state.rs          — CommsState · send_message · stream_direct · management_* · request_* · CommsReply · CommsEvent
      pty.rs            — PtyChannel: Component
      http/             — (legacy) raw TCP HTTP channel
        mod.rs          — HttpChannel: Component
        api.rs          — API route handlers
        ui.rs           — UI delegation
      axum_channel/     — (default) axum/hyper HTTP channel
        mod.rs          — AxumChannel: Component · AxumState · build_router
        api.rs          — All API handlers incl. message_stream (SSE)
        ui.rs           — SPA fallback
      telegram.rs       — TelegramChannel: Component
    ui/
      mod.rs            — UiServe trait, UiServeHandle, start(config) → Option<UiServeHandle>
      svui.rs           — SvuiBackend: UiServe (static file serving, built-in placeholder)
```

### Capability boundary

`CommsState` is the only surface channels see. The raw `BusHandle` is private;
channels call typed methods:

| Method | Description |
|--------|-------------|
| `send_message(channel_id, content, session_id, agent_id)` | Route a message to the agents subsystem; return `CommsReply` (reply, optional session_id, optional thinking). |
| `stream_direct(channel_id, content, system)` | Issue `llm/stream` on the bus; return `mpsc::Receiver<StreamChunk>` for token-by-token delivery. Bypasses session history. |
| `management_http_get()` | Request health/status JSON from the management bus route. |
| `management_health_refresh()` | Trigger a live health re-check across all subsystems; return updated health JSON. |
| `management_http_tree()` | Request component tree JSON. |
| `request_sessions()` | Request session list JSON from the agents subsystem. |
| `request_agents()` | Request agent list JSON from the agents subsystem. |
| `request_session_detail(session_id, agent_id)` | Request session detail (metadata + transcript). |
| `request_session_memory(session_id, agent_id)` | Request working memory content. |
| `request_session_files(session_id, agent_id)` | Request session file list. |
| `request_session_debug(session_id, agent_id)` | Request per-turn debug data. |
| `request_agent_kg(agent_id)` | Request knowledge graph for an agent's KGDocStore (direct fs read via agents subsystem). |
| `request_memory_kg(agent_id)` | Request knowledge graph via the memory bus handler (`memory/kg_graph`). Preferred for UI endpoints. |
| `report_event(CommsEvent)` | Signal the subsystem manager (non-blocking `try_send`). |

`CommsReply` carries `reply: String`, `session_id: Option<String>`, and `thinking: Option<String>`. The `thinking` field is populated when the underlying agent's LLM call produced reasoning content.

`CommsEvent` variants: `ChannelShutdown { channel_id }`, `SessionStarted { channel_id }`.

### Concurrent channel lanes

`comms::start()` is **synchronous** — it spawns all enabled channels into a
`JoinSet` and returns a `SubsystemHandle` immediately. Channels run as
independent concurrent tasks. The manager task additionally `select!`s on an
internal `mpsc` channel for `CommsEvent`s from running channels.

If any channel exits with an error, the shared `CancellationToken` is cancelled
so sibling channels and the supervisor all shut down cooperatively.

### Channel implementation

Channels implement the generic [`Component` trait](../standards/runtime.md) from `subsystems/runtime.rs`:

```rust
pub trait Component: Send + 'static {
    fn id(&self) -> &str;
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}
```

`Arc<CommsState>` and any other shared state are captured at construction — not passed to `run`.

### Message flow (real PTY lane)

```
PTY stdin
  → CommsState::send_message("pty0", content)           [typed — no raw bus]
    → BusHandle::request("agents", CommsMessage { channel_id, content })
      → SupervisorBus::rx (mpsc, bounded 64)
        → supervisor: HashMap dispatch (prefix = "agents")
          → AgentsSubsystem::handle_request   ← supervisor returns immediately
            → resolve plugin → basic_chat
              → tokio::spawn {
                  AgentsState::complete_via_llm(channel_id, content)  [typed]
                    → BusHandle::request("llm/complete", LlmRequest { .. })
                      → supervisor: dispatch (prefix = "llm")
                        → LlmSubsystem::handle_request
                          → tokio::spawn {
                              DummyProvider::complete(content)
                                → Ok("[echo] {content}")
                              reply_tx.send(Ok(CommsMessage { .. }))
                            }
                  reply_tx.send(Ok(CommsMessage { .. }))
                }
  ← Ok(reply)
  → pty prints reply to stdout
PTY stdout
```

For `echo`: `reply_tx` resolved inline, no spawn.
Ctrl-C
  → tokio::signal::ctrl_c()
    → CancellationToken::cancel()
      → pty::run select! branch fires → prints shutdown notice → returns Ok(())
      → supervisor::run select! branch fires → returns
  → main joins both tasks → process exits cleanly
```

---

## Config

```toml
[comms.pty]
# Real PTY lane for interactive stdin/stdout.
enabled = true

[comms.http]
# HTTP channel — API under /api/, UI on other paths when [ui.svui] enabled.
enabled = true
bind = "127.0.0.1:8080"

[comms.telegram]
# Telegram channel — requires TELEGRAM_BOT_TOKEN env var.
enabled = false
```

When stdio management is connected, Comms skips real PTY startup and management `/chat` acts as a virtual PTY stream.
