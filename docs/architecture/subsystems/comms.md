# Comms Subsystem

**Status:** v0.2.0 — `Channel` trait · concurrent channel tasks · `CommsState` capability boundary · intra-subsystem event queue · `start()` returns `SubsystemHandle`.

---

## Overview

The Comms subsystem manages all external I/O for the bot. It provides multiple transport layers (PTY, HTTP) and hosts pluggable **channel plugins** for messaging services (Telegram, Slack, Discord, etc.).

Channels are plugins *within* Comms. They only handle send/recv of messages — session logic and routing lives in the Agents subsystem.

---

## Components

### PTY Layer — ✓ Implemented
- Console I/O (stdin/stdout)
- Auto-loads when no other channel is enabled (`[comms.pty] enabled = true` in config)
- Can be force-disabled with `enabled = false`
- Reads lines from stdin, routes each through the supervisor bus via `BusHandle::request`, prints the reply
- Multiple PTY instances are supported: each sends `"agents"` with its own `channel_id` (e.g. `"pty0"`, `"pty1"`); the embedded `oneshot` in each request carries the correct return address independently
- Ctrl-C sends a shutdown signal via `CancellationToken`; all tasks shut down gracefully
- Used for local testing and development

**Source:** `src/subsystems/comms/pty.rs`

### HTTP Layer — Planned
- REST API endpoints for session and message management
- WebSocket support (future, streaming events)

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
      mod.rs            — Channel trait; start(config, bus, shutdown) → SubsystemHandle
      state.rs          — CommsState (private bus, send_message, report_event, CommsEvent)
      pty.rs            — PtyChannel: Channel
```

### Capability boundary

`CommsState` is the only surface channels see. The raw `BusHandle` is private;
channels call typed methods:

| Method | Description |
|--------|-------------|
| `send_message(channel_id, content)` | Route a message to the agents subsystem; return the reply string. |
| `report_event(CommsEvent)` | Signal the subsystem manager (non-blocking `try_send`). |

`CommsEvent` variants: `ChannelShutdown { channel_id }`, `SessionStarted { channel_id }`.

### Concurrent channel lanes

`comms::start()` is **synchronous** — it spawns all enabled channels into a
`JoinSet` and returns a `SubsystemHandle` immediately. Channels run as
independent concurrent tasks. The manager task additionally `select!`s on an
internal `mpsc` channel for `CommsEvent`s from running channels.

If any channel exits with an error, the shared `CancellationToken` is cancelled
so sibling channels and the supervisor all shut down cooperatively.

### Channel trait

```rust
pub trait Channel: Send + 'static {
    fn id(&self) -> &str;
    fn run(
        self: Box<Self>,
        state: Arc<CommsState>,
        shutdown: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>>;
}
```

Channels receive `Arc<CommsState>` at spawn time — they do not hold it in their
struct. This keeps ownership clear: the subsystem creates and owns `CommsState`;
channels borrow a reference-counted handle while they run.

### Message flow (current — basic_chat default)

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
# PTY auto-loads when no other channel is enabled.
# Set to false to suppress even when no other channel is present.
enabled = true
```

`Config::comms_pty_should_load()` is the hook for auto-enable logic —
will return `true` when no other channel is configured, even if the key is absent.
