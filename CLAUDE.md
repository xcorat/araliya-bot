# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

All commands run from `araliya-bot/` (the Cargo workspace root).

```bash
# Backend
cargo build                          # Default features
cargo build --release --locked       # Release binary
cargo build --no-default-features --features minimal
cargo build --all-features           # Full feature set

# Run
./target/release/araliya-bot         # Daemon mode
./target/release/araliya-bot -i      # Interactive mode

# Minimal + dummy LLM (no API key needed — for local bus round-trip testing)
cargo build -p araliya-bot --no-default-features --features minimal
./target/debug/araliya-bot -i --config config/dummy.toml

# Management CLI (requires --features cli)
./target/debug/araliya-ctl status
./target/debug/araliya-ctl health
./target/debug/araliya-ctl shutdown

# Frontend (from frontend/svui/)
pnpm install
pnpm build     # Output to frontend/build/
pnpm dev       # Dev server at http://localhost:5173

# Docker
docker-compose up --build
```

## Testing & Linting

```bash
# Workspace-wide
cargo test --workspace               # All tests across all crates (~318 tests)
cargo test -p araliya-core           # Core foundation tests (44 tests)
cargo test -p araliya-supervisor     # Supervisor tests (6 tests)
cargo test -p araliya-llm            # LLM provider tests (10 tests — includes dummy dispatch)
cargo test -p araliya-comms          # Comms state tests (4 tests)
cargo test -p araliya-memory         # Memory subsystem tests (64 base, 91 with features)
cargo test -p araliya-bot            # Bot subsystem tests

# Feature-gated tests
cargo test -p araliya-memory --features "isqlite,idocstore,ikgdocstore"
cargo test --features idocstore      # Include doc store tests
cargo test --features ikgdocstore    # Include knowledge graph tests

# Build all tiers
cargo build -p araliya-bot                                      # Default features
cargo build -p araliya-bot --no-default-features --features minimal
cargo build -p araliya-bot --all-features

# Linting/formatting
cargo check --workspace
cargo clippy -p araliya-core -- -D warnings
cargo clippy -p araliya-supervisor -- -D warnings
cargo clippy -p araliya-llm -- -D warnings
cargo clippy -p araliya-comms --all-features -- -D warnings
cargo fmt --check

# Frontend type checking
cd frontend/svui && pnpm check
```

To run a single test:
```bash
cargo test test_name
cargo test -p araliya-core test_name
cargo test module::path::test_name
```

## Logging Verbosity

CLI flags override config:
- `-vv` → info (default)
- `-vvv` → debug
- `-vvvv` → trace
- `--log-file /tmp/araliya.log` → write to file

## Feature Tiers

| Flag | Purpose |
|------|---------|
| `minimal` | Basic subsystems only (agents, LLM, PTY, basic-chat) |
| `default` | Full recommended feature set |
| `full` | All features (Gmail, Telegram, news, docs indexing) |

Feature-gated code uses `#[cfg(feature = "feature-name")]` throughout.

## Architecture

**Multi-crate workspace** — shared types and contracts live in `araliya-core`, the runtime orchestrator in `araliya-supervisor`, LLM providers in `araliya-llm`, I/O channels in `araliya-comms`, session management in `araliya-memory`, and remaining subsystem implementations + binary wiring in `araliya-bot`. All subsystems are Tokio tasks within one process communicating through a typed channel bus (star topology). The supervisor is a pure router; it never awaits results.

**Crate dependency DAG:**
```
araliya-core          ← foundation: config, error, identity, bus protocol, traits, UI serve trait
araliya-supervisor    ← dispatch loop, control plane, management, adapters (depends on core)
araliya-llm           ← LLM provider abstraction: OpenAI-compatible, Qwen, dummy (depends on core)
araliya-comms         ← I/O channels: PTY, HTTP, Axum, Telegram (depends on core)
araliya-memory        ← session management, stores (doc, KG, SQL); bus handler (depends on core)
araliya-bot           ← binary + remaining subsystems (depends on all above)
```

**Bus routing** (method prefix → subsystem):
```
"agents/*"  → agents subsystem
"llm/*"     → LLM subsystem
"cron/*"    → cron subsystem
"manage/*"  → management subsystem
"memory/*"  → memory bus handler (read-only KG queries)
"tools/*"   → tools subsystem
```

Each request carries a `reply_tx: oneshot::Sender<BusResult>` that is forwarded immediately to the handler, which resolves it synchronously or from a spawned task.

