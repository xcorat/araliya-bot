# Architecture Overview

**Status:** v0.5.0 — generic subsystem runtime · `BusHandler` trait · concurrent channel tasks · `Component` trait · `Agent` trait · `OpenAiCompatibleProvider` · capability-scoped state · **Compile-time modularity via Cargo Features** · **Chat-family agent composition (`ChatCore`)** · **Memory subsystem with pluggable stores (`basic_session`, optional `idocstore`)** · **UI subsystem (`svui` backend)** · **Cron subsystem (timer-based event scheduling)** · **Tools subsystem (Gmail MVP)** · **LLM token usage tracking + per-session cost accumulation (`spend.json`)**.

---

## Design Principles

- **Single-process supervisor model** — all subsystems run as Tokio tasks within one process; upgradeable to OS-level processes later without changing message types.
- **Star topology** — supervisor is the hub; subsystems communicate only via the supervisor. Per-hop overhead is negligible (~100–500 ns). Provides centralized logging, cancellation, and permission gating without actor mailbox complexity.
- **Capability-passing** — subsystems receive only the handles they need at init; no global service locator.
- **Non-blocking supervisor loop** — the supervisor is a pure router; it forwards `reply_tx` ownership to handlers and returns immediately.
- **Split planes** — subsystem traffic uses the supervisor bus; supervisor management uses an internal control plane.
- **Compile-time Modularity** — Subsystems and agents can be disabled via Cargo features to optimize binary size and memory footprint.

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
  ├─ "tools/*"   → tools.handle_request(method, payload, reply_tx)
  └─ unknown     → reply_tx.send(Err(ERR_METHOD_NOT_FOUND))
```

Handlers resolve `reply_tx` directly for synchronous work or from a `tokio::spawn`ed task for async work.

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
- [subsystems/memory.md](subsystems/memory.md) — sessions, transcripts, working memory (planned), 
- [subsystems/intelligent_doc_store.md](subsystems/intelligent_doc_store.md) — feature-gated document indexing, chunking, BM25 search
- [subsystems/cron.md](subsystems/cron.md) — timer-based event scheduling, schedule/cancel/list API
- [subsystems/tools.md](subsystems/tools.md) — tool execution, Gmail MVP
- [standards/index.md](standards/index.md) — bus protocol, component runtime, plugin interfaces, capabilities model
