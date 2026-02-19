# Comms Subsystem

**Status:** v0.0.4 — PTY channel implemented with agents-first routing. HTTP and channel plugins planned.

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
  supervisor/
    bus.rs          — event bus protocol (BusMessage, BusPayload, BusHandle, SupervisorBus)
    mod.rs          — supervisor run loop; routes on method string
  subsystems/
    mod.rs
    comms/
      mod.rs        — run(config, bus: BusHandle, shutdown)
      state.rs      — CommsState { bus: BusHandle }
      pty.rs        — PTY channel task
```

### Message flow (current — agents default basic_chat)

```
PTY stdin
  → BusHandle::request("agents", CommsMessage { channel_id: "pty0", content })
    → SupervisorBus::rx (mpsc, bounded 64)
      → supervisor::run  method prefix dispatch ("agents/*")
        → agents subsystem resolves route (method agent_id, else channel_id map, else default)
        → selected agent returns CommsMessage via reply_tx (oneshot)
          (default: `basic_chat`; fallback: `echo` when `agents.enabled` is empty)
    ← BusResult::Ok(CommsMessage { .. })
  → pty::run prints reply to stdout
PTY stdout
```

### Shutdown flow

```
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

---

## Message Flow (future — with Agents)

```
Channel plugin (inbound)
  → Comms subsystem
    → Supervisor event bus
      → Agents subsystem
        → (response via event bus)
      → Comms subsystem
    → Channel plugin (outbound)
```