**Subsystems** (`crates/araliya-bot/src/subsystems/`):
- `comms/` — shim re-exporting from `araliya-comms` (PTY, Telegram, HTTP, Axum channels)
- `agents/` — agent routing + registration; loads system agents from `config/agents/` and user agents from `~/.araliya/agents/`; built-in agents: `echo`, `basic-chat`, `chat`, `agentic-chat`, `docs`, `uniweb`, `gmail`, `news`, `gdelt_news`, `newsroom`, `news_aggregator`, `test_rssnews`, `webbuilder`, `runtime_cmd`, `docs_agent`
- `llm/` — shim re-exporting from `araliya-llm` (OpenAI-compatible, Qwen, dummy providers)
- `memory/` — shim re-exporting from `araliya-memory` (session lifecycle, stores, bus handler)
- `cron/` — timer-based event scheduling
- `tools/` — external actions (Gmail MVP)
- `ui/` — SvelteKit web backend (`svui`), GPUI desktop, beacon

**Key traits** (defined in `araliya-core`, re-exported through shims in `araliya-bot`):
- `Component` — pluggable subsystem lifecycle (`araliya_core::runtime`)
- `BusHandler` — standardized request handling (`araliya_core::bus`)
- `Agent` — pluggable agent interface (`crates/araliya-bot/src/subsystems/agents/`)

**Bot identity** — persistent ed25519 keypair at `~/.araliya/bot-pkey{bot_id}/`; `bot_id` = first 8 hex chars of SHA256(verifying_key). Stable across restarts.

## Modularization Plan

**Phase 5 (complete): Memory subsystem extraction** — `araliya-memory` crate.
- `MemorySystem` lifecycle: `new()`, `create_session()`, `load_session()`, `list_sessions()`
- `SessionStore` trait with 5 implementations: `BasicSessionStore`, `TmpStore`, `AgentStore`, `SqliteStore`, `IDocStore`, `IKGDocStore`
- `MemoryBusHandler` for `memory/kg_graph` and `memory/status` (management plane, read-only)
- Feature-gated document stores: `idocstore` (BM25 FTS), `ikgdocstore` (KG extraction + BFS)
- Background `DocstoreManager` for auto-indexing and orphan cleanup
- Shim re-exports in `araliya-bot/subsystems/memory/` preserve all `use crate::subsystems::memory::*` call sites

**Phase 6 (complete): Agent definitions** — agent identity, manifests, and prompt co-location.
- `AgentDefinition` type in `araliya-core/src/config/agent_def.rs` with TOML parsing and directory scanning
- `config/agents/` directory with 15 agent definitions + `_shared/` prompt layers
- Unix-like directory layering: system agents (`config/agents/`) vs user agents (`~/.araliya/agents/`)
- `PromptBuilder.agent_layer()` method resolves prompts with user override support
- Core agents: echo, basic-chat, chat, agentic-chat, docs, uniweb
- Plugin agents: gmail, news, gdelt_news, newsroom, news_aggregator, test_rssnews, webbuilder, runtime_cmd, docs_agent

**Future phases:**
- Phase 7: Extract tools subsystem (`araliya-tools`)
- Phase 8: Extract cron subsystem (`araliya-cron`)
- Phase 9: Extract agents registry + routing (`araliya-agents`)

## Configuration

TOML-based with inheritance: `config/default.toml` is the base. Overlays declare `[meta] base = "other.toml"` for composition.

Key config sections: `[supervisor]`, `[comms.pty]`, `[comms.telegram]`, `[agents]`, `[memory]`, `[llm]`, `[tools]`, `[ui]`.

Environment variable overrides for any setting:
```bash
ARALIYA_WORK_DIR=...   # Override ~/.araliya
ARALIYA_LOG_LEVEL=debug
LLM_API_KEY=...
TELEGRAM_BOT_TOKEN=...
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
```

See `.env.example` for full list. Config format documented in `docs/configuration.md`.

## Code Layout

