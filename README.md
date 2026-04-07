# Araliya Bot

Modular agentic assistant. Rust backend with a pluggable subsystem architecture and a SvelteKit web UI.

**Status:** v0.2.0-alpha · Linux x86_64 / aarch64

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
```

The installer asks what you want to use it for, downloads the right pre-built binary from [GitHub Releases](https://github.com/xcorat/araliya-bot/releases), and walks you through LLM and channel setup.

**→ [Full installation guide](docs/installation.md)**

---

## Build from source

Requires [Rust 1.80+](https://rustup.rs).

```bash
git clone https://github.com/xcorat/araliya-bot
cd araliya-bot
cargo build --release
./target/release/araliya-bot setup   # first-time config wizard
./target/release/araliya-bot -i      # interactive terminal
```

Build tiers:

| Flag | Purpose |
|---|---|
| `minimal` | PTY + basic-chat + LLM. No API key required. |
| `default` | HTTP/web UI, all core agents, cron, tools |
| `full` | Everything — Telegram, Gmail, GDELT, KG docstore |

```bash
cargo build --release                                                 # default
cargo build --release --no-default-features --features minimal
cargo build --release --features full
```

No-API round-trip test (dummy LLM):

```bash
cargo build --no-default-features --features minimal
./target/debug/araliya-bot -i --config config/dummy.toml
```

Log verbosity: `-v` warn · `-vv` info · `-vvv` debug · `-vvvv` trace

---

## Architecture

Multi-crate Cargo workspace. All subsystems run as Tokio tasks in one process, communicating through a typed channel bus (star topology).

```
araliya-core          foundation: config, error, identity, bus protocol, traits
araliya-supervisor    runtime orchestrator: dispatch loop, control plane, management
araliya-llm           LLM provider abstraction: OpenAI-compatible, Qwen, dummy
araliya-comms         I/O channels: PTY, Axum HTTP, Telegram (feature-gated)
araliya-memory        session management, pluggable stores (SQLite, FTS, KG)
araliya-tools         external tools: Gmail OAuth2, GDELT BigQuery, RSS
araliya-cron          timer-based event scheduling
araliya-runtimes      script execution in external runtimes (node, python3)
araliya-ui            UI backends: SvUI static serving + GPUI desktop client
araliya-agents        Agent trait, AgentsSubsystem, all 15 built-in agent plugins
araliya-bot           binary wiring: main.rs + LLM subsystem
```

Bus routing by method prefix:

```
agents/*  → araliya-agents       (AgentsSubsystem)
llm/*     → araliya-bot          (LlmSubsystem)
tools/*   → araliya-tools        (ToolsSubsystem)
cron/*    → araliya-cron         (CronSubsystem)
memory/*  → araliya-memory       (MemoryBusHandler)
manage/*  → araliya-supervisor   (ManagementSubsystem)
```

---

## Project layout

```
araliya-bot/
├── install.sh                    curl | sh installer
├── Cargo.toml                    workspace root
├── config/
│   ├── agents/                   agent TOML manifests + prompts (15 agents)
│   ├── default.toml              full reference config
│   ├── minimal.toml              minimal overlay
│   └── dummy.toml                no-API-key test overlay
├── crates/
│   ├── araliya-core/
│   ├── araliya-supervisor/
│   ├── araliya-llm/
│   ├── araliya-comms/
│   ├── araliya-memory/
│   ├── araliya-tools/
│   ├── araliya-cron/
│   ├── araliya-runtimes/
│   ├── araliya-ui/
│   ├── araliya-agents/
│   └── araliya-bot/
├── frontend/
│   └── svui/                     SvelteKit 5 web UI
└── docs/                         architecture, operations, development docs
```

---

## Desktop client (GPUI)

An optional native desktop client (`araliya-gpui`) connects to the bot daemon over HTTP, built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui).

Linux prerequisites:

```bash
# Debian / Ubuntu
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

```bash
cargo build --bin araliya-gpui --features ui-gpui
cargo run --bin araliya-gpui --features ui-gpui   # daemon must be running
```

See [`docs/development/gpui.md`](docs/development/gpui.md) for details.

---

## Documentation

- [Installation](docs/installation.md) — install script, use-case guide, env overrides
- [Getting Started](docs/getting-started.md) — build, run, verify
- [Configuration](docs/configuration.md) — config files and env vars
- [Architecture Overview](docs/architecture/overview.md) — system design
- [Operations](docs/operations/deployment.md) — running in production
- [Contributing](docs/development/contributing.md) — development and testing

---

## Binary releases

Tagging `v*` triggers CI to build and publish three-tier Linux bundles:

```
araliya-bot-{version}-minimal-x86_64-unknown-linux-gnu.tar.gz
araliya-bot-{version}-default-x86_64-unknown-linux-gnu.tar.gz
araliya-bot-{version}-full-x86_64-unknown-linux-gnu.tar.gz
SHA256SUMS
```

Each bundle includes `bin/araliya-bot`, `config/`, and `frontend/svui/`.

---

## License

GPL-3.0-only — see [LICENSE](LICENSE).
