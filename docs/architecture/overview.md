# Architecture Overview

**Version:** v0.2.0-alpha — full multi-crate workspace · 11 crates · `araliya-runtimes` and `araliya-ui` extracted (Phase 12) · `araliya-bot` is pure binary wiring (only `llm/` subsystem remains) · no shim re-exports · orphaned deps purged.

---

## Design Principles

- **Single-process supervisor model** — all subsystems run as Tokio tasks within one process; upgradeable to OS-level processes later without changing message types.
- **Star topology** — supervisor is the hub; subsystems communicate only via the supervisor. Per-hop overhead is negligible (~100–500 ns). Provides centralized logging, cancellation, and permission gating without actor mailbox complexity.
- **Capability-passing** — subsystems receive only the handles they need at init; no global service locator.
- **Non-blocking supervisor loop** — the supervisor is a pure router; it forwards `reply_tx` ownership to handlers and returns immediately.
- **Split planes** — subsystem traffic uses the supervisor bus; supervisor management uses an internal control plane.
- **Compile-time Modularity** — Subsystems and agents can be disabled via Cargo features to optimize binary size and memory footprint.

---

## Crate Workspace

The codebase is a multi-crate Cargo workspace. Each crate is an independently compilable library; `araliya-bot` is the thin binary that wires them.

```
araliya-core          Tier 0 — config, error, identity, bus protocol, Component/BusHandler traits
araliya-supervisor    Tier 1 — dispatch loop, control plane, management, stdio/UDS adapters
araliya-llm           Tier 1 — LLM provider abstraction (OpenAI-compatible, Qwen, dummy)
araliya-comms         Tier 1 — I/O channels: PTY, Axum, HTTP, Telegram (all feature-gated)
araliya-memory        Tier 1 — session lifecycle, pluggable stores, bus handler
araliya-tools         Tier 1 — external tools: Gmail, GDELT BigQuery, RSS
araliya-cron          Tier 1 — timer-based scheduling, BusHandler for cron/*
araliya-runtimes      Tier 1 — external runtime execution (node, python3, bash)
araliya-ui            Tier 1 — UI backends (svui static file serving + SPA routing)
araliya-agents        Tier 2 — Agent trait, AgentsSubsystem, all 15 built-in agent plugins
araliya-bot           Tier 3 — binary: main.rs + LLM bus handler (pure wiring)
```

Each crate depends only on `araliya-core` plus the Tier 1 crates it needs. `araliya-agents` depends on `araliya-core`, `araliya-memory`, and `araliya-llm` (for `ModelRates`). No circular dependencies.

Feature flags are per-crate. `araliya-bot` forwards plugin/channel flags to the appropriate crates via `araliya-agents/plugin-*`, `araliya-comms/channel-*`, etc.

---

## Process Structure

```
┌──────────────────────────────────────────────────────┐
│               SUPERVISOR (main process)              │
│          config · identity · logger · error          │
├──────────────────────────────────────────────────────┤
│                                                      │
│  ┌─────────────┐   ┌─────────────┐  ┌────────────┐  │
│  │   Comms     │   │   Memory    │  │    Cron    │  │
│  │  Subsystem  │   │   System    │  │  Subsystem │  │
│  └──────┬──────┘   └──────┬──────┘  └─────┬──────┘  │
│         │                 │               │         │
│  ┌──────┴─────────────────┴───────────────┴──┐      │
│  │       Typed Channel Router (star hub)     │      │
│  └──────┬─────────────────┬──────────────────┘      │
│         │                 │                         │
│  ┌──────┴──────┐  ┌───────┴──────┐  ┌────────────┐  │
│  │   Agents    │  │     LLM      │  │   Tools    │  │
│  │  Subsystem  │  │  Subsystem   │  │  Subsystem │  │
│  └─────────────┘  └──────────────┘  └────────────┘  │
│                                                      │
└──────────────────────────────────────────────────────┘
```

---

## Supervisor Routing Model

The supervisor dispatches by method prefix and immediately forwards ownership of `reply_tx: oneshot::Sender<BusResult>` to the target subsystem. It does not await the result.