```
crates/
├── araliya-core/src/        # Shared foundation (config, error, identity, bus protocol, traits)
│   ├── config/              # Config types, TOML loading, env overrides
│   ├── bus/                 # BusMessage, BusPayload, BusHandler, HealthRegistry, ComponentInfo
│   ├── runtime.rs           # Component trait, SubsystemHandle
│   ├── types/llm.rs         # StreamChunk, LlmUsage, LlmTiming, ModelRates
│   ├── identity.rs          # Ed25519 keypair management
│   ├── logger.rs            # Tracing logger init
│   └── error.rs             # AppError enum
├── araliya-supervisor/src/  # Runtime orchestrator
│   ├── run.rs               # Supervisor dispatch loop
│   ├── control.rs           # ControlCommand, ControlHandle, SupervisorControl
│   ├── management.rs        # ManagementSubsystem (manage/* bus handler)
│   └── adapters/            # stdio REPL, Unix domain socket
├── araliya-llm/src/        # LLM provider abstraction
│   ├── lib.rs               # ProviderError, LlmResponse, LlmProvider enum
│   └── providers/           # OpenAI-compatible, Qwen, dummy implementations
├── araliya-comms/src/      # Comms subsystem (I/O channels)
│   ├── lib.rs               # CommsStatusHandler, start()
│   ├── state.rs             # CommsState, CommsReply, CommsEvent
│   ├── pty.rs               # PTY channel (cfg: channel-pty)
│   ├── telegram.rs          # Telegram channel (cfg: channel-telegram)
│   ├── http/                # HTTP channel (cfg: channel-http)
│   └── axum_channel/        # Axum channel (cfg: channel-axum)
├── araliya-memory/src/      # Memory subsystem (session management, stores, bus handler)
│   ├── lib.rs               # MemorySystem, SessionInfo, SessionSpend, MemoryConfig
│   ├── bus.rs               # MemoryBusHandler (management plane, read-only kg_graph queries)
│   ├── handle.rs            # SessionHandle async API
│   ├── rw.rs                # SessionRw blocking I/O dispatch
│   ├── types.rs             # PrimaryValue, Obj, TextFile, Value
│   ├── collections.rs       # Doc, Block, Collection
│   ├── store.rs             # SessionStore trait + Store
│   ├── docstore_manager.rs  # Background maintenance task (feature: idocstore)
│   └── stores/              # Store implementations
│       ├── basic_session.rs # Capped KV + transcript (in-memory)
│       ├── tmp.rs           # Ephemeral typed collections
│       ├── agent.rs         # Persistent agent-scoped sessions
│       ├── sqlite_core.rs   # Shared SQLite helpers
│       ├── sqlite_store.rs  # General-purpose typed SQL (feature: isqlite)
│       ├── docstore.rs      # FTS5 document index (feature: idocstore)
│       └── kg_docstore.rs   # Document store + knowledge graph (feature: ikgdocstore)
└── araliya-bot/src/         # Binary + remaining subsystems
    ├── main.rs              # Entry point, CLI parsing
    ├── lib.rs               # Library exports
    ├── bootstrap/           # Re-exports from araliya-core (identity, logger)
    ├── core/                # Re-exports from araliya-core (config, error)
    ├── supervisor/          # Re-exports from araliya-core + araliya-supervisor
    ├── llm/                 # Shim re-exporting from araliya-llm
    ├── subsystems/
    │   ├── runtime.rs       # Re-exports from araliya-core
    │   ├── agents/          # Agent routing + all agent plugins
    │   ├── comms/           # Shim re-exporting from araliya-comms
    │   ├── llm/             # LLM bus handler
    │   ├── memory/          # Shim re-exporting from araliya-memory
    │   ├── memory_bus/      # Shim re-exporting MemoryBusHandler
    │   ├── cron/            # Scheduler
    │   ├── tools/           # Tool execution
    │   └── ui/              # UI backends (UiServe trait in araliya-core)
    └── bin/                 # Additional binaries (araliya-ctl, gmail_read_one, gpui, beacon)

frontend/svui/               # SvelteKit web UI (pnpm, TypeScript, Tailwind CSS 4, Bits UI)
config/                      # TOML config files + agent definitions
│   ├── agents/              # System agent definitions (manifests + prompts)
│   │   ├── _shared/         # Shared prompt layers (id.md, agent.md, memory_and_tools.md, etc.)
│   │   ├── echo/
│   │   ├── basic-chat/
│   │   ├── chat/
│   │   ├── agentic-chat/
│   │   ├── docs/
│   │   ├── docs_agent/
│   │   ├── uniweb/
│   │   └── …(11 more agents)
│   └── *.toml               # Configuration overlays
docs/                        # Architecture, operations, development docs
```

## Testing Patterns

- Filesystem tests use `tempfile::TempDir` — never write to `~/.araliya`
- Config tests pass overrides directly into `load_from()` rather than mutating env vars
- Tests go in `#[cfg(test)]` blocks at the bottom of the module file
- One assertion per test where practical

## Release

GitHub Actions (`.github/workflows/release-layered-binary.yml`) triggers on `v*` tags, building three tier binaries for `x86_64-unknown-linux-gnu` and publishing to GitHub Releases as `.tar.gz` archives.
