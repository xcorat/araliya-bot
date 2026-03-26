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

# Homebuilder (static init → LLM modification flow)
cargo build -p araliya-agents --features plugin-homebuilder
./target/debug/araliya-bot --config config/agents/homebuilder/agent.toml -i
# Then in REPL: /chat initialize the home page
# Then: /chat change the title text to something more fitting
# Visit http://localhost:8080/home/ (refresh to see changes)
```

## Testing & Linting

```bash
# Workspace-wide
cargo test --workspace               # All tests across all crates (~250 tests)
cargo test -p araliya-core           # Core foundation tests (44 tests)
cargo test -p araliya-supervisor     # Supervisor tests (6 tests)
cargo test -p araliya-llm            # LLM provider tests (10 tests — includes dummy dispatch)
cargo test -p araliya-comms          # Comms state tests (4 tests)
cargo test -p araliya-memory         # Memory subsystem tests (64 base, 91 with features)
cargo test -p araliya-cron           # Cron service tests (4 tests)
cargo test -p araliya-runtimes       # Runtimes subsystem tests (5 tests)
cargo test -p araliya-agents         # Agents subsystem tests
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
cargo clippy -p araliya-tools -- -D warnings
cargo clippy -p araliya-cron -- -D warnings
cargo clippy -p araliya-runtimes -- -D warnings
cargo clippy -p araliya-ui -- -D warnings
cargo clippy -p araliya-agents -- -D warnings
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

**Multi-crate workspace** — shared types and contracts live in `araliya-core`, the runtime orchestrator in `araliya-supervisor`, LLM providers in `araliya-llm`, I/O channels in `araliya-comms`, session management in `araliya-memory`, agents in `araliya-agents`, and the binary wiring in `araliya-bot`. All subsystems are Tokio tasks within one process communicating through a typed channel bus (star topology). The supervisor is a pure router; it never awaits results.