```
Request { method, payload, reply_tx }
  ├─ "agents/*"  → agents.handle_request(method, payload, reply_tx)
  ├─ "llm/*"     → llm.handle_request(method, payload, reply_tx)
  ├─ "cron/*"    → cron.handle_request(method, payload, reply_tx)
  ├─ "manage/*"  → management.handle_request(method, payload, reply_tx)
  ├─ "memory/*"  → memory_bus.handle_request(method, payload, reply_tx)
  ├─ "tools/*"   → tools.handle_request(method, payload, reply_tx)
  └─ unknown     → reply_tx.send(Err(ERR_METHOD_NOT_FOUND))
```

Handlers resolve `reply_tx` directly for synchronous work or from a `tokio::spawn`ed task for async work.

---

## Agent Runtime Model

The agents subsystem organizes every registered agent under an explicit **runtime class** — a first-class architectural attribute that describes the agent's execution model independently of what the agent does.

The current runtime classes are:

| Class | Execution model |
|---|---|
| `RequestResponse` | Stateless single-turn exchange. One message in, one reply out. No session state required. |
| `Session` | Persistent multi-turn conversation. Session memory is maintained across requests. |
| `Agentic` | Bounded multi-step orchestration. Instruction pass → tool execution → response pass per request. |
| `Specialized` | Transitional class for built-in agents whose model does not cleanly fit the above. |
| `Workflow` | Planned — explicit step-graph orchestration with optional checkpointing. Not yet implemented. |
| `Background` | Planned — event-driven long-running process with supervised lifecycle. Not yet implemented. |

Each registered agent is stored as an `AgentRegistration` pairing its implementation with its runtime class. The routing layer resolves agent IDs without regard to runtime class; the runtime class governs how the agent's execution is structured internally.

Built-in agent classifications:

| Agent | Runtime class |
|---|---|
| `echo` | `RequestResponse` |
| `basic_chat` | `RequestResponse` |
| `chat` | `Session` |
| `agentic-chat` | `Agentic` |
| `docs` | `Agentic` |
| `news` | `Specialized` |
| `gmail` | `Specialized` |
| `runtime_cmd` | `Specialized` |
| `webbuilder` | `Agentic` |

See [Agents Subsystem](subsystems/agents.md) for the full architecture, orchestration model, and configuration reference.

---

## Identity

Each bot instance has a persistent ed25519 keypair. The `bot_id` is the first 8 hex characters of `SHA256(verifying_key)`.

- `bot_id` is stable across restarts
- Used as the identity directory name: `bot-pkey{bot_id}/`
- Future: signs events, authenticates to external services

See [identity.md](identity.md) for full details.

---

## Further Reading

- [diagrams.md](diagrams.md) — **visual architecture diagrams** (system overview, bus protocol, startup sequence, chat workflow, comms channels, memory system, identity hierarchy, component runtime)
- [identity.md](identity.md) — keypair lifecycle, file format, security
- [subsystems/agents.md](subsystems/agents.md) — runtime classes, agent families, orchestration model, routing, session queries, configuration
- [subsystems/comms.md](subsystems/comms.md) — PTY, HTTP, Telegram, and axum channel plugins
- [subsystems/llm.md](subsystems/llm.md) — LLM provider abstraction, instruction vs response routing, dummy provider
- [subsystems/memory.md](subsystems/memory.md) — sessions, transcripts, KV store, agent stores, spend accounting
- [subsystems/intelligent_doc_store.md](subsystems/intelligent_doc_store.md) — document indexing, chunking, BM25 search
- [subsystems/kg_docstore.md](subsystems/kg_docstore.md) — knowledge graph construction, BFS traversal, KG+FTS retrieval
- [subsystems/cron.md](subsystems/cron.md) — timer-based event scheduling
- [subsystems/tools.md](subsystems/tools.md) — tool execution, Gmail MVP
- [subsystems/ui.md](subsystems/ui.md) — SvelteKit web UI backend
- [standards/index.md](standards/index.md) — bus protocol, component runtime, plugin interfaces, capabilities model
