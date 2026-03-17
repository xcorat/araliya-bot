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
cargo test                           # All tests
cargo test --features idocstore      # Include doc store tests
cargo test --features ikgdocstore    # Include knowledge graph tests

# Linting/formatting
cargo check
cargo clippy -- -D warnings
cargo fmt --check

# Frontend type checking
cd frontend/svui && pnpm check
```

To run a single test:
```bash
cargo test test_name
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

**Single-process supervisor** — all subsystems are Tokio tasks within one process communicating through a typed channel bus (star topology). The supervisor is a pure router; it never awaits results.

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
- `comms/` — I/O channels (PTY, Telegram, HTTP, Axum); each channel is a concurrent Tokio task
- `agents/` — message routing with pluggable `Agent` trait; built-in agents: `echo`, `basic-chat`, `chat`, `gmail`, `news`, `docs`
- `llm/` — `OpenAiCompatibleProvider` abstraction; dummy provider for testing
- `memory/` — session + transcript store; optional SQLite-backed doc store (`idocstore`) and knowledge graph (`ikgdocstore`)
- `cron/` — timer-based event scheduling
- `tools/` — external actions (Gmail MVP)
- `ui/` — SvelteKit web backend (`svui`), GPUI desktop, beacon

**Key traits** (`src/subsystems/runtime.rs`):
- `Component` — pluggable subsystem lifecycle
- `BusHandler` — standardized request handling
- `Agent` — pluggable agent interface

**Bot identity** — persistent ed25519 keypair at `~/.araliya/bot-pkey{bot_id}/`; `bot_id` = first 8 hex chars of SHA256(verifying_key). Stable across restarts.

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
crates/araliya-bot/src/
├── main.rs              # Entry point, CLI parsing
├── lib.rs               # Library exports
├── bootstrap/           # Identity & logger init
├── core/                # Config loading, error types
├── llm/                 # LLM provider abstraction
├── supervisor/          # Bus router, dispatch loop
├── subsystems/
│   ├── runtime.rs       # Component/BusHandler/Agent traits
│   ├── agents/          # Agent routing + all agent plugins
│   ├── comms/           # Channel plugins
│   ├── llm/             # LLM handler
│   ├── memory/          # Session & transcript stores
│   ├── cron/            # Scheduler
│   ├── tools/           # Tool execution
│   └── ui/              # UI backends
└── bin/                 # Additional binaries (araliya-ctl, gmail_read_one, gpui, beacon)

frontend/svui/           # SvelteKit web UI (pnpm, TypeScript, Tailwind CSS 4, Bits UI)
config/                  # TOML config files + prompts/
docs/                    # Architecture, operations, development docs
```

## Testing Patterns

- Filesystem tests use `tempfile::TempDir` — never write to `~/.araliya`
- Config tests pass overrides directly into `load_from()` rather than mutating env vars
- Tests go in `#[cfg(test)]` blocks at the bottom of the module file
- One assertion per test where practical

## Release

GitHub Actions (`.github/workflows/release-layered-binary.yml`) triggers on `v*` tags, building three tier binaries for `x86_64-unknown-linux-gnu` and publishing to GitHub Releases as `.tar.gz` archives.
