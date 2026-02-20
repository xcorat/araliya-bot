# Comms Subsystem

**Status:** v0.6.1 — concurrent channel tasks · `CommsState` capability boundary · intra-subsystem event queue · `start()` returns `SubsystemHandle` · PTY runtime is conditional when stdio management is active · **HTTP channel split into `http/` module (mod, api, ui) with full `/api/` surface (health, message, sessions, session detail, session memory, session files) · POST body parsing · session-id threading · optional UI backend delegation.**

---

## Overview

The Comms subsystem manages all external I/O for the bot. It provides multiple transport layers (PTY, HTTP) and hosts pluggable **channel plugins** for messaging services (Telegram, Slack, Discord, etc.).

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

### HTTP Layer — Implemented
- Single HTTP channel on a configurable bind address (default `127.0.0.1:8080`)
- Request parsing supports both GET and POST methods with Content-Length body reading
- API routes under the `/api/` prefix:
  - `GET  /api/health`              — returns enriched health JSON (bot_id, llm_provider, model, timeout, tools, session_count)
  - `POST /api/message`             — accepts `{"message", "session_id?", "mode?"}`, forwards to agents via bus with session-id threading, returns `MessageResponse` JSON with `session_id`
  - `GET  /api/sessions`            — returns session list from agents/memory subsystem
  - `GET  /api/session/{session_id}` — returns session detail (metadata + transcript) from agents/memory subsystem
  - `GET  /api/sessions/{session_id}/memory` — returns working memory payload for session status view
  - `GET  /api/sessions/{session_id}/files` — returns file list (`kv.json`, `transcript.md`, etc.) with size and modified timestamp
- Browser favicon requests are handled with `GET /favicon.ico -> 204 No Content` to avoid UI console 404 noise
- When the UI subsystem is enabled (`[ui.svui]`), non-API GET paths are delegated to the active `UiServeHandle`; the HTTP channel receives the handle at construction
- When the UI subsystem is disabled, non-API paths return 404
- Raw TCP listener with minimal request parsing (no framework dependency)

**Source:** `src/subsystems/comms/http/` (mod.rs — server loop & dispatch, api.rs — API route handlers, ui.rs — welcome page & UI delegation)

### Channel Plugins — Planned
- Pluggable, loadable/unloadable at runtime
- Each channel handles: receive inbound message → publish to event bus, subscribe to responses → deliver outbound message
- Planned channels: Telegram, Slack, Discord, Email, SMS, WebChat

---

## Architecture

### Module layout

```
src/
  subsystems/
    runtime.rs          — Component trait, SubsystemHandle, spawn_components
    comms/
      mod.rs            — start(config, bus, shutdown, [ui_handle]) → SubsystemHandle
      state.rs          — CommsState (private bus, send_message, management_http_get, request_sessions, request_session_detail, request_session_memory, request_session_files, report_event, CommsEvent, CommsReply)
      pty.rs            — PtyChannel: Component
      http/
        mod.rs          — HttpChannel: Component (server loop, connection dispatch, request parsing, response helpers)
        api.rs          — API route handlers (/api/health, /api/message, /api/sessions, /api/session/{id}, /api/sessions/{id}/memory, /api/sessions/{id}/files)
        ui.rs           — UI route handlers (root welcome page, /ui/* delegation, 404 catch-all)
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
| `send_message(channel_id, content, session_id)` | Route a message to the agents subsystem; return `CommsReply` (reply string + optional session_id). |
| `management_http_get()` | Request health/status JSON from the management bus route. |
| `request_sessions()` | Request session list JSON from the agents subsystem via `agents/sessions`. |
| `request_session_detail(session_id)` | Request session detail JSON from agents via `agents/sessions/detail`. |
| `request_session_memory(session_id)` | Request session working-memory JSON from agents via `agents/sessions/memory`. |
| `request_session_files(session_id)` | Request session file list JSON from agents via `agents/sessions/files`. |
| `report_event(CommsEvent)` | Signal the subsystem manager (non-blocking `try_send`). |

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
```

When stdio management is connected, Comms skips real PTY startup and management `/chat` acts as a virtual PTY stream.
