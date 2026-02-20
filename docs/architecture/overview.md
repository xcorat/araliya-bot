# Architecture Overview

**Status:** v0.3.0 — generic subsystem runtime · `BusHandler` trait · concurrent channel tasks · `Component` trait · `Agent` trait · `OpenAiCompatibleProvider` · capability-scoped state · **Compile-time modularity via Cargo Features** · **Chat-family agent composition (`ChatCore`)** · **Memory subsystem with pluggable stores (`basic_session`)** · **UI subsystem (`svui` backend).**

---

## Design Principles

- **Single-process supervisor model** — all subsystems run as Tokio tasks within one process; upgradeable to OS-level processes later without changing message types
- **Star topology** — supervisor is the hub; subsystems communicate only via the supervisor, not directly with each other. Per-hop overhead is ~100–500 ns (tokio mpsc + oneshot); replies bypass the supervisor via direct oneshot channels. This is negligible next to the I/O the bus orchestrates (LLM/HTTP calls in the hundreds-of-ms range). The central hub provides free centralised logging, cancellation, and a future permission gate without the complexity of actor mailboxes or external brokers
- **Capability-passing** — subsystems receive only the handles they need at init; no global service locator
- **Non-blocking supervisor loop** — the supervisor is a pure router; it forwards `reply_tx` ownership to each handler and returns immediately; handlers resolve the reply in their own time (sync or via `tokio::spawn`)
- **Split planes** — subsystem traffic uses the supervisor bus; supervisor management uses an internal control plane (not routed through bus methods)
- **Plugin-based extensibility** — subsystems can load and unload plugins at runtime
- **Agent / Plugin distinction** — `Agent` trait for autonomous actors in the agents subsystem; `Plugin` (future) for capability extensions in the tools subsystem
- **Compile-time Modularity** — Subsystems (`agents`, `llm`, `comms`, `memory`) and agents can be disabled via Cargo features to optimize binary size and memory footprint.

---

## Process Structure

```
┌──────────────────────────────────────────────────────┐
│               SUPERVISOR (main process)              │
│          config · identity · logger · error          │
├──────────────────────────────────────────────────────┤
│                                                      │
│  ┌─────────────┐   ┌─────────────┐                  │
│  │   Comms     │   │   Memory    │   Implemented     │
│  │  Subsystem  │   │   System    │                   │
│  │ PTY│HTTP│Ch.│   │basic_session│                   │
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

## Modularity (Features)

Building on the ZeroClaw standard, Araliya supports swappable subsystems and plugins:

| Feature | Scope | Description |
|---------|-------|-------------|
| `subsystem-agents` | Agents | Routing engine for agent workflows. |
| `subsystem-llm` | LLM | Completion provider subsystem. |
| `subsystem-comms` | Comms | I/O channel management. |
| `subsystem-memory` | Memory | Session management and pluggable memory stores. |
| `plugin-echo` | Agent | Echo agent for the Agents subsystem. |
| `plugin-basic-chat` | Agent | Basic chat agent — minimal LLM pass-through (requires `subsystem-llm`). |
| `plugin-chat` | Agent | Session-aware chat agent — extends `ChatCore` with memory integration (requires `subsystem-llm`, `subsystem-memory`). |
| `channel-pty` | Channel | Local console PTY channel. |
| `channel-http` | Channel | HTTP channel — API routes under `/api/`, optional UI serving. |
| `subsystem-ui` | UI | Display-oriented interface providers. |
| `ui-svui` | UI backend | Svelte-based web UI — static file serving (requires `subsystem-ui`). |

---

## Modules

| Module | File | Summary |
|--------|------|---------|
| Supervisor | `main.rs`, `supervisor/` | Async entry point; owns the event bus; routes messages between subsystems |
| Supervisor bus | `supervisor/bus.rs` | JSON-RPC 2.0-style protocol: `BusMessage` (Request/Notification), `BusPayload` enum, `BusHandle` (public API), `SupervisorBus` (owned receiver + handle) |
| Supervisor control | `supervisor/control.rs` | Thin supervisor-internal management interface (typed commands/responses), intended transport target for stdio/http adapters |
| Config | `config.rs` | TOML load, env overrides, path expansion; `[comms.pty]`, `[comms.http]`, `[agents]`, `[llm]`, `[memory]`, `[ui]` sections |
| Identity | `identity.rs` | ed25519 keypair, bot_id derivation, file persistence |
| Logger | `logger.rs` | tracing-subscriber init, CLI/env/config level precedence |
| Error | `error.rs` | Typed error enum with thiserror |
| LLM providers | `llm/` | `LlmProvider` enum dispatch; `providers::build(name)` factory; `DummyProvider` |

---

## Subsystems

| Subsystem | Doc | Status |
|-----------|-----|--------|
| Comms — PTY channel | [comms.md](subsystems/comms.md) | Implemented (Optional feature: `channel-pty`) |
| Comms — HTTP channel | [comms.md](subsystems/comms.md) | Implemented (Optional feature: `channel-http`) |
| UI — svui backend | [subsystems/ui.md](subsystems/ui.md) | Implemented (Optional features: `subsystem-ui`, `ui-svui`) |
| Memory System | [subsystems/memory.md](subsystems/memory.md) | Implemented — `basic_session` store (Optional feature: `subsystem-memory`) |
| Agents | [subsystems/agents.md](subsystems/agents.md) | Implemented (Optional features: `plugin-echo`, `plugin-basic-chat`, `plugin-chat`) |
| LLM Subsystem | [subsystems/llm.md](subsystems/llm.md) | Implemented (Optional feature: `subsystem-llm`) |
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
  ├─ (conditional) MemorySystem::new(identity_dir, config)  init memory
  ├─ CancellationToken::new()       shared shutdown signal
  ├─ SupervisorBus::new(64)         mpsc channel; clone bus.handle before move
  ├─ SupervisorControl::new(32)     supervisor-internal control channel
  ├─ spawn: ctrl_c → token.cancel() Ctrl-C handler
  ├─ (conditional) LlmSubsystem::new(&config.llm) build LLM subsystem
  ├─ (conditional) AgentsSubsystem::new(config.agents, bus_handle.clone(), memory)
  ├─ (conditional) handlers = vec![Box::new(agents), Box::new(llm)]  register handlers
  ├─ spawn: supervisor::run(bus, control, handlers)  router + control command loop
  ├─ supervisor::adapters::start(control_handle, bus_handle, shutdown)  supervisor-internal stdio/http adapters
  ├─ (conditional) ui_handle = subsystems::ui::start(&config)  build UI serve handle
  ├─ (conditional) comms = subsystems::comms::start(...)  non-blocking; channels spawn immediately
  ├─ (conditional) comms.join().await             block until all channels exit
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
- **Standards & Protocols** — [standards/index.md](standards/index.md) — bus protocol, component runtime, plugin interfaces, capabilities model
