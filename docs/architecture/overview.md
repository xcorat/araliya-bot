# Architecture Overview

**Status:** v0.2.0 — generic subsystem runtime · `BusHandler` trait · concurrent channel tasks · `Channel` trait · `AgentPlugin` trait · capability-scoped state.

---

## Design Principles

- **Single-process supervisor model** — all subsystems run as Tokio tasks within one process; upgradeable to OS-level processes later without changing message types
- **Star topology** — supervisor is the hub; subsystems communicate only via the supervisor, not directly with each other
- **Capability-passing** — subsystems receive only the handles they need at init; no global service locator
- **Non-blocking supervisor loop** — the supervisor is a pure router; it forwards `reply_tx` ownership to each handler and returns immediately; handlers resolve the reply in their own time (sync or via `tokio::spawn`)
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
│  ┌──────┴──────┐  ┌───────┴──────┐  ┌────────────┐  │
│  │   Agents    │  │     LLM      │  │   Tools    │  │
│  │  Subsystem  │  │  Subsystem   │  │  Subsystem │  │
│  │             │  │ DummyProvider│  │ (planned)  │  │
│  └─────────────┘  └──────────────┘  └────────────┘  │
│                                                      │
└──────────────────────────────────────────────────────┘
```

---

## Modules

| Module | File | Summary |
|--------|------|---------|
| Supervisor | `main.rs`, `supervisor/` | Async entry point; owns the event bus; routes messages between subsystems |
| Supervisor bus | `supervisor/bus.rs` | JSON-RPC 2.0-style protocol: `BusMessage` (Request/Notification), `BusPayload` enum, `BusHandle` (public API), `SupervisorBus` (owned receiver + handle) |
| Config | `config.rs` | TOML load, env overrides, path expansion; `[comms.pty]`, `[agents]`, `[llm]` sections |
| Identity | `identity.rs` | ed25519 keypair, bot_id derivation, file persistence |
| Logger | `logger.rs` | tracing-subscriber init, CLI/env/config level precedence |
| Error | `error.rs` | Typed error enum with thiserror |
| LLM providers | `llm/` | `LlmProvider` enum dispatch; `providers::build(name)` factory; `DummyProvider` |

---

## Subsystems

| Subsystem | Doc | Status |
|-----------|-----|--------|
| Comms — PTY channel | [comms.md](subsystems/comms.md) | Implemented |
| Comms — HTTP, channel plugins | [comms.md](subsystems/comms.md) | Planned |
| Memory Service | — | Planned |
| Agents | [subsystems/agents.md](subsystems/agents.md) | Implemented (`basic_chat` routes to LLM subsystem; `echo` fallback; channel mapping) |
| LLM Subsystem | [subsystems/llm.md](subsystems/llm.md) | Implemented (dummy provider; real provider support planned) |
| Tools | — | Planned |

---

## Startup Sequence

```
main()  [#[tokio::main]]
  ├─ dotenvy::dotenv()              load .env if present
  ├─ config::load()                 read default.toml + env overrides
  ├─ parse CLI `-v` flags           resolve verbosity override
  ├─ logger::init(...)              initialize logger once
  ├─ identity::setup(&config)       load or generate ed25519 keypair
  ├─ CancellationToken::new()       shared shutdown signal
  ├─ SupervisorBus::new(64)         mpsc channel; clone bus.handle before move
  ├─ spawn: ctrl_c → token.cancel() Ctrl-C handler
  ├─ LlmSubsystem::new(&config.llm) build LLM subsystem (provider from config)
  ├─ AgentsSubsystem::new(config.agents, bus_handle.clone())
  ├─ handlers = vec![Box::new(agents), Box::new(llm)]  register BusHandlers
  ├─ spawn: supervisor::run(bus, handlers)  pure router, HashMap prefix dispatch
  ├─ comms = subsystems::comms::start(...)  non-blocking; channels spawn immediately
  ├─ comms.join().await             block until all channels exit
  ├─ token.cancel()                 ensure all tasks stop if comms exits first
  └─ join supervisor task
```

---

## Supervisor Routing Model

The supervisor dispatches by method prefix and immediately forwards ownership of `reply_tx: oneshot::Sender<BusResult>` to the target subsystem. It does not await the result.

```
Request { method, payload, reply_tx }
  ├─ "agents/*"  → agents.handle_request(method, payload, reply_tx)
  ├─ "llm/*"     → llm.handle_request(method, payload, reply_tx)
  └─ unknown     → reply_tx.send(Err(ERR_METHOD_NOT_FOUND))
```

Handlers resolve `reply_tx` directly for synchronous work (echo) or from a `tokio::spawn`ed task for async work (LLM calls, future tool I/O). Adding a new subsystem is one match arm.

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
- [subsystems/agents.md](subsystems/agents.md) — agent routing, LLM wiring, method grammar
- [subsystems/llm.md](subsystems/llm.md) — LLM provider abstraction, dummy provider, adding real providers
- [subsystems/memory.md](subsystems/memory.md) — sessions, transcripts, working memory (planned)
- [subsystems/tools.md](subsystems/tools.md) — tool registry, sandboxing (planned)
