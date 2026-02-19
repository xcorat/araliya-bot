# Architecture Overview

**Status:** v0.2 — supervisor bootstrap + PTY comms channel.

---

## Design Principles

- **Single-process supervisor model** — all subsystems run as Tokio tasks within one process; upgradeable to OS-level processes later without changing message types
- **Star topology** — supervisor is the hub; subsystems communicate only via the supervisor, not directly with each other
- **Capability-passing** — subsystems receive only the handles they need at init; no global service locator
- **Plugin-based extensibility** — subsystems can load and unload plugins at runtime

---

## Process Structure

```
┌──────────────────────────────────────────────────────┐
│               SUPERVISOR (main process)              │
│          config · identity · logger · error          │
├──────────────────────────────────────────────────────┤
│                                                      │
│  ┌─────────────┐   ┌─────────────┐                  │
│  │   Comms     │   │   Memory    │   (planned)       │
│  │  Subsystem  │   │   Service   │                   │
│  │ PTY│HTTP│Ch.│   │             │                   │
│  └──────┬──────┘   └──────┬──────┘                   │
│         │                 │                          │
│  ┌──────┴─────────────────┴──────────────────┐       │
│  │      Typed Channel Router (star hub)      │       │
│  └──────┬─────────────────┬──────────────────┘       │
│         │                 │                          │
│  ┌──────┴──────┐   ┌───────┴──────┐                  │
│  │   Agents    │   │    Tools     │   (planned)       │
│  │  Subsystem  │   │  Subsystem   │                   │
│  └─────────────┘   └─────────────┘                   │
│                                                      │
└──────────────────────────────────────────────────────┘
```

---

## Modules (v0.2)

| Module | File | Summary |
|--------|------|---------|
| Supervisor | `main.rs`, `supervisor/` | Async entry point; owns the event bus; supervises subsystem tasks |
| Supervisor bus | `supervisor/bus.rs` | `CommsMessage` (oneshot reply slot) + `SupervisorBus` (mpsc channel pair) |
| Config | `config.rs` | TOML load, env overrides, path expansion, `[comms.pty]` section |
| Identity | `identity.rs` | ed25519 keypair, bot_id derivation, file persistence |
| Logger | `logger.rs` | tracing-subscriber init, level parsing |
| Error | `error.rs` | Typed error enum with thiserror |

---

## Subsystems

| Subsystem | Doc | Status |
|-----------|-----|--------|
| Comms — PTY channel | [comms.md](subsystems/comms.md) | Implemented |
| Comms — HTTP, channel plugins | [comms.md](subsystems/comms.md) | Planned |
| Memory Service | — | Planned |
| Agents | — | Planned |
| Tools | — | Planned |
| AI/LLM Provider | — | Planned |

---

## Startup Sequence (v0.2)

```
main()  [#[tokio::main]]
  ├─ dotenvy::dotenv()              load .env if present
  ├─ logger::init("info")           bootstrap logger
  ├─ config::load()                 read default.toml + env overrides
  ├─ identity::setup(&config)       load or generate ed25519 keypair
  ├─ CancellationToken::new()       shared shutdown signal
  ├─ SupervisorBus::new(64)         mpsc channel; clone comms_tx for comms
  ├─ spawn: ctrl_c → token.cancel() Ctrl-C handler
  ├─ spawn: supervisor::run(bus)    supervisor message loop
  ├─ subsystems::comms::run(...)    PTY channel — blocks until shutdown or EOF
  ├─ token.cancel()                 ensure all tasks stop if comms exits first
  └─ join supervisor task
```

---

## Identity

Each bot instance has a persistent ed25519 keypair. The `bot_id` is the first 8 hex characters of `SHA256(verifying_key)`.

- `bot_id` is stable across restarts
- Used as the identity directory name: `bot-pkey{bot_id}/`
- Future: signs events, authenticates to external services

See [identity.md](identity.md) for full details.

---

## Further Reading

- [identity.md](identity.md) — keypair lifecycle, file format, security
- [subsystems/comms.md](subsystems/comms.md) — PTY, HTTP, channel plugins
- [subsystems/memory.md](subsystems/memory.md) — sessions, transcripts, working memory
- [subsystems/agents.md](subsystems/agents.md) — agent orchestration, lanes, runs
- [subsystems/tools.md](subsystems/tools.md) — tool registry, sandboxing
- [subsystems/llm.md](subsystems/llm.md) — LLM provider client, failover
