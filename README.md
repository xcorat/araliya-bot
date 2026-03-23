# Araliya Bot

Modular agentic assistant. Rust, single-process supervisor with pluggable subsystems and a SvelteKit web UI.

**Status:** v0.2.0-alpha — full multi-crate workspace

---

## Quick Start

**Requirements:** Rust 1.80+, Linux/macOS

```bash
git clone <repo>
cd araliya-bot
cargo build
./target/debug/araliya-bot -i   # interactive mode
```

On first run, a persistent bot identity is generated at `~/.araliya/bot-pkey{id}/`.

For a minimal no-API-key build (dummy LLM, PTY only):

```bash
cargo build -p araliya-bot --no-default-features --features minimal
./target/debug/araliya-bot -i --config config/dummy.toml
```

Log verbosity:

```bash
./target/debug/araliya-bot -vvv   # debug
./target/debug/araliya-bot -vvvv  # trace
```

---

## Build Tiers

| Flag | Purpose |
|---|---|
| `minimal` | PTY channel + basic-chat agent + dummy LLM. No API key required. |
| `default` | Full recommended feature set — Axum HTTP, web UI, all core agents, cron, tools |
| `full` | All features — Telegram, Gmail, GDELT news, KG docstore |

```bash
cargo build -p araliya-bot                                       # default
cargo build -p araliya-bot --no-default-features --features minimal
cargo build -p araliya-bot --all-features
```

---

## Architecture

Multi-crate Cargo workspace. All subsystems run as Tokio tasks within one process, communicating through a typed channel bus (star topology).

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
agents/*  → araliya-agents  (AgentsSubsystem)
llm/*     → araliya-bot     (LlmSubsystem)
tools/*   → araliya-tools   (ToolsSubsystem)
cron/*    → araliya-cron    (CronSubsystem)
memory/*  → araliya-memory  (MemoryBusHandler — read-only KG queries)
manage/*  → araliya-supervisor (ManagementSubsystem)
```

---

## Project Structure

```
araliya-bot/
├── Cargo.toml                    workspace root
├── config/
│   ├── agents/                   agent definitions (TOML manifests + prompts)
│   │   ├── _shared/              shared prompt layers
│   │   ├── echo/, basic-chat/, chat/, agentic-chat/, docs/, ...
│   │   └── (15 agents total)
│   ├── default.toml              default config
│   ├── minimal.toml              minimal overlay
│   └── dummy.toml                no-API-key test overlay
├── crates/
│   ├── araliya-core/             shared foundation
│   ├── araliya-supervisor/       runtime orchestrator
│   ├── araliya-llm/              LLM providers
│   ├── araliya-comms/            I/O channels
│   ├── araliya-memory/           session management + stores
│   ├── araliya-tools/            external tool integrations
│   ├── araliya-cron/             timer scheduling
│   ├── araliya-runtimes/         script execution (node, python3)
│   ├── araliya-ui/               UI backends: svui + gpui desktop client
│   ├── araliya-agents/           agent subsystem + all plugins
│   └── araliya-bot/              binary entry point
├── frontend/
│   └── svui/                     SvelteKit web UI (Tailwind CSS 4, Bits UI)
└── docs/                         architecture, operations, development docs
```

---

## Desktop Client (GPUI)

An optional native desktop client (`araliya-gpui`) connects to the bot daemon over HTTP. It is built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), Zed's GPU-accelerated UI framework, and lives in the `araliya-ui` crate under the `ui-gpui` feature.

### System prerequisites (Linux)

GPUI links against native X11/XCB libraries that are not bundled by Cargo. Install them before building:

```bash
# Debian / Ubuntu / Mint
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev

# Fedora / RHEL
sudo dnf install -y libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel

# Arch Linux
sudo pacman -S libxcb libxkbcommon libxkbcommon-x11
```

These are `-dev` packages (symlinks + headers for the linker). The runtime `.so` files are typically present on any desktop Linux system already.

### Build and run

```bash
# Build
cargo build --bin araliya-gpui --features ui-gpui

# Run (start the bot daemon first)
cargo run                                          # Terminal 1 — daemon on :8080
cargo run --bin araliya-gpui --features ui-gpui   # Terminal 2 — desktop client
```

See [`docs/development/gpui.md`](docs/development/gpui.md) for full details.

---

## Documentation

- [Getting Started](docs/getting-started.md) — build, run, verify
- [Quick Intro](docs/quick-intro.md) — feature tour
- [Configuration](docs/configuration.md) — config files and env vars
- [Architecture Overview](docs/architecture/overview.md) — system design
- [Operations](docs/operations/deployment.md) — running in production
- [Development](docs/development/contributing.md) — contributing and testing

---

## Binary Releases

Tagging `v*` triggers GitHub Actions to build three-tier Linux x86_64 release bundles.

```bash
git tag v0.2.0-alpha
git push origin v0.2.0-alpha
```

Release artifacts:

- `araliya-bot-v0.2.0-alpha-minimal-x86_64-unknown-linux-gnu.tar.gz`
- `araliya-bot-v0.2.0-alpha-default-x86_64-unknown-linux-gnu.tar.gz`
- `araliya-bot-v0.2.0-alpha-full-x86_64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS`

Each bundle includes `bin/araliya-bot`, `config/`, and `frontend/svui/`.

```bash
sha256sum -c SHA256SUMS
tar -xzf araliya-bot-v0.2.0-alpha-default-x86_64-unknown-linux-gnu.tar.gz
cd araliya-bot-v0.2.0-alpha-default-x86_64-unknown-linux-gnu
./bin/araliya-bot -f config/cfg.toml
```