**Crate dependency DAG:**
```
araliya-core          ← foundation: config, error, identity, bus protocol, traits, UI serve trait
araliya-supervisor    ← dispatch loop, control plane, management, adapters (depends on core)
araliya-llm           ← LLM provider abstraction: OpenAI-compatible, Qwen, dummy (depends on core)
araliya-comms         ← I/O channels: PTY, HTTP, Axum, Telegram (depends on core)
araliya-memory        ← session management, stores (doc, KG, SQL); bus handler (depends on core)
araliya-tools         ← external tool integrations: Gmail, GDELT BigQuery, RSS (depends on core)
araliya-cron          ← timer-based event scheduling; BusHandler for cron/* (depends on core)
araliya-runtimes      ← script execution in external runtimes (node, python3); BusHandler for runtimes/* (depends on core)
araliya-ui            ← UI backends: SvUI static serving (ui-svui), GPUI desktop client (ui-gpui), beacon widget (ui-beacon) (depends on core)
araliya-agents        ← Agent trait, AgentsSubsystem, all built-in agent plugins (depends on core, memory, llm)
araliya-bot           ← binary wiring: main.rs + LLM subsystem only (depends on all above)
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
- `llm/` — LLM bus handler routing `llm/*` requests to providers in `araliya-llm`

All other subsystems (agents, memory, tools, cron, runtimes, ui, comms) live in their own crates and are wired directly from `main.rs`.

**Key traits** (defined in `araliya-core`):
- `Component` — pluggable subsystem lifecycle (`araliya_core::runtime`)
- `BusHandler` — standardized request handling (`araliya_core::bus`)
- `Agent` — pluggable agent interface (`araliya-agents/src/lib.rs`)

**Bot identity** — persistent ed25519 keypair at `~/.araliya/bot-pkey{bot_id}/`; `bot_id` = first 8 hex chars of SHA256(verifying_key). Stable across restarts.

## Modularization Plan (complete)

**Phase 5 (complete): Memory subsystem extraction** — `araliya-memory` crate.
- `MemorySystem` lifecycle: `new()`, `create_session()`, `load_session()`, `list_sessions()`
- `SessionStore` trait with implementations: `BasicSessionStore`, `TmpStore`, `AgentStore`, `SqliteStore`, `IDocStore`, `IKGDocStore`
- `MemoryBusHandler` for `memory/kg_graph` and `memory/status` (management plane, read-only)
- Feature-gated document stores: `idocstore` (BM25 FTS), `ikgdocstore` (KG extraction + BFS)
- Background `DocstoreManager` for auto-indexing and orphan cleanup

**Phase 6 (complete): Agent definitions** — agent identity, manifests, and prompt co-location.
- `AgentDefinition` type in `araliya-core/src/config/agent_def.rs` with TOML parsing and directory scanning
- `config/agents/` directory with 15 agent definitions + `_shared/` prompt layers
- Unix-like directory layering: system agents (`config/agents/`) vs user agents (`~/.araliya/agents/`)
- `PromptBuilder.agent_layer()` method resolves prompts with user override support

**Phase 7 (complete): Tools subsystem extraction** — `araliya-tools` crate.
- `ToolsSubsystem` struct + `BusHandler` impl in `araliya-tools/src/dispatcher.rs`
- Tool implementations: `gmail.rs`, `newsmail_aggregator.rs`, `gdelt_bigquery.rs`, `rss_fetch.rs`
- Features: `plugin-gmail-tool`, `plugin-gdelt-tool`, `plugin-rss-fetch-tool`

**Phase 8 (complete): Cron subsystem extraction** — `araliya-cron` crate.
- `CronSubsystem` (BusHandler) + `CronService` (background timer loop)
- Zero-polling BTreeMap priority queue; `cron/schedule`, `cron/cancel`, `cron/list` bus methods
- 4 timer service tests

**Phase 9 (complete): Remove all shim re-exports** — all import sites updated directly.

**Phase 10 (complete): Agents extraction** — `araliya-agents` crate.
- `AgentsSubsystem` (BusHandler for `agents/*`) + `Agent` trait + all 15 built-in agent plugins
- `AgentsState` — capability surface passed to plugins (bus handle, memory, identity, config)
- `ChatCore`, `AgenticLoop` — shared chat/agentic composition layer
- Features: `plugin-echo`, `plugin-basic-chat`, `plugin-chat`, `plugin-agentic-chat`, `plugin-docs`, `plugin-docs-agent`, `plugin-gmail-agent`, `plugin-news-agent`, `plugin-gdelt-news-agent`, `plugin-newsroom-agent`, `plugin-news-aggregator`, `plugin-test-rssnews`, `plugin-runtime-cmd`, `plugin-uniweb`, `plugin-webbuilder`
- `araliya-bot/main.rs` wires `AgentsSubsystem` directly from `araliya_agents::AgentsSubsystem`

**Phase 11 (complete): Cargo.toml cleanup.**
- Removed orphaned deps from `araliya-bot`: `axum`, `teloxide`, `ed25519-dalek`, `rand_core`, `hex`, `thiserror`, `text-splitter`, `chrono` (non-gpui), `htmd`, `toml`, `futures-util`
- `channel-axum`/`channel-telegram` features now forward only to `araliya-comms` (no `dep:` redeclarations)
- `idocstore`/`ikgdocstore` features no longer redeclare `dep:text-splitter` (handled in `araliya-memory`)
- Removed stale TODO comments in `main.rs`

**Phase 12 (complete): Runtimes + UI extraction** — `araliya-runtimes` and `araliya-ui` crates.
- `RuntimesSubsystem` (BusHandler for `runtimes/*`) moved to `araliya-runtimes/src/dispatcher.rs`
- `SvuiBackend` + `start()` moved to `araliya-ui/src/` — no longer a BusHandler, provides `UiServeHandle`
- `araliya-bot/src/subsystems/` now contains only `llm/` — all other subsystems in their own crates
- CI `build-tiers` matrix extended: `runtimes`, `ui`, `agents`, `memory-extended` per-crate jobs added
- All crate versions unified at `0.2.0-alpha`

**Phase 13 (complete): GPUI + Beacon extraction** — both optional desktop binaries fully migrated into `araliya-ui`.
- `araliya-ui` grows two new feature-gated modules: `gpui` (`ui-gpui`) and `beacon` (`ui-beacon`)
- `gpui/mod.rs` exposes `pub fn run()` — app bootstrap, `GpuiAssets`, window wiring; sibling modules: `api`, `state`, `components`, `canvas_scene`, `icons/`
- `beacon/mod.rs` exposes `pub fn run()` — winit event loop, wgpu/vello renderer, `BeaconApp`; sibling modules: `scene`, `ipc`
- `araliya-ui` edition bumped to `2024` (let-chain syntax used in both backends)
- `araliya-bot` binary shims thinned to single-line `araliya_ui::{gpui,beacon}::run()` calls
- `gpui`, `gpui-component`, `winit`, `vello`, `wgpu`, `pollster` deps removed from `araliya-bot/Cargo.toml`; `ui-gpui` and `ui-beacon` features now forward to `araliya-ui`
- `araliya-ui/README.md` added; `README.md` and `docs/development/gpui.md` updated to reflect new source locations and system prerequisites

## Configuration

TOML-based with inheritance: `config/default.toml` is the base. Overlays declare `[meta] base = "other.toml"` for composition. Named launch profiles live in `config/profiles/` and use `[meta] base = "../default.toml"`.

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
├── araliya-cron/src/        # Cron subsystem (timer scheduling)
│   ├── lib.rs               # pub use dispatcher::CronSubsystem
│   ├── dispatcher.rs        # CronSubsystem + BusHandler impl
│   └── service.rs           # CronService background timer loop (4 tests)
├── araliya-tools/src/       # Tools subsystem (external integrations)
│   ├── lib.rs               # Public re-exports
│   ├── dispatcher.rs        # ToolsSubsystem + BusHandler impl
│   ├── gmail.rs             # Gmail OAuth2 API (feature: plugin-gmail-tool)
│   ├── newsmail_aggregator.rs # Gmail filtering/aggregation (feature: plugin-gmail-tool)
│   ├── gdelt_bigquery.rs    # GDELT v2 BigQuery API (feature: plugin-gdelt-tool)
│   └── rss_fetch.rs         # RSS/Atom feed parser (feature: plugin-rss-fetch-tool)
├── araliya-runtimes/src/    # Runtimes subsystem (script execution in external runtimes)
│   ├── lib.rs               # pub use dispatcher::RuntimesSubsystem + type re-exports
│   ├── dispatcher.rs        # RuntimesSubsystem + BusHandler impl (runtimes/init, exec, status)
│   └── types.rs             # RuntimeExecRequest, RuntimeInitRequest, result types
├── araliya-ui/src/          # UI backends: svui serving, gpui desktop client, beacon widget
│   ├── lib.rs               # start() → Option<UiServeHandle>; re-exports UiServe, UiServeHandle
│   ├── svui.rs              # SvuiBackend: serves static files or built-in placeholder (ui-svui)
│   ├── gpui/                # GPUI desktop client (ui-gpui): mod.rs, api, state, components, canvas_scene, icons/
│   └── beacon/              # Beacon widget (ui-beacon): mod.rs, scene, ipc
├── araliya-agents/src/      # Agents subsystem (Agent trait, routing, all plugins)
│   ├── lib.rs               # AgentsSubsystem, AgentsState, Agent trait, AgentRegistration
│   ├── core/                # AgentRuntimeClass, PromptBuilder, AgenticLoop, LocalTool
│   ├── chat/                # ChatCore, BasicChat, SessionChat
│   ├── agentic_chat.rs      # Agentic chat plugin (feature: plugin-agentic-chat)
│   ├── docs.rs              # Docs RAG plugin (feature: plugin-docs)
│   ├── docs_agent.rs        # Docs agent plugin (feature: plugin-docs-agent)
│   ├── docs_import.rs       # Document import helpers
│   ├── gmail.rs             # Gmail agent plugin (feature: plugin-gmail-agent)
│   ├── news.rs              # News agent plugin (feature: plugin-news-agent)
│   ├── news_aggregator.rs   # News aggregator plugin (feature: plugin-news-aggregator)
│   ├── newsroom.rs          # Newsroom agent plugin (feature: plugin-newsroom-agent)
│   ├── gdelt_news.rs        # GDELT news plugin (feature: plugin-gdelt-news-agent)
│   ├── runtime_cmd.rs       # Runtime command plugin (feature: plugin-runtime-cmd)
│   ├── sqlite_tool.rs       # SQLite tool (feature: isqlite)
│   ├── test_rssnews.rs      # RSS news test plugin (feature: plugin-test-rssnews)
│   ├── uniweb/              # Uniweb plugin (feature: plugin-uniweb)
│   └── webbuilder/          # Webbuilder plugin (feature: plugin-webbuilder)
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
└── araliya-bot/src/         # Binary entry point — pure wiring
    ├── main.rs              # Entry point, CLI parsing, subsystem wiring
    ├── lib.rs               # Library exports
    ├── bootstrap/           # Re-exports from araliya-core (identity, logger)
    ├── core/                # Re-exports from araliya-core (config, error)
    ├── supervisor/          # Re-exports from araliya-core + araliya-supervisor
    ├── subsystems/
    │   └── llm/             # LLM bus handler (routes llm/* to araliya-llm providers)
    └── bin/                 # Additional binaries (araliya-ctl, gmail_read_one, gpui shim, beacon shim)

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
│   ├── profiles/            # Named launch configurations (feature/deployment-specific overlays)
│   │   ├── full.toml        # All features enabled (Telegram, Gmail, news, docs)
│   │   ├── docker.toml      # Container deployment (0.0.0.0 binds, no PTY)
│   │   ├── llm-test.toml    # Local Qwen inference testing
│   │   ├── docs_agent.toml  # Agentic chat + KG docstore
│   │   ├── news.toml        # Gmail news agent focus
│   │   ├── newsroom.toml    # Persistent newsroom + GDELT aggregation
│   │   ├── runtime_cmd.toml # Interactive REPL passthrough (no LLM)
│   │   ├── test-gdelt.toml  # GDELT BigQuery news testing
│   │   └── uniweb.toml      # Shared-session front-porch agentic chat
│   ├── default.toml         # Base config (binary default)
│   ├── minimal.toml         # Minimal feature set (CI + local testing)
│   └── dummy.toml           # Dummy LLM (no API key — bus round-trip testing)
docs/                        # Architecture, operations, development docs
```

## Testing Patterns

- Filesystem tests use `tempfile::TempDir` — never write to `~/.araliya`
- Config tests pass overrides directly into `load_from()` rather than mutating env vars
- Tests go in `#[cfg(test)]` blocks at the bottom of the module file
- One assertion per test where practical

## Release

GitHub Actions (`.github/workflows/release-layered-binary.yml`) triggers on `v*` tags, building three tier binaries for `x86_64-unknown-linux-gnu` and publishing to GitHub Releases as `.tar.gz` archives.
