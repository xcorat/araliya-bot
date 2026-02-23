
---

<- 93 tests source: docs/index.md -->
# Araliya Docs

Welcome to the documentation portal.

Start here:

- [Quick Intro](quick-intro.md) — project overview and core concepts.
- [Getting Started](getting-started.md) — build, run, and first-run verification.
- [Configuration](configuration.md) — config files, runtime flags, and env vars.

UI notes:

- Svelte web UI: `ui/svui` (served by bot HTTP channel).
- GPUI desktop UI: `araliya-gpui` binary (`cargo run --bin araliya-gpui --features ui-gpui`).

Deep dives:

- [Architecture Overview](architecture/overview.md)
- [Identity](architecture/identity.md)
- [Agents](architecture/subsystems/agents.md)
- [Tools Subsystem](architecture/subsystems/tools.md)
- [Memory Subsystem](architecture/subsystems/memory.md) — typed value model, TmpStore, SessionHandle (v0.4.0: typed Value/Collection model, `TmpStore`, memory always enabled)

Operations:

- [Deployment](operations/deployment.md)
- [Monitoring](operations/monitoring.md)

Development:

- [Contribution Guide](development/contributing.md)
- [Testing](development/testing.md)

If you're new, read **Quick Intro** and then **Getting Started**.

---

<- 93 tests source: docs/quick-intro.md -->
# 🌸 Araliya Bot — Modular Agentic Assistant

**Araliya Bot** is a fast, modular, and fully autonomous AI assistant infrastructure built in Rust. It operates as a single-process supervisor with pluggable subsystems, designed to act as a cohesive agentic AI.

## ✨ Highlights

- **Modular Architecture:** The bot acts as the main entry point (supervisor), owning the event bus and managing global events.
- **Pluggable Subsystems:** Subsystems are separate modules that can be toggled on/off at startup. They can dynamically load, unload, and manage multiple agents at runtime.
- **Event-Driven Communication:** Subsystems and agents communicate seamlessly with each other and the supervisor via a central event bus.
- **Secure Identity:** Automatically generates a persistent cryptographic identity (ed25519 keypair) on the first run, ensuring secure and verifiable operations.
- **Lean & Fast:** Built in Rust for minimal overhead, fast cold starts, and memory safety.

## 🚀 Quick Start (TL;DR)

**Requirements:** Rust toolchain (1.80+), Linux/macOS

```bash
# Clone the repository
git clone <repo>
cd araliya-bot

# Build the release binary
cargo build --release

# Run the supervisor
cargo run --release
```

On the first run, a persistent bot identity is generated at `~/.araliya/bot-pkey{id}/`.

```text
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

### Logging & Debugging

Log verbosity can be set at runtime with `-v` flags:

```bash
cargo run -- -v      # warn  (quiet — errors and warnings only)
cargo run -- -vv     # info  (normal operational output)
cargo run -- -vvv    # debug (routing, handler registration, diagnostics)
cargo run -- -vvvv   # trace (full payload dumps, very verbose)
```

## 🏗️ Architecture

Araliya Bot is designed around a flexible, event-driven architecture:

1. **Supervisor:** The core application. It holds the primary public-key identity, handles global events, and owns the event bus.
2. **Subsystems:** Independent modules that provide specific capabilities. They can be enabled or disabled via configuration.
3. **Agents:** Autonomous actors loaded and managed by the agents subsystem at runtime. Each agent can be granted access to the event bus and memory system.
4. **Event Bus:** The central nervous system of the bot, routing messages between the supervisor, subsystems, and agents.

## 📊 Comparison

| Feature | Araliya Bot 🌸 | ZeroClaw 🦀 | OpenClaw 🦞 |
| :--- | :--- | :--- | :--- |
| **Language** | Rust | Rust | TypeScript / Node.js |
| **Architecture** | Single-process supervisor, pluggable subsystems, event bus | Trait-driven, single binary, swappable providers/channels | Gateway WS control plane, multi-agent routing |
| **Memory Footprint** | ~6.1 MB  | < 5MB | > 1GB |
| **Startup Time** | < 1s | < 10ms | > 500s |
| **Binary Size** | ~3.5 MB  | ~3.4 MB | ~28MB (dist) |
| **Identity** | ed25519 keypair persisted, Markdown identity | AIEOS (JSON) or OpenClaw (Markdown) | Markdown files (IDENTITY.md, SOUL.md, etc.) |
| **Security** | Persistent ed25519 identity implemented; pairing/sandboxing/allowlists not implemented (see `notes/` for design) | Gateway pairing, strict sandboxing, explicit allowlists | Gateway pairing, sandboxing, allowlists |
| **Channels** | PTY enabled by default; auto-disabled when stdio management is attached (virtual `/chat` path). Telegram available by feature/config | CLI, Telegram, Discord, Slack, WhatsApp, etc. | WhatsApp, Telegram, Slack, Discord, etc. |
| **Memory System** | Basic session store: capped k-v JSON + capped Markdown transcript, UUIDv7 sessions under identity dir | SQLite hybrid search, PostgreSQL, Lucid bridge | *No info* |
| **Tools** | No general tool subsystem; built-in agents: `echo`, `basic_chat`, `chat` (`src/subsystems/agents/`) | Shell, file, memory, cron, browser, composio | Browser control, Canvas, Nodes, Skills |

## 📈 Benchmarks (CI) - NOT SETUP [TODO]

A GitHub Actions workflow has been added at `.github/workflows/benchmarks.yml` that measures:

- `binary size` (`target/release/araliya-bot`)
- `startup latency` (time from process start until the log line `identity ready — starting subsystems`)
- `memory RSS` (VmRSS while running)

Run locally:

```bash
cargo build --release
./target/release/araliya-bot & sleep 1; pkill araliya-bot
```

Example local measurement (observed on this machine):

```text
$ ls -lh target/release/araliya-bot
-rwxr-xr-x. 2 sachi sachi 3.5M Feb 19 13:00 target/release/araliya-bot

# sample process info (RES)
PID    USER   RSS
603255 sachi  6.1M
```

## ⚙️ Configuration & Secrets

Araliya Bot strictly separates configuration from secrets:

- **Configuration:** Non-sensitive settings belong in `config/` (e.g., `config/default.toml`).
- **Secrets:** API keys (e.g., `LLM_API_KEY`) and tokens must be provided via environment variables or an `.env` file. 
  > **Note:** Never commit secrets. The `.env` file must remain gitignored.

## 🛠️ Development

We expect a clean and efficient developer workflow:

- Use `cargo check` for quick validation during development.
- Use `cargo test` to ensure reliability when behavior changes.
- Keep dependencies minimal. Prefer small, single-purpose crates over large frameworks.

## 📚 Documentation

Dive deeper into the specifics of Araliya Bot:

- [Getting Started](getting-started.md) — Build, run, and verify your setup.
- [Configuration](configuration.md) — Detailed guide on config files and environment variables.
- [Architecture Overview](architecture/overview.md) — In-depth look at the system design and event bus.
- [Identity](architecture/identity.md) — How cryptographic identities and bot IDs work.
- [Operations](operations/deployment.md) — Guide for running Araliya Bot in production.
- [Development](development/contributing.md) — Guidelines for contributing and testing.

---

<- 93 tests source: docs/getting-started.md -->
# Getting Started

## Requirements

- Rust toolchain 1.80+ (`rustup` recommended)
- Linux or macOS
- Internet access for initial `cargo build` (downloads dependencies)

## Build

```bash
cd araliya-bot
cargo build
```

### Modular Features

Araliya Bot uses Cargo features to enable or disable subsystems, plugins, and channels at compile-time. This allows for lean builds on resource-constrained hardware.

| Feature Group | Features | Description |
|---------------|----------|-------------|
| **Subsystems**| `subsystem-agents`, `subsystem-llm`, `subsystem-comms`, `subsystem-memory` | Main architectural blocks. |
| **Agents**    | `plugin-echo`, `plugin-basic-chat`, `plugin-chat`, `plugin-gmail-agent` | Capabilities for the `agents` subsystem. |
| **Channels**  | `channel-pty`, `channel-http`, `channel-telegram` | I/O channels for the `comms` subsystem. |
| **Tools**     | `subsystem-tools`, `plugin-gmail-tool` | Tool execution and implementations. |
| **UI**        | `subsystem-ui`, `ui-svui`, `ui-gpui` | Web UI backend and optional GPUI desktop client. |
| **Binaries**  | `cli`, `gmail-app` | Additional binaries (`araliya-ctl`, `gmail_read_one`). |

**Default build (Daemon only, all subsystems enabled):**
```bash
cargo build
```

**Build all binaries (Daemon, CLI, Gmail App):**
```bash
cargo build --all-features
```

**Minimal build (No subsystems enabled):**
```bash
cargo build --no-default-features
```

**Custom build (LLM and Agents only):**
```bash
cargo build --no-default-features --features subsystem-llm,subsystem-agents,plugin-basic-chat
```

For a release build:
```bash
cargo build --release --locked
```

### Building the Web UI

The Svelte UI lives in `ui/svui/` and builds to `ui/build/`:

```bash
cd ui/svui
pnpm install
pnpm build
```

The bot serves the built UI at `http://127.0.0.1:8080/ui/` when `comms.http.enabled = true` and `ui.svui.enabled = true`.

For development with hot reload:

```bash
cd ui/svui
pnpm dev   # starts on http://localhost:5173/ui/
```

Set `VITE_API_BASE_URL=http://127.0.0.1:8080` in the dev environment to proxy API calls to the running bot.

### Building the GPUI Desktop Client

The optional native desktop client is provided as a separate binary under `src/bin/araliya-gpui/` and is gated behind the `ui-gpui` feature.

**Linux system dependencies** (XCB and XKB libraries) must be installed first — see [docs/development/gpui.md](development/gpui.md) for details and distro-specific install commands.

```bash
cargo check --bin araliya-gpui --features ui-gpui
cargo run --bin araliya-gpui --features ui-gpui
```

By default it targets `http://127.0.0.1:8080` and expects the bot API to be running there.

For CI/reproducible environments:

```bash
cargo build --release --locked --frozen
```

Binary output: `target/debug/araliya-bot` or `target/release/araliya-bot`.

Quick size checks:

```bash
ls -lh target/release/araliya-bot
size target/release/araliya-bot
readelf -S target/release/araliya-bot | grep -E '\.debug|\.symtab|\.strtab'
```

## Run

### Daemon mode (default)

```bash
cargo run
# or
./target/debug/araliya-bot
```

No stdin is read, no stdout is written. All tracing output goes to stderr (journald-compatible). The Unix domain socket at `{work_dir}/araliya.sock` is always active for management.

### Interactive mode

```bash
./target/debug/araliya-bot -i
```

Activates the stdio management adapter and PTY channel:

```
# /status
# /health
# /chat <message>
# /exit
```

### GPUI Desktop mode

Run the bot API and GPUI desktop client in separate terminals:

```bash
# Terminal 1: bot API
cargo run

# Terminal 2: desktop UI
cargo run --bin araliya-gpui --features ui-gpui
```

The GPUI client currently covers baseline chat flows: health status, sessions list, transcript view, and message send.

### Management CLI (`araliya-ctl`)

While the daemon is running (in either mode), use `araliya-ctl` from any terminal:

```bash
./target/debug/araliya-ctl status
./target/debug/araliya-ctl health
./target/debug/araliya-ctl subsystems
./target/debug/araliya-ctl shutdown
```

Socket path resolution: `--socket <path>` → `$ARALIYA_WORK_DIR/araliya.sock` → `~/.araliya/araliya.sock`.

### First Run

On first run the bot generates a persistent ed25519 keypair and saves it to `~/.araliya/bot-pkey{id}/`. Expected output:

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

### Subsequent Runs

The existing keypair is loaded. The same `bot_id` is printed every time:

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

## Verify

```bash
# Check identity files were created
ls ~/.araliya/
# → bot-pkey5d16993c/

ls ~/.araliya/bot-pkey5d16993c/
# → id_ed25519   id_ed25519.pub

# Verify secret key permissions
stat -c "%a %n" ~/.araliya/bot-pkey5d16993c/id_ed25519
# → 600 ...
```

## Environment Variables

| Flag / Variable | Effect |
|-----------------|--------|
| `-i` / `--interactive` | Enable interactive mode (management adapter + PTY). Default: daemon mode, no stdio. |
| `ARALIYA_WORK_DIR` | Override working directory (default: `~/.araliya`) |
| `ARALIYA_LOG_LEVEL` | Override log level (default: `info`) |
| `RUST_LOG` | Standard tracing env filter (overrides `log_level`) |
| `-v` | CLI override → `warn` |
| `-vv` | CLI override → `info` |
| `-vvv` | CLI override → `debug` |
| `-vvvv` | CLI override → `trace` |

Example:

```bash
ARALIYA_WORK_DIR=/tmp/test-bot RUST_LOG=debug cargo run

# CLI verbosity override
cargo run -- -vvv
```

## Run Tests

```bash
cargo test
```

All 41 tests should pass. Tests use `tempfile` for filesystem isolation — they do not touch `~/.araliya`.

---

<- 93 tests source: docs/configuration.md -->
# Configuration

## Config File

Primary config: `config/default.toml` (relative to working directory).

```toml
[supervisor]
bot_name = "araliya"
work_dir = "~/.araliya"
identity_dir = "bot-pkey51aee87e" # optional, absolute path or relative to work_dir
log_level = "info"

[comms.pty]
enabled = true

[comms.telegram]
enabled = false

[comms.http]
enabled = false
bind = "127.0.0.1:8080"

[agents]
default = "basic_chat"

[agents.routing]
# pty0 = "echo"

[agents.chat]
memory = ["basic_session"]

[memory]
# Global memory settings

[memory.basic_session]
# kv_cap = 200
# transcript_cap = 500

[llm]
default = "dummy"

[tools.newsmail_aggregator]
mailbox = "inbox"
n_last = 10
# tsec_last = 86400
```

When `comms.http.enabled = true`, the HTTP channel exposes `GET /health` on
`comms.http.bind` and forwards the request to the management bus method
`manage/http/get`.

### Full-Featured Config (`full.toml`)

`config/full.toml` is a **partial overlay** that inherits from `default.toml` via `[meta] base`.  It only lists the keys that differ from the base, so it stays short and easy to read:

```toml
[meta]
base = "default.toml"  # path relative to this file

[comms.telegram]
enabled = true

[agents]
default = "chat"
# … only changed entries …
```

To use it:

```bash
cargo run -- -f config/full.toml
```

For the news agent overlay:

```bash
cargo run -- -f config/news.toml
```

The loader follows the `base` chain automatically, deep-merging each layer so that overlay keys win and everything else comes from the base.

### Config Inheritance (`[meta] base`)

Any config file can declare a base file it extends:

```toml
[meta]
base = "default.toml"  # relative to *this* file's directory, or absolute
```

**Merge rules**

- **Tables** are merged recursively — only the keys present in the overlay are changed; everything else is inherited.
- **Scalars and arrays** follow the overlay-wins rule.
- **Chains are supported** — the base can itself have a `[meta] base`, creating a stack (grandbase → base → overlay).
- **Circular references** are detected and reported as a config error.
- The `[meta]` table is internal bookkeeping and is stripped before the config is resolved.

**Creating your own overlay**

```toml
# config/local.toml
[meta]
base = "default.toml"

[supervisor]
log_level = "debug"

[llm.openai]
model = "gpt-4o-mini"
```

```bash
cargo run -- -f config/local.toml
```

## Modular Features (Cargo Flags)

Araliya Bot is built with **compile-time modularity**. If a subsystem or plugin is disabled via Cargo feature, it will not be loaded even if configured in `default.toml`.

| Feature | Enable/Disable | Mandatory |
|---------|----------------|----------|
| `subsystem-agents` | `--features subsystem-agents` | Yes, for agent logic |
| `subsystem-llm` | `--features subsystem-llm` | Yes, for completion tools |
| `subsystem-comms` | `--features subsystem-comms` | Yes, for PTY/HTTP I/O |
| `subsystem-memory` | `--features subsystem-memory` | No, for session memory |
| `subsystem-tools` | `--features subsystem-tools` | No, for tools execution |
| `channel-pty` | `--features channel-pty` | No, for terminal console |
| `channel-http` | `--features channel-http` | No, for HTTP `/health` channel |
| `channel-telegram` | `--features channel-telegram` | No, for Telegram bot |
| `plugin-gmail-tool` | `--features plugin-gmail-tool` | No, Gmail tool implementation |
| `plugin-gmail-agent` | `--features plugin-gmail-agent` | No, `agents/gmail/read` agent |
| `plugin-news-agent` | `--features plugin-news-agent` | No, `agents/news/(handle\|read)` via `newsmail_aggregator/get` |
| `ui-gpui` | `--features ui-gpui` | No, enables the `araliya-gpui` desktop binary |

If you disable a subsystem but leave its configuration in `default.toml`, the bot will proceed normally but will not initialize the corresponding handler.

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_name` | string | `"araliya"` | Human-readable name for this instance |
| `work_dir` | path | `"~/.araliya"` | Root directory for all persistent data. `~` expands to `$HOME`. |
| `identity_dir` | path (optional) | none | Explicit identity directory. Required to disambiguate when multiple `bot-pkey*` dirs exist. |
| `log_level` | string | `"info"` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |

## Comms Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `comms.pty.enabled` | bool | `true` | Enables PTY (console) channel. Only active when `-i` / `--interactive` is passed at runtime. Without `-i` the bot runs as a daemon with no stdio I/O. |
| `comms.telegram.enabled` | bool | `false` | Enables Telegram channel (requires `TELEGRAM_BOT_TOKEN`). |
| `comms.http.enabled` | bool | `false` | Enables HTTP channel with API and UI serving. |
| `comms.http.bind` | string | `"127.0.0.1:8080"` | TCP bind address for HTTP channel listener. |

### HTTP Routes

| Path | Description |
|------|-------------|
| `GET /` | Root welcome page (always available, even without UI subsystem). |
| `GET /api/health` | JSON health status from management bus. |
| `/ui/*` | Delegated to the UI backend when `ui.svui.enabled = true`. |

## UI Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ui.svui.enabled` | bool | `true` | Enables the Svelte-based web UI backend. |
| `ui.svui.static_dir` | string (optional) | none | Path to the static build directory. Relative to the bot's working directory. If absent, a built-in placeholder is served. |

The UI is a SvelteKit SPA built with shadcn-svelte, served at `/ui/`. Build it with:

```bash
cd ui/svui && pnpm install && pnpm build
```

The build output goes to `ui/build/`, which matches the default `static_dir` setting.

## Agents Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.default` | string | `"basic_chat"` | Which agent handles unrouted messages. |
| `agents.routing` | map<string,string> | `{}` | Optional `channel_id -> agent_id` routing overrides. |
| `agents.{id}.memory` | array<string> | `[]` | Memory store types this agent requires (e.g. `["basic_session"]`). |

Gmail agent endpoint:

- Bus method: `agents/gmail/read`
- Internal tool call: `tools/execute` with `tool = "gmail"`, `action = "read_latest"`

Newsmail aggregator tool endpoint:

- Bus method: `tools/execute`
- Tool/action: `tool = "newsmail_aggregator"`, `action = "get"`
- Current request shape: empty `{}` supported; optional keys are `label`, `mailbox`, `n_last`, `t_interval` (preferred), `tsec_last` (legacy)
- Healthcheck action: `tool = "newsmail_aggregator"`, `action = "healthcheck"` (returns one `newsletter`-filtered sample when available)

News agent endpoint (MVP):

- Bus methods: `agents/news` (default `handle` action), `agents/news/read`, `agents/news/health`
- Internal tool call: `tools/execute` with `tool = "newsmail_aggregator"`, `action = "get"`
- Health path: `agents/news/health` returns local component status text
- Interactive shortcut: `/health news` in stdio mode
- Default query args can be set in config via `[agents.news.query]`

`agents.news.query` fields:

- `label` (optional string, Gmail label name e.g. `n/News`)
- `mailbox` (optional string)
- `n_last` (optional integer)
- `t_interval` (optional string duration, e.g. `1min`, `1d`, `1mon`)
- `tsec_last` (optional integer seconds, legacy fallback)

## Tools Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tools.newsmail_aggregator.mailbox` | string | `"inbox"` | Gmail mailbox/query base used by `newsmail_aggregator/get`. |
| `tools.newsmail_aggregator.n_last` | usize | `10` | Maximum number of latest emails to fetch before local filtering. |
| `tools.newsmail_aggregator.tsec_last` | integer (optional) | none | Optional recent window in seconds. Only emails newer than `now - tsec_last` are returned. |

## Memory Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum key-value entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |

## LLM Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active LLM provider (`"dummy"` or `"openai"`). Requires `subsystem-llm` feature. |
| `llm.openai.api_base_url` | string | OpenAI endpoint | Chat completions URL. Override for Ollama / LM Studio. |
| `llm.openai.model` | string | `"gpt-4o-mini"` | Model name sent in each request. |
| `llm.openai.temperature` | float | `0.2` | Sampling temperature (omitted automatically for `gpt-5` family). |
| `llm.openai.timeout_seconds` | integer | `60` | Per-request HTTP timeout in seconds. |
| `llm.openai.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.openai.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.openai.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |

Pricing fields are used by `SessionHandle::accumulate_spend` to write per-session `spend.json` sidecars after each LLM turn. They default to `0.0` so cost is silently omitted rather than wrong when not set.

Provider API keys are never stored in config — supply them via environment or `.env`:

## CLI Flags

| Flag | Effect |
|------|--------|
| `-h`, `--help` | Print help information and exit. |
| `-i`, `--interactive` | Activates the stdio management adapter (`/status`, `/health`, `/chat`, …) and the PTY channel. Without this flag the bot runs as a daemon — no stdin is read and no stdout is written. |
| `-f`, `--config <PATH>` | Path to configuration file (default: `config/default.toml`). |
| `-v` … `-vvvv` | Override log level (see Verbosity table below) |

## CLI Verbosity Flags

You can override log level at runtime with `-v` flags. Each additional `-v` raises the verbosity one tier:

| Flags | Effective level | What you see |
|-------|-----------------|------|
| *(none)* | config/env default | whatever `log_level` is set to |
| `-v` | `warn` | warnings and errors only |
| `-vv` | `info` | normal operational output |
| `-vvv` | `debug` | routing, handler registration, flow diagnostics |
| `-vvvv`+ | `trace` | full payload dumps, very verbose |

## Environment Variable Overrides

Env vars take precedence over `default.toml` values.

| Variable | Overrides | Example |
|----------|-----------|---------|
| `ARALIYA_WORK_DIR` | `work_dir` | `ARALIYA_WORK_DIR=/data/bot` |
| `ARALIYA_LOG_LEVEL` | `log_level` | `ARALIYA_LOG_LEVEL=debug` |
| `RUST_LOG` | `log_level` (full filter syntax) | `RUST_LOG=araliya_bot=debug` |

`RUST_LOG` uses the standard `tracing` env-filter syntax and overrides `ARALIYA_LOG_LEVEL` when both are set.

## Secrets

Secrets must come from environment variables or `.env`, never from config files.

| Variable | Purpose |
|----------|---------|
| `LLM_API_KEY` | LLM provider API key |
| `TELEGRAM_BOT_TOKEN` | Telegram channel token |
| `GOOGLE_CLIENT_ID` | Gmail OAuth desktop client ID |
| `GOOGLE_CLIENT_SECRET` | Optional Gmail OAuth client secret |
| `GOOGLE_REDIRECT_URI` | Optional loopback callback URI for Gmail tool |

A `.env` file in `araliya-bot/` is loaded automatically at startup if present. It is gitignored — never commit it.

```bash
# .env
LLM_API_KEY=sk-...
TELEGRAM_BOT_TOKEN=123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11
```

## Resolution Order

Highest precedence wins:

1. CLI `-v` through `-vvvv` flags (log level only)
2. `RUST_LOG` env var (log level only)
3. `ARALIYA_*` env vars
4. `.env` file values
5. Selected config file (overlay, if `[meta] base` is set; base layers applied first, then overlay)
6. `config/default.toml` (if no `-f` flag)
7. Built-in defaults

## Data Directory Layout

All persistent data is stored under `work_dir` (default `~/.araliya`):

```
~/.araliya/
└── bot-pkey{8-hex-bot_id}/     bot identity directory
    ├── id_ed25519               ed25519 signing key seed (mode 0600)
    ├── id_ed25519.pub           ed25519 verifying key (mode 0644)
    └── memory/                  session data (when subsystem-memory enabled)
        ├── sessions.json        session index (includes spend summary per session)
        └── sessions/
            └── {uuid}/
                ├── kv.json
                ├── transcript.md
                └── spend.json   token & cost totals (created on first LLM turn)
```

See [Memory Subsystem](architecture/subsystems/memory.md) for details on session data layout.

---

# Architecture


---

<- 93 tests source: docs/architecture/overview.md -->
# Architecture Overview

**Status:** v0.5.0 — generic subsystem runtime · `BusHandler` trait · concurrent channel tasks · `Component` trait · `Agent` trait · `OpenAiCompatibleProvider` · capability-scoped state · **Compile-time modularity via Cargo Features** · **Chat-family agent composition (`ChatCore`)** · **Memory subsystem with pluggable stores (`basic_session`)** · **UI subsystem (`svui` backend)** · **Cron subsystem (timer-based event scheduling)** · **Tools subsystem (Gmail MVP)** · **LLM token usage tracking + per-session cost accumulation (`spend.json`)**.

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
│  ┌─────────────┐   ┌─────────────┐  ┌────────────┐  │
│  │   Comms     │   │   Memory    │  │    Cron    │  │
│  │  Subsystem  │   │   System    │  │  Subsystem │  │
│  │PTY│HTTP│Tg. │   │basic_session│  │ timer svc  │  │
│  └──────┬──────┘   └──────┬──────┘  └─────┬──────┘  │
│         │                 │               │         │
│  ┌──────┴─────────────────┴───────────────┴──┐      │
│  │       Typed Channel Router (star hub)     │      │
│  └──────┬─────────────────┬──────────────────┘      │
│         │                 │                         │
│  ┌──────┴──────┐  ┌───────┴──────┐  ┌────────────┐  │
│  │   Agents    │  │     LLM      │  │   Tools    │  │
│  │  Subsystem  │  │  Subsystem   │  │  Subsystem │  │
│  │             │  │ DummyProvider│  │  Gmail MVP │  │
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
| `channel-telegram` | Channel | Telegram bot channel via teloxide (requires `TELEGRAM_BOT_TOKEN`). |
| `subsystem-ui` | UI | Display-oriented interface providers. |
| `ui-svui` | UI backend | Svelte-based web UI — static file serving (requires `subsystem-ui`). |
| `subsystem-cron` | Cron | Timer-based event scheduling — emits bus notifications on schedule. |
| `subsystem-tools` | Tools | Tool execution subsystem for agent-delegated actions. |
| `plugin-gmail-tool` | Tool | Gmail read_latest tool implementation (OAuth). |
| `plugin-gmail-agent` | Agent | Gmail agent — `agents/gmail/read` endpoint. |

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
| Comms — Telegram channel | [comms.md](subsystems/comms.md) | Implemented (Optional feature: `channel-telegram`) |
| UI — svui backend | [subsystems/ui.md](subsystems/ui.md) | Implemented (Optional features: `subsystem-ui`, `ui-svui`) |
| Memory System | [subsystems/memory.md](subsystems/memory.md) | Implemented — `basic_session` store (Optional feature: `subsystem-memory`) |
| Agents | [subsystems/agents.md](subsystems/agents.md) | Implemented (Optional features: `plugin-echo`, `plugin-basic-chat`, `plugin-chat`) |
| LLM Subsystem | [subsystems/llm.md](subsystems/llm.md) | Implemented (Optional feature: `subsystem-llm`) |
| Cron | [subsystems/cron.md](subsystems/cron.md) | Implemented (Optional feature: `subsystem-cron`) |
| Tools | [subsystems/tools.md](subsystems/tools.md) | Implemented — Gmail MVP (Optional feature: `subsystem-tools`) |
| Management | — | Implemented (always-on) — health API, cron status aggregation |

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
  │                  .with_llm_rates(rates_from_config)    wire pricing into agent state
  ├─ (conditional) CronSubsystem::new(bus_handle, shutdown)  cron timer service
  ├─ ManagementSubsystem::new(control, bus_handle, info)  health/status handler
  ├─ (conditional) handlers = vec![Box::new(agents), Box::new(llm), Box::new(cron), ...]  register handlers
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
  ├─ "cron/*"    → cron.handle_request(method, payload, reply_tx)
  ├─ "manage/*"  → management.handle_request(method, payload, reply_tx)
  ├─ "tools/*"   → tools.handle_request(method, payload, reply_tx)
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
- [subsystems/cron.md](subsystems/cron.md) — timer-based event scheduling, schedule/cancel/list API
- [subsystems/tools.md](subsystems/tools.md) — tool execution, Gmail MVP
- [standards/index.md](standards/index.md) — bus protocol, component runtime, plugin interfaces, capabilities model

---

<- 93 tests source: docs/architecture/identity.md -->
# Identity

## Overview

Each Araliya instance, as well as its individual agents and subagents, has a persistent **ed25519 keypair**. The keypair is generated on first run and then loaded on every subsequent run. It is the basis for:

- A stable `public_id` that identifies the entity (bot, agent, or subagent)
- (Future) signing outbound events and messages
- (Future) authenticating to external services

## public_id

`public_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.

```
verifying_key_bytes (32 bytes)
  → SHA256 → hex string (64 chars)
  → first 8 chars = public_id
```

Example: `5d16993c`

The `public_id` names the identity directory for the bot:

```
~/.araliya/bot-pkey5d16993c/
```

## File Layout

The identity system is hierarchical. The main bot identity sits at the root, and agent/subagent identities are nested within the bot's memory directory.

```
{work_dir}/
└── bot-pkey{bot_public_id}/
    ├── id_ed25519        32-byte signing key seed (raw bytes, mode 0600)
    ├── id_ed25519.pub    32-byte verifying key (raw bytes, mode 0644)
    └── memory/
        └── agent/
            └── {agent_name}-{agent_public_id}/
                ├── id_ed25519
                ├── id_ed25519.pub
                └── subagents/
                    └── {subagent_name}-{subagent_public_id}/
                        ├── id_ed25519
                        └── id_ed25519.pub
```

- `id_ed25519` — the secret key seed. Must be kept private. Mode `0600` (owner read/write only).
- `id_ed25519.pub` — the public verifying key. Safe to share. Mode `0644`.

## Lifecycle

### Bot Identity
```
identity::setup(&config)
  ├─ scan work_dir for bot-pkey*/ directory containing id_ed25519
  ├─ if found:
  │   ├─ load id_ed25519 (32 bytes)
  │   ├─ load id_ed25519.pub (32 bytes)
  │   ├─ reconstruct verifying key from seed
  │   ├─ verify reconstructed vk == stored pub (integrity check)
  │   └─ return Identity
  └─ if not found:
      ├─ generate new ed25519 keypair (OsRng)
      ├─ compute public_id from verifying key
      ├─ create {work_dir}/bot-pkey{public_id}/
      ├─ save id_ed25519 (mode 0600)
      ├─ save id_ed25519.pub (mode 0644)
      └─ return Identity
```

### Agent & Subagent Identities
Agents and subagents use `identity::setup_named_identity(base_dir, prefix)`. This function scans the `base_dir` for a directory starting with `{prefix}-`. If found, it loads the keys. If not, it generates a new keypair, computes the `public_id`, and creates the directory `{prefix}-{public_id}`.

## Security Notes

- The secret key seed (`id_ed25519`) file mode is enforced to `0600` on Unix at creation time
- The key is never logged or printed
- Backup `id_ed25519` to retain identity across machine changes; losing it generates a new identity with a different `public_id`

## Identity Struct

```rust
pub struct Identity {
    pub public_id: String,    // "5d16993c"
    pub identity_dir: PathBuf // ~/.araliya/bot-pkey5d16993c/
    // private fields: verifying_key, signing_key_seed
}
```

## Standards


---

<- 93 tests source: docs/architecture/standards/index.md -->
# Standards & Protocols

This section contains normative specifications for the fundamental contracts that all components in the araliya-bot architecture must follow. These are reference documents — they describe what *must* be true for a component to integrate correctly, not how any one subsystem happens to work internally.

---

## Documents

| Spec | What it covers | Status |
|------|---------------|--------|
| [Bus Protocol](bus-protocol.md) | Method naming, `BusMessage` variants, payload enum, error codes, `BusHandle` API, `BusHandler` registration | Implemented |
| [Component Runtime](runtime.md) | `Component` trait, `ComponentFuture`, `spawn_components`, cancellation / fail-fast model | Implemented |
| [Plugin Interfaces](plugin-interfaces.md) | `Agent`, `BusHandler`, `LlmProvider` enum-dispatch extension pattern | Implemented |
| [Capabilities Model](capabilities.md) | Typed capability objects, planned permission enforcement | Planned |

---

## Why a standards section?

The subsystem docs under `architecture/subsystems/` describe what each subsystem does. This section describes the *shared contracts* those subsystems depend on — things like the event bus wire format, the component lifecycle, and how extension points are defined. Any contributor adding a new subsystem, plugin, or provider should read the relevant spec here before writing code.

---

<- 93 tests source: docs/architecture/standards/bus-protocol.md -->
# Bus Protocol

**Status:** Implemented — `supervisor/bus.rs`, `supervisor/dispatch.rs`

The supervisor event bus is the communication channel between subsystems. This document specifies the protocol every subsystem participant must follow.

Supervisor management/control commands are intentionally outside this protocol and live on the supervisor-internal control plane (`supervisor/control.rs`).

---

## Design basis

The protocol follows **JSON-RPC 2.0 semantics** — request/response correlation by `id`, structured error objects with numeric codes, and a clear separation between requests (expecting a reply) and notifications (fire-and-forget). The in-process implementation uses Tokio channels; the type definitions are IPC-ready without callsite changes (see [IPC migration path](#ipc-migration-path)).

### Why a central bus and not direct calls?

The bus adds ~100–500 ns per request hop — negligible compared to the I/O it orchestrates (an LLM call is ~200 ms, a Gmail API call ~500 ms). Replies travel back via `oneshot` channels that bypass the supervisor entirely, so a full request–reply chain (e.g. PTY → agents → tools → agents → PTY) costs only 2 supervisor hops, not 4.

The star topology was chosen over alternatives for these reasons:

| Alternative | Why not |
|---|---|
| Direct function calls | Tight coupling; no centralised logging, cancellation, or permission enforcement |
| Actor-per-entity (Actix / Erlang) | Better at thousands of independent actors; we have <20 subsystems — a hub is simpler |
| Broker (NATS, Kafka) | 100–1000× slower (network + serialisation + persistence); designed for distributed systems we don't need yet |
| gRPC / microservices | Full network stack per call (~1–5 ms); premature for a single-process bot |

The single-threaded supervisor loop can dispatch ~2–5 M msgs/sec; this only becomes a bottleneck at scales far beyond the current design. If it ever does, the [IPC migration path](#ipc-migration-path) lets us shard without changing caller code.

---

## Method naming

Method strings use `/`-separated path segments:

```
"subsystem/component/action"
```

Examples:
- `"agents"` — agents subsystem, default component, default action
- `"agents/echo/handle"` — explicit agent + action
- `"llm/complete"` — LLM subsystem, complete action

**Reserved prefix:** `$/` is reserved for system-level events (e.g. `"$/cancel"`). No subsystem may register a handler with a prefix beginning with `$`.

The supervisor dispatches by the **first segment only**. Everything after the first `/` is passed verbatim to the handler for secondary routing. So `"agents/echo/handle"` is delivered to the `"agents"` handler, which sees the full method string and routes internally.

---

## Message kinds

```rust
pub enum BusMessage {
    Request {
        id: Uuid,
        method: String,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    },
    Notification {
        method: String,
        payload: BusPayload,
    },
}
```

### Request

- Caller awaits exactly one reply via the embedded `oneshot::Sender<BusResult>`.
- `reply_tx` is `!Clone` — single-recipient delivery is enforced at compile time.
- `id` is a `Uuid` generated by `BusHandle::request`; becomes the wire `id` field when IPC is added.
- Handlers **must not block the supervisor loop** — resolve `reply_tx` synchronously or move it into a `tokio::spawn` task.

### Notification

- Fire-and-forget. No reply channel, no `id`.
- Sent via `BusHandle::notify`, which uses `try_send` (non-blocking).
- **Intentionally lossy under back-pressure**: if the bus buffer is full the notification is dropped and `BusCallError::Full` is returned. Callers must log a warning and not retry. If guaranteed delivery is needed, use a `Request` instead.

---

## Payload enum

All known message bodies are variants of `BusPayload`. Every variant derives `Serialize + Deserialize` — the enum is IPC-ready without any callsite changes.

```rust
pub enum BusPayload {
    CommsMessage  { channel_id: String, content: String, session_id: Option<String> },
    LlmRequest    { channel_id: String, content: String },
    CancelRequest { id: Uuid },
    SessionQuery  { session_id: String },
    JsonResponse  { data: String },
    Empty,
}
```

`channel_id` is threaded through `LlmRequest` so the LLM subsystem can re-attach it to the `CommsMessage` reply, enabling callers to correlate replies with the originating channel without extra bookkeeping.

`session_id` on `CommsMessage` threads session identity end-to-end: comms channels send an optional `session_id` inbound, agents attach the memory session id on the reply, and the HTTP API returns it to the client.

`SessionQuery` / `JsonResponse` support structured subsystem queries (e.g. session list, session detail) without overloading `CommsMessage`.

When adding a new message type, add a new variant here. Do not reuse an existing variant for a semantically different message.

---

## Error codes

```rust
pub struct BusError {
    pub code: i32,
    pub message: String,
}

pub const ERR_METHOD_NOT_FOUND: i32 = -32601;  // mirrors JSON-RPC 2.0
```

`BusError` mirrors the JSON-RPC 2.0 error object. `ERR_METHOD_NOT_FOUND` (`-32601`) is returned by the supervisor when no handler is registered for the incoming method prefix. Application-level errors use the range `-32000` to `-32099` (JSON-RPC 2.0 server-defined errors).

---

## `BusHandle` API

`BusHandle` is the **only public surface** subsystems and plugins touch. Raw channel types are not exposed outside `bus.rs`. It is `Clone` and inexpensive to pass around.

```rust
// Send a request and await exactly one reply.
pub async fn request(
    &self,
    method: impl Into<String>,
    payload: BusPayload,
) -> Result<BusResult, BusCallError>

// Send a notification. Non-blocking (try_send). Lossy under back-pressure.
pub fn notify(
    &self,
    method: impl Into<String>,
    payload: BusPayload,
) -> Result<(), BusCallError>
```

`BusCallError` variants:
- `Send` — supervisor `mpsc` receiver was dropped (supervisor is dead)
- `Recv` — supervisor dropped `reply_tx` without sending a reply
- `Full` — notification dropped due to back-pressure (only from `notify`)

---

## `BusHandler` registration contract

Each subsystem implements `BusHandler` and registers with the supervisor at startup:

```rust
pub trait BusHandler: Send + Sync {
    fn prefix(&self) -> &str;
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>);
    fn handle_notification(&self, _method: &str, _payload: BusPayload) {}  // default: no-op
}
```

Rules:
- `prefix()` must be unique. The supervisor **panics at startup** if two handlers share the same prefix.
- `handle_request` receives the **full method string** (e.g. `"agents/echo/handle"`), not just the suffix.
- `handle_notification` has a default no-op; subsystems that don't use notifications need not override it.
- Neither method may block the caller — offload async work to `tokio::spawn`.

The supervisor owns handlers as `Vec<Box<dyn BusHandler>>` and builds a `HashMap<&str, usize>` index at startup for O(1) prefix lookup.

---
TODO: check this section, code and doc
## Observability

The bus and supervisor emit structured `tracing` logs at `debug` and `trace` levels:

| Component | Level | What is logged |
|-----------|-------|----------------|
| `BusHandle::request` | `debug` | Request sent (id, method), request completed (id, ok/err) |
| `BusHandle::request` | `trace` | Full outbound payload, full result payload |
| `BusHandle::request` | `warn` | Send failure (supervisor dead), reply channel dropped |
| `BusHandle::notify` | `debug` | Notification sent (method) |
| `BusHandle::notify` | `trace` | Full notification payload |
| `BusHandle::notify` | `warn` | Buffer full (back-pressure), send failure |
| `SupervisorBus::new` | `debug` | Bus created with buffer size |
| Supervisor loop | `debug` | Handler registration, request/notification routing (id, method, prefix) |
| Supervisor loop | `trace` | Full request/notification payloads at dispatch time |
| Supervisor loop | `warn` | Unhandled request method (with error reply) |

Use `-vvv` (debug) for flow diagnostics, `-vvvv` (trace) for full payload inspection. See [Configuration](../../configuration.md#cli-verbosity-flags) for the full CLI flag table.

---

## IPC migration path

When the architecture is extended to cross a process boundary:

1. Remove `reply_tx` from `BusMessage::Request`.
2. The supervisor stores pending reply senders in a `HashMap<Uuid, oneshot::Sender<BusResult>>`.
3. Serialize `{ id, method, payload }` as JSON over a Unix socket or pipe.
4. Match responses back by `id` and resolve the stored sender.

`BusHandle::request()` is unchanged from callers' perspective — the async/await contract is identical.

---

<- 93 tests source: docs/architecture/standards/capabilities.md -->
# Capabilities Model

**Status:** Planned — typed state objects exist; supervisor-enforced permissions not yet implemented.

---

## Overview

The capabilities model governs what a component is allowed to do. Rather than giving components direct access to the raw `BusHandle` or filesystem, each subsystem exposes a **typed state object** that wraps only the operations that component class is permitted to perform. Supervisor-level permission enforcement is planned but not yet implemented.

---

## Current state: typed capability objects

Each subsystem constructs a typed state struct that hides the raw `BusHandle` and exposes only permitted operations:

| State type | Used by | Permitted operations |
|------------|---------|----------------------|
| `AgentsState` | `Agent` implementations | `complete_via_llm(channel_id, content)`, `memory` field (`Option<Arc<MemorySystem>>`), `agent_memory` field |
| `CommsState` | `Component` implementations in the Comms subsystem | `send_message(channel_id, content)`, `report_event(CommsEvent)` |

The raw `BusHandle` is private within the owning module. Plugins cannot address arbitrary bus methods directly.

---

## Planned: supervisor permission enforcement

The following is the intended design — not yet implemented:

- The supervisor will maintain a permission table: `HashMap<prefix, AllowedMethods>`.
- When a subsystem or plugin sends a `BusMessage::Request`, the supervisor checks the caller's registered permissions before forwarding to the target handler.
- Permission grants are configured at startup; plugins cannot escalate their own permissions.
- A plugin requesting `fs_storage` access would receive a pre-scoped wrapper (e.g. a path-restricted write function) rather than raw filesystem access — analogous to the current `AgentsState`/`CommsState` pattern, generalized.
- System methods prefixed with `$/` are reserved and may only be invoked by the supervisor itself.

---

## Design principle

The current typed-state approach is the foundation of this model. Adding supervisor-level enforcement means instrumenting the dispatch loop in `supervisor/dispatch.rs` with a permission check — the state object pattern does not need to change.

---

<- 93 tests source: docs/architecture/standards/plugin-interfaces.md -->
# Plugin Interfaces

**Status:** v0.3.0 — agents (`src/subsystems/agents/mod.rs`), LLM (`src/llm/mod.rs`), BusHandler (`src/supervisor/dispatch.rs`)

This document specifies the three extension points in the architecture: how to add a new agent, a new LLM provider, and how subsystems register on the bus.

> **Naming convention:** `Agent` refers to autonomous actors in the agents subsystem.
> `Plugin` is reserved for capability extensions in the future tools subsystem.

---

## `Agent` — adding a new agent

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        channel_id: String,
        content: String,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

### Contract

- `id()` must match the name used in config routing and `[agents.routing]` values.
- `handle` must **not block the caller**:
  - Synchronous agents resolve `reply_tx` immediately (see `EchoAgent`).
  - Async agents `tokio::spawn` a task and resolve `reply_tx` from within it (see `BasicChatPlugin`, `SessionChatPlugin`).
- `reply_tx` is `oneshot::Sender<BusResult>` — consume it exactly once. Dropping it without sending causes the caller to receive `BusCallError::Recv`.
- `state: Arc<AgentsState>` is the capability surface. Do not circumvent it to access raw bus handles.

### `AgentsState` capability surface

```rust
impl AgentsState {
    pub async fn complete_via_llm(&self, channel_id: &str, content: &str) -> BusResult;
}

// Fields:
pub memory: Option<Arc<MemorySystem>>,  // when subsystem-memory is enabled
pub agent_memory: HashMap<String, Vec<String>>,  // per-agent store requirements
```

Agents call typed methods on `AgentsState` rather than addressing arbitrary bus targets. The raw `BusHandle` is private to the agents module.

### Built-in agents

| ID | Feature | Behaviour |
|----|---------|----------|
| `echo` | `plugin-echo` | Returns input unchanged; synchronous. |
| `basic_chat` | `plugin-basic-chat` | Delegates to `ChatCore::basic_complete` in a spawned task. |
| `chat` | `plugin-chat` | Session-aware chat via `SessionChatPlugin`; creates a memory session on first message, appends user/assistant transcript entries, injects recent history as LLM context. Requires `subsystem-memory`. |

### Adding an agent

1. Implement `Agent` in a new file under `src/subsystems/agents/`.
   - For chat-family agents, add to `src/subsystems/agents/chat/` and compose with `ChatCore`.
2. Add a Cargo feature gate (e.g. `plugin-myagent = ["subsystem-agents"]`).
3. Register it in `AgentsSubsystem::new()` behind `#[cfg(feature = "plugin-myagent")]`.
4. Add `[agents.myagent]` in `config/default.toml`.
5. If the agent needs memory, add `memory = ["basic_session"]` to its config section.

---

## `BusHandler` — registering a subsystem

```rust
pub trait BusHandler: Send + Sync {
    fn prefix(&self) -> &str;
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>);
    fn handle_notification(&self, _method: &str, _payload: BusPayload) {}
}
```

See [bus-protocol.md](bus-protocol.md#bushandler-registration-contract) for the full specification. Key rules:

- `prefix()` is a string owned exclusively by this handler. The supervisor panics at startup on duplicates.
- `handle_request` receives the **full method string** including the prefix (e.g. `"agents/echo/handle"`).
- Neither method may block — offload to `tokio::spawn`.

### Adding a subsystem

1. Implement `BusHandler` for your subsystem struct.
2. Add it to the `handlers` vec in `main.rs` before calling `supervisor::run`.
3. Announce any new `BusPayload` variants needed and add them to the enum in `bus.rs`.

---

## `LlmProvider` — adding a new model backend

The LLM abstraction uses **enum dispatch** rather than `dyn` trait objects, avoiding `async-trait` and dynamic dispatch overhead.

```rust
pub enum LlmProvider {
    Dummy(providers::dummy::DummyProvider),
    OpenAiCompatible(providers::openai_compatible::OpenAiCompatibleProvider),
}

impl LlmProvider {
    pub async fn complete(&self, content: &str) -> Result<String, ProviderError>;
}
```

### Design rationale

Enum dispatch was chosen over `dyn LlmProvider` because:
- Rust's async/await does not work directly with trait objects without the `async-trait` crate.
- The set of providers is known at compile time and changes infrequently.
- Enum dispatch is zero-cost — no heap allocation or vtable lookup per call.

### Adding a provider

1. Create `src/llm/providers/<name>.rs` with a struct implementing `async fn complete(&self, content: &str) -> Result<String, ProviderError>`.
2. Add a variant to `LlmProvider`.
3. Add a match arm to `LlmProvider::complete`.
4. Add a build case in `src/llm/providers/mod.rs` (`providers::build`).
5. Add configuration fields under `[llm]` in `config/default.toml` and wire them in `config.rs`.

### Current providers

| Variant | Config `provider` value | Status |
|---------|------------------------|--------|
| `Dummy` | `"dummy"` | Implemented — returns `"[echo] {input}"` |
| `OpenAiCompatible` | `"openai"` / `"openai-compatible"` | Implemented — reqwest-based; configurable `api_base_url`, `model`, `temperature`, `timeout_seconds` |

---

## Note on `Channel` and `Component`

Earlier versions of the comms docs described a separate `Channel` trait with `run(self, Arc<CommsState>, CancellationToken)`. The current implementation uses the generic `Component` trait (`run(self: Box<Self>, CancellationToken)`) for all comms channels — `Arc<CommsState>` is captured at construction, not passed to `run`. There is no separate `Channel` trait in the codebase.

---

<- 93 tests source: docs/architecture/standards/runtime.md -->
# Component Runtime

**Status:** Implemented — `src/subsystems/runtime.rs`

The component runtime is the generic scaffolding shared by all subsystems. It defines how independently-runnable units are structured, spawned, and shut down.

---

## `Component` trait

```rust
pub trait Component: Send + 'static {
    fn id(&self) -> &str;
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}

pub type ComponentFuture =
    Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>>;
```

A `Component` is any independently-runnable unit owned by a subsystem: a comms channel (PTY, HTTP), an agent plugin wrapper, a tool runner, etc.

**Construction contract:** components capture all shared state (`Arc<XxxState>`, `BusHandle`, configuration) at construction time — not at `run` time. `run` takes only `self` (by value, boxed) and a `CancellationToken`. There are no mutable references to shared state after construction.

**`run` contract:**
- Called exactly once by `spawn_components`.
- Must run until `shutdown` is cancelled or the component's own work is complete.
- Must return `Err(AppError)` on failure; the error propagates to trigger sibling cancellation.
- Must be `Send + 'static` — no borrowed references that outlive the call.
- Must not block a Tokio thread — use `.await` for I/O, `tokio::task::spawn_blocking` for CPU-bound work.

`ComponentFuture` is a `Pin<Box<dyn Future>>` type alias so the trait is object-safe on stable Rust without `async-trait`.

---

## `spawn_components`

```rust
pub fn spawn_components(
    components: Vec<Box<dyn Component>>,
    shutdown: CancellationToken,
) -> SubsystemHandle
```

Takes ownership of all components for a subsystem and spawns each as an independent Tokio task. Returns a `SubsystemHandle` immediately — components run concurrently as soon as they are spawned.

### Error and cancellation behaviour (fail-fast)

1. Any component that returns `Err` cancels the shared `CancellationToken`.
2. All sibling components (and the supervisor, which shares the same token) receive the signal and stop cooperatively.
3. The internal manager task drains the remaining join handles and returns the **first error** encountered.

This ensures the system never continues running in a partially-failed state.

### Lifecycle

```
subsystem::start()
  ├─ construct Component instances (capture Arc<State>, BusHandle, config)
  ├─ spawn_components(components, shutdown_token)  → SubsystemHandle
  │   └─ per Component: tokio::spawn(component.run(shutdown_token.clone()))
  │
  │   [components run concurrently]
  │
  ├─ on any component Err: token.cancel() → siblings receive cancellation signal
  └─ SubsystemHandle::join().await → first Err, or Ok(())
```

---

## `SubsystemHandle`

```rust
pub struct SubsystemHandle {
    inner: JoinHandle<Result<(), AppError>>,
}

impl SubsystemHandle {
    pub async fn join(self) -> Result<(), AppError>;
    pub fn from_handle(handle: JoinHandle<Result<(), AppError>>) -> Self;  // escape hatch
}
```

An opaque handle to a running subsystem. `join()` blocks until all components have exited. `from_handle` is an escape hatch for subsystems that build a custom manager task outside of `spawn_components`.

---

## Intra-subsystem events

Each subsystem may maintain its own `mpsc` channel for component-to-manager signalling (e.g. "session started", "channel shutdown"). This is kept **out of the generic runtime** because the event type is subsystem-specific. Subsystems wire it up in their own `start()` function before calling `spawn_components`.

See `subsystems/comms/state.rs` (`CommsEvent`) for a reference implementation.

## Subsystems


---

<- 93 tests source: docs/architecture/subsystems/agents.md -->
# Agents Subsystem

**Status:** v0.4.1 — `Agent` trait (with `session_id`) · `AgentsState` capability boundary · `BusHandler` impl · agent dispatch · **`ChatCore` composition layer** · `SessionChatPlugin` with memory integration and session reload · session query handlers (`agents/sessions`, `agents/sessions/detail`, `agents/sessions/memory`, `agents/sessions/files`).

---

## Overview

The Agents subsystem receives agent-targeted requests from the supervisor bus and routes each message to an agent. Agent handlers are non-blocking: each handler receives ownership of `reply_tx` and resolves it in its own time — synchronously for simple agents, via `tokio::spawn` for agents that perform I/O.

---

## Agents

| Agent | Behaviour |
|-------|-----------|
| `basic_chat` | Calls `ChatCore::basic_complete` → `llm/complete` on the bus. |
| `chat` | Session-aware chat via `SessionChatPlugin`. Creates or reloads a memory session (via `session_id`), appends user/assistant turns to a Markdown transcript, and injects recent history as LLM context. Returns `session_id` in the reply. Default agent. Configured with `memory = ["basic_session"]`. |
| `news` | Calls `tools/execute` with `newsmail_aggregator/get` and returns the raw tool payload as comms content. |
| `echo` | Returns the input unchanged. Used as safety fallback when `enabled` is empty. |

---

## Routing

Agents are resolved in this priority order:

1. Explicit `{agent_id}` from the method path
2. Channel mapping: `channel_id → agent_id` in `[agents.routing]`
3. Default agent: first entry in `agents.enabled` (falls back to `echo` if `enabled` is empty)

---

## Method Grammar

- `agents` — default agent, default action
- `agents/{agent_id}` — explicit agent, default action
- `agents/{agent_id}/{action}` — explicit agent + action (`{action}` accepted but not yet differentiated)

---

## Handle Request Contract

`AgentsSubsystem` implements `BusHandler` with prefix `"agents"`. The supervisor
calls `handle_request` and returns immediately:

- **`echo`** — `EchoAgent::handle` resolves `reply_tx` inline; zero latency.
- **`basic_chat`** — `BasicChatPlugin::handle` moves `reply_tx` into a
  `tokio::spawn`ed task that calls `AgentsState::complete_via_llm`.
- **`chat`** — `SessionChatPlugin::handle` spawns a task that initialises a
  memory session on first use, appends to transcript, builds context, calls
  `ChatCore::basic_complete`, and appends the LLM reply.

---

## basic_chat Flow

```
handle_request("agents", CommsMessage { channel_id, content }, reply_tx)
  → resolve agent → "basic_chat"
  → tokio::spawn {
      bus.request("llm/complete", LlmRequest { channel_id, content }).await
        → LlmSubsystem.handle_request → DummyProvider::complete
        ← Ok(CommsMessage { channel_id, content: "[echo] {input}" })
      reply_tx.send(Ok(CommsMessage { .. }))
    }
```

---

## Agent Architecture

`Agent` is the extension trait for all agent implementations:

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

Agents are stored in a `HashMap<String, Box<dyn Agent>>` inside
`AgentsSubsystem`. Resolution order (by `id()`) maps to the routing priority
table above.

### Agent and Subagent Identities

Each registered agent is provisioned with its own cryptographic identity (`ed25519` keypair) during subsystem initialization. These identities are stored in `AgentsState::agent_identities` and persisted under `{memory_root}/agent/{agent_id}-{public_id}/`.

Agents can also spawn **subagents** — ephemeral or task-specific workers that operate under their parent's identity structure. Subagents are provisioned via `AgentsState::get_or_create_subagent(agent_id, subagent_name)`, which creates a nested identity at `{memory_root}/agent/{agent_id}-{public_id}/subagents/{subagent_name}-{public_id}/`.

> **Naming convention:** `Agent` for autonomous actors in the agents subsystem;
> `Plugin` is reserved for capability extensions in the future tools subsystem.

### Chat-family composition (`ChatCore`)

Chat-family agents (`basic_chat`, `chat`, and future variants) share logic
through composition rather than inheritance:

```
src/subsystems/agents/chat/
├── mod.rs           # feature-gated re-exports
├── core.rs          # ChatCore — shared building blocks
├── basic_chat.rs    # BasicChatPlugin (thin wrapper over ChatCore)
└── session_chat.rs  # SessionChatPlugin (ChatCore + future extensions)
```

`ChatCore` is a stateless struct providing composable methods:

```rust
impl ChatCore {
    pub async fn basic_complete(state, channel_id, content) -> BusResult;
    // Future: prompt_template(), inject_memory(), tool_dispatch(), ...
}
```

Each chat agent calls `ChatCore` methods and layers its own behaviour on top.
This avoids code duplication while allowing progressive enhancement:

```
ChatCore::basic_complete()        ← shared logic
    ↑                    ↑
BasicChatPlugin     SessionChatPlugin  (core + session/memory/tools)
                         ↑
                  AdvancedChatPlugin   (future — further extensions)
```

### Capability boundary — `AgentsState`

Agents receive `Arc<AgentsState>`, not a raw `BusHandle`. Available methods:

| Method | Description |
|--------|-------------|
| `complete_via_llm(channel_id, content)` | Forward to `llm/complete` on the bus; return `BusResult`. |
| `memory` (field) | `Arc<MemorySystem>` — create/load sessions. In builds that include `subsystem-agents`, memory is available to agents directly. |
| `agent_memory` (field) | `HashMap<String, Vec<String>>` — per-agent memory store requirements from config. |

The raw bus is private to `AgentsState`. Agents cannot call arbitrary bus
targets.

## Session queries

The agents subsystem intercepts session query bus methods before agent routing:

| Method | Payload | Response |
|--------|---------|----------|
| `agents/sessions` | `Empty` | `JsonResponse` — JSON array of all sessions (id, created_at, store_types, last_agent) |
| `agents/sessions/detail` | `SessionQuery { session_id }` | `JsonResponse` — session metadata + full transcript |
| `agents/sessions/memory` | `SessionQuery { session_id }` | `JsonResponse` — `{ session_id, content }`, where `content` is current working memory |
| `agents/sessions/files` | `SessionQuery { session_id }` | `JsonResponse` — `{ session_id, files[] }` with `name`, `size_bytes`, `modified` |

These are handled directly by `AgentsSubsystem` (not routed to individual agents).

## Initialisation

`AgentsSubsystem::new(config: AgentsConfig, bus: BusHandle, memory: Arc<MemorySystem>)` — the `BusHandle`
is injected at init and wrapped inside `AgentsState`. Built-in agents
(`EchoAgent`, `BasicChatPlugin`, `SessionChatPlugin`) are registered behind
Cargo feature gates; the `enabled` list controls which ones are reachable via
routing.

## Next phases

- Primary agents will have a stable identity value derived from key material and identity payload, modeled as `hash(prv:pub, id.md|{json})`.
- A single primary agent identity can own multiple sessions concurrently.
- A subagent is a delegated worker without its own unique persistent identity; it executes under a parent agent context.

---

## Config

```toml
[agents]
default = "chat"

[agents.routing]
# pty0 = "echo"

[agents.chat]
memory = ["basic_session"]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.default` | string | `"chat"` | Which agent handles unrouted messages. |
| `agents.routing` | map\<string,string\> | `{}` | Optional `channel_id → agent_id` routing overrides. |
| `agents.{id}.enabled` | bool | `true` | Set to `false` to disable without removing the section. |
| `agents.{id}.memory` | array\<string\> | `[]` | Memory store types this agent requires (e.g. `["basic_session"]`). |

---

<- 93 tests source: docs/architecture/subsystems/comms.md -->
# Comms Subsystem

**Status:** v0.7.0 — concurrent channel tasks · `CommsState` capability boundary · intra-subsystem event queue · `start()` returns `SubsystemHandle` · PTY runtime is conditional when stdio management is active · **HTTP channel split into `http/` module (mod, api, ui) with full `/api/` surface (health, message, sessions, session detail) · POST body parsing · session-id threading · optional UI backend delegation · Telegram channel (teloxide)**.

---

## Overview

The Comms subsystem manages all external I/O for the bot. It provides multiple transport layers (PTY, HTTP, Telegram) and hosts pluggable **channel plugins** for additional messaging services (Slack, Discord, etc.).

Channels are plugins *within* Comms. They only handle send/recv of messages — session logic and routing lives in the Agents subsystem.

---

## Components

### PTY Layer — Implemented
- Console I/O (stdin/stdout)
- Enabled by config in normal interactive runs
- Auto-disabled when supervisor stdio adapter owns stdio (virtual `/chat` route is used instead)
- Reads lines from stdin, routes each through the supervisor bus via `BusHandle::request`, prints the reply
- Multiple PTY instances are supported: each sends `"agents"` with its own `channel_id` (e.g. `"pty0"`, `"pty1"`); the embedded `oneshot` in each request carries the correct return address independently
- Ctrl-C sends a shutdown signal via `CancellationToken`; all tasks shut down gracefully
- Used for local testing and development

**Source:** `src/subsystems/comms/pty.rs`

### Virtual PTY via Supervisor Stdio Adapter — Implemented
- Lives in `src/supervisor/adapters/stdio.rs` (internal to supervisor)
- Enabled when stdio is non-interactive (management/IPC attachment)
- Performs a minimal slash protocol translation for tty lines:
  - First non-whitespace character **must** be `/`
  - Interactive mode shows a `# ` prompt before each command
  - `/chat <message>` → `BusPayload::CommsMessage { channel_id: "pty0", content }` to `agents`
  - `/health`, `/status`, `/subsys`, `/exit` → supervisor control plane commands
  - `/help` prints protocol usage
- Keeps comms behavior consistent by reusing the virtual PTY channel id (`pty0`)

### HTTP Layer — Implemented
- Single HTTP channel on a configurable bind address (default `127.0.0.1:8080`)
- Request parsing supports both GET and POST methods with Content-Length body reading
- API routes under the `/api/` prefix:
  - `GET  /api/health`              — returns enriched health JSON (bot_id, llm_provider, model, timeout, tools, session_count)
  - `POST /api/message`             — accepts `{"message", "session_id?", "mode?"}`, forwards to agents via bus with session-id threading, returns `MessageResponse` JSON with `session_id`
  - `GET  /api/sessions`            — returns session list from agents/memory subsystem
  - `GET  /api/session/{session_id}` — returns session detail (metadata + transcript) from agents/memory subsystem
- When the UI subsystem is enabled (`[ui.svui]`), non-API GET paths are delegated to the active `UiServeHandle`; the HTTP channel receives the handle at construction
- When the UI subsystem is disabled, non-API paths return 404
- Raw TCP listener with minimal request parsing (no framework dependency)

**Source:** `src/subsystems/comms/http/` (mod.rs — server loop & dispatch, api.rs — API route handlers, ui.rs — welcome page & UI delegation)

### Telegram Channel — Implemented
- Connects to Telegram Bot API via `teloxide`
- Enabled by Cargo feature `channel-telegram` and config `comms.telegram.enabled = true`
- Requires `TELEGRAM_BOT_TOKEN` env var; gracefully exits if missing
- Receives text messages, routes through `CommsState::send_message`, replies in-chat
- Shutdown via shared `CancellationToken` (`select!` on dispatcher + shutdown signal)

**Source:** `src/subsystems/comms/telegram.rs`

### Channel Plugins — Planned
- Pluggable, loadable/unloadable at runtime
- Each channel handles: receive inbound message → publish to event bus, subscribe to responses → deliver outbound message
- Planned channels: Slack, Discord, Email, SMS, WebChat

---

## Architecture

### Module layout

```
src/
  subsystems/
    runtime.rs          — Component trait, SubsystemHandle, spawn_components
    comms/
      mod.rs            — start(config, bus, shutdown, [ui_handle]) → SubsystemHandle
      state.rs          — CommsState (private bus, send_message, management_http_get, request_sessions, request_session_detail, report_event, CommsEvent, CommsReply)
      pty.rs            — PtyChannel: Component
      http/
        mod.rs          — HttpChannel: Component (server loop, connection dispatch, request parsing, response helpers)
        api.rs          — API route handlers (/api/health, /api/message, /api/sessions, /api/session/{id})
        ui.rs           — UI route handlers (root welcome page, /ui/* delegation, 404 catch-all)
      telegram.rs       — TelegramChannel: Component
    ui/
      mod.rs            — UiServe trait, UiServeHandle, start(config) → Option<UiServeHandle>
      svui.rs           — SvuiBackend: UiServe (static file serving, built-in placeholder)
```

### Capability boundary

`CommsState` is the only surface channels see. The raw `BusHandle` is private;
channels call typed methods:

| Method | Description |
|--------|-------------|
| `send_message(channel_id, content, session_id)` | Route a message to the agents subsystem; return `CommsReply` (reply string + optional session_id). |
| `management_http_get()` | Request health/status JSON from the management bus route. |
| `request_sessions()` | Request session list JSON from the agents subsystem via `agents/sessions`. |
| `request_session_detail(session_id)` | Request session detail JSON from agents via `agents/sessions/detail`. |
| `report_event(CommsEvent)` | Signal the subsystem manager (non-blocking `try_send`). |

`CommsEvent` variants: `ChannelShutdown { channel_id }`, `SessionStarted { channel_id }`.

### Concurrent channel lanes

`comms::start()` is **synchronous** — it spawns all enabled channels into a
`JoinSet` and returns a `SubsystemHandle` immediately. Channels run as
independent concurrent tasks. The manager task additionally `select!`s on an
internal `mpsc` channel for `CommsEvent`s from running channels.

If any channel exits with an error, the shared `CancellationToken` is cancelled
so sibling channels and the supervisor all shut down cooperatively.

### Channel implementation

Channels implement the generic [`Component` trait](../standards/runtime.md) from `subsystems/runtime.rs`:

```rust
pub trait Component: Send + 'static {
    fn id(&self) -> &str;
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}
```

`Arc<CommsState>` and any other shared state are captured at construction — not passed to `run`.

### Message flow (real PTY lane)

```
PTY stdin
  → CommsState::send_message("pty0", content)           [typed — no raw bus]
    → BusHandle::request("agents", CommsMessage { channel_id, content })
      → SupervisorBus::rx (mpsc, bounded 64)
        → supervisor: HashMap dispatch (prefix = "agents")
          → AgentsSubsystem::handle_request   ← supervisor returns immediately
            → resolve plugin → basic_chat
              → tokio::spawn {
                  AgentsState::complete_via_llm(channel_id, content)  [typed]
                    → BusHandle::request("llm/complete", LlmRequest { .. })
                      → supervisor: dispatch (prefix = "llm")
                        → LlmSubsystem::handle_request
                          → tokio::spawn {
                              DummyProvider::complete(content)
                                → Ok("[echo] {content}")
                              reply_tx.send(Ok(CommsMessage { .. }))
                            }
                  reply_tx.send(Ok(CommsMessage { .. }))
                }
  ← Ok(reply)
  → pty prints reply to stdout
PTY stdout
```

For `echo`: `reply_tx` resolved inline, no spawn.
Ctrl-C
  → tokio::signal::ctrl_c()
    → CancellationToken::cancel()
      → pty::run select! branch fires → prints shutdown notice → returns Ok(())
      → supervisor::run select! branch fires → returns
  → main joins both tasks → process exits cleanly
```

---

## Config

```toml
[comms.pty]
# Real PTY lane for interactive stdin/stdout.
enabled = true

[comms.http]
# HTTP channel — API under /api/, UI on other paths when [ui.svui] enabled.
enabled = true
bind = "127.0.0.1:8080"

[comms.telegram]
# Telegram channel — requires TELEGRAM_BOT_TOKEN env var.
enabled = false
```

When stdio management is connected, Comms skips real PTY startup and management `/chat` acts as a virtual PTY stream.

---

<- 93 tests source: docs/architecture/subsystems/cron.md -->
# Cron Subsystem

**Status:** Implemented — `src/subsystems/cron/`  
**Feature:** `subsystem-cron`  
**Bus prefix:** `cron`

---

## Overview

The cron subsystem provides timer-based event scheduling. Other subsystems schedule events by sending bus requests; the cron service emits those events as bus notifications at the specified times. This keeps all inter-subsystem communication on the bus (star topology preserved).

---

## Architecture

### Module layout

```
src/subsystems/cron/
├── mod.rs       CronSubsystem — BusHandler, owns mpsc::Sender to bg task
└── service.rs   CronService   — background tokio task, priority queue, timer loop
```

### Internal communication

`CronSubsystem` (the handler) communicates with `CronService` (the background task) via an internal `mpsc` channel using `CronCommand`:

```rust
enum CronCommand {
    Schedule { id, target_method, payload_json, spec, reply },
    Cancel { id, reply },
    List { reply },
}
```

### Timer implementation

- **Priority queue:** `BTreeMap<Instant, ScheduleEntry>` — entries sorted by next fire time.
- **Secondary index:** `HashMap<String, Instant>` — schedule_id → key lookup for O(1) cancel.
- **Sleep strategy:** `tokio::time::sleep_until(next_deadline)` — no polling, no tick interval. When the queue is empty, the sleep branch is disabled via `std::future::pending()`.
- **Collision handling:** If two entries share the same `Instant`, the new entry is nudged forward by 1ns (`insert_unique`).

### Run loop

```rust
loop {
    tokio::select! {
        _ = shutdown.cancelled() => break,
        Some(cmd) = cmd_rx.recv() => { /* Schedule / Cancel / List */ },
        _ = sleep_until(next) => {
            // Fire notification via bus
            // Re-enqueue if Interval, remove if Once
        }
    }
}
```

---

## Bus Methods

### `cron/schedule` — Request

Schedule a new timed event.

**Payload:** `BusPayload::CronSchedule`

| Field | Type | Description |
|-------|------|-------------|
| `target_method` | `String` | Bus method to emit when the timer fires (e.g. `"agents/daily-digest"`) |
| `payload_json` | `String` | Serialized `BusPayload` to include in the notification |
| `spec` | `CronScheduleSpec` | Timing specification |

**`CronScheduleSpec` variants:**

| Variant | Fields | Behaviour |
|---------|--------|-----------|
| `Once` | `at_unix_ms: u64` | Fire once at the given UTC timestamp (ms), then remove |
| `Interval` | `every_secs: u64` | Fire repeatedly at the given interval from now |

**Reply:** `BusPayload::CronScheduleResult { schedule_id: String }`

### `cron/cancel` — Request

Cancel an active schedule.

**Payload:** `BusPayload::CronCancel { schedule_id: String }`

**Reply:** `BusPayload::Empty` on success, or `ERR_BAD_REQUEST` if the schedule_id was not found.

### `cron/list` — Request

List all active schedules.

**Payload:** `BusPayload::CronList`

**Reply:** `BusPayload::CronListResult { entries: Vec<CronEntryInfo> }`

| Field | Type | Description |
|-------|------|-------------|
| `schedule_id` | `String` | Unique identifier |
| `target_method` | `String` | Method that will be notified |
| `spec` | `CronScheduleSpec` | Original timing spec |
| `next_fire_unix_ms` | `u64` | Next fire time (UTC ms) |

---

## Event emission

When a timer fires, the cron service calls:

```rust
bus.notify(entry.target_method, BusPayload::Text(entry.payload_json))
```

The supervisor routes this notification by prefix to the appropriate subsystem. No special handling is needed — it looks like any other bus notification.

---

## Management integration

The management subsystem (`manage/http/get`) queries `cron/list` via the bus and includes `cron_active` (count) and `cron_schedules` (array) in the `main_process.details` section of the health JSON response.

The UI `StatusView` displays active cron schedules in the main process card with target method, spec type, and next fire countdown.

---

## Tests

4 unit tests in `service.rs` (use `tokio::test` with `start_paused = true`):

| Test | Validates |
|------|-----------|
| `schedule_and_list` | Schedule inserts correctly, List returns entry with correct fields |
| `cancel_success_and_miss` | Cancel removes entry, cancelling unknown ID returns error |
| `interval_fires_notification` | Interval timer fires notification on the bus at the right time |
| `once_fires_and_is_removed` | Once timer fires and is automatically removed from the queue |

---

## Design rationale

Approach A (full BusHandler) was chosen over side-channel or hybrid approaches. See [cron-service-design.md](../../../notes/implementation/cron-service-design.md) for the full comparison.

Key reasons:
- **Bus-native** — all scheduling goes through the bus, consistent with star topology
- **Discoverable** — any subsystem with a `BusHandle` can schedule events
- **Introspectable** — `cron/list` is available to management and HTTP adapters
- **Future-proof** — runtime-loaded plugins already have `BusHandle`, no extra wiring needed

---

<- 93 tests source: docs/architecture/subsystems/llm.md -->
# LLM Subsystem

**Status:** v0.5.0 — `LlmResponse` + `LlmUsage` + `ModelRates` · token usage deserialized from OpenAI wire format (incl. cached tokens) · cost computed per-call · per-session spend accumulated to `spend.json` · pricing rates in config.

---

## Overview

The LLM subsystem is a bus participant that handles all `llm/*` requests. It owns the configured provider and resolves each request asynchronously — the supervisor loop is never blocked on provider I/O.

The Agents subsystem uses the bus to call `llm/complete` rather than holding a direct reference to the provider. Any future subsystem can do the same.

---

## Responsibilities

- Receive `llm/complete` requests via the supervisor bus
- Forward the prompt to the configured `LlmProvider`
- Deserialize token usage from the provider response
- Compute per-call cost using configured pricing rates and log it
- Return the reply as `BusPayload::CommsMessage` (preserving `channel_id` and `usage`)
- Spawn one task per request so the supervisor loop is non-blocking

---

## Module Layout

```
src/
  llm/
    mod.rs              LlmProvider enum · LlmResponse · LlmUsage · ModelRates
    providers/
      mod.rs            build(name) factory function
      dummy.rs          DummyProvider — returns "[echo] {input}", usage: None
      openai_compatible.rs  reqwest HTTP client; deserializes usage + cached tokens
  subsystems/
    llm/
      mod.rs            LlmSubsystem — handle_request, tokio::spawn per call
```

---

## Types

### `LlmUsage`
```rust
pub struct LlmUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,   // from prompt_tokens_details.cached_tokens
}
```

### `ModelRates`
```rust
pub struct ModelRates {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cached_input_per_million_usd: f64,
}
```

### `LlmResponse`
```rust
pub struct LlmResponse {
    pub text: String,
    pub usage: Option<LlmUsage>,   // None for DummyProvider and keyless endpoints
}
```

`LlmUsage::cost_usd(rates: &ModelRates) -> f64` applies per-million-token pricing.

---

## Provider Abstraction

`LlmProvider` is an enum over concrete implementations. Enum dispatch avoids `dyn` trait objects and the `async-trait` dependency. Adding a backend = new module + new variant + new `complete` arm + new `build()` match arm.

```rust
pub enum LlmProvider {
    Dummy(DummyProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),
}

impl LlmProvider {
    pub async fn complete(&self, content: &str) -> Result<LlmResponse, ProviderError>;
}
```

---

## Bus Protocol

**Request method:** `"llm/complete"`

**Request payload:** `BusPayload::LlmRequest { channel_id: String, content: String }`

`channel_id` is threaded through so the reply can be associated with the originating channel without extra bookkeeping by the caller.

**Reply payload:** `BusPayload::CommsMessage { channel_id, content: reply, session_id: None, usage: Option<LlmUsage> }`

`usage` is `None` when the provider does not report token counts.

---

## Request Lifecycle

```
supervisor receives Request { method: "llm/complete", payload: LlmRequest { .. }, reply_tx }
  → llm.handle_request(method, payload, reply_tx)       // supervisor returns immediately
    → tokio::spawn {
        provider.complete(&content).await
          → Ok(LlmResponse { text, usage })
              → log input_tokens, output_tokens, cached_tokens, cost_usd  [DEBUG]
              → reply_tx.send(Ok(CommsMessage { channel_id, content: text, usage }))
          → Err(e)
              → reply_tx.send(Err(BusError { .. }))
      }
```

---

## Spend Accumulation

After each LLM call in `SessionChatPlugin`, if the response carries `usage` and the session is disk-backed, `SessionHandle::accumulate_spend(usage, &state.llm_rates)` is called. This reads/updates/writes `sessions/{id}/spend.json`:

```json
{
  "total_input_tokens": 1240,
  "total_output_tokens": 380,
  "total_cached_tokens": 0,
  "total_cost_usd": 0.000694,
  "last_updated": "2026-02-21T10:59:42Z"
}
```

`AgentsState.llm_rates` is populated from config at startup via `AgentsSubsystem::with_llm_rates(rates)`.

---

## Current Providers

`DummyProvider` requires no API key. It returns `"[echo] {input}"` with `usage: None`.

`OpenAiCompatibleProvider` uses `[llm.openai]` settings plus `LLM_API_KEY` from env/.env. It deserializes the OpenAI `usage` object including `prompt_tokens_details.cached_tokens`.

---

## Configuration

```toml
[llm]
default = "openai"

[llm.openai]
api_base_url = "https://api.openai.com/v1/chat/completions"
model = "gpt-5-nano"
temperature = 0.2
timeout_seconds = 60
# Token pricing — USD per 1 million tokens. Defaults to 0.0 when not set.
input_per_million_usd = 1.10
output_per_million_usd = 4.40
cached_input_per_million_usd = 0.275
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active provider. Supported: `"dummy"`, `"openai"`. |
| `llm.openai.api_base_url` | string | OpenAI endpoint | Chat completions URL. Set to a local server for Ollama / LM Studio. |
| `llm.openai.model` | string | `"gpt-4o-mini"` | Model name sent in the request body. |
| `llm.openai.temperature` | float | `0.2` | Sampling temperature (silently omitted for `gpt-5` family). |
| `llm.openai.timeout_seconds` | integer | `60` | Per-request HTTP timeout. |
| `llm.openai.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.openai.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.openai.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |

Pricing fields default to `0.0` so cost is silently omitted rather than wrong when not configured.

---

## Adding a Real Provider

1. Create `src/llm/providers/{name}.rs` — implement `async fn complete(&self, content: &str) -> Result<LlmResponse, ProviderError>`.
2. Add a variant to `LlmProvider` in `src/llm/mod.rs`.
3. Add a match arm to `LlmProvider::complete`.
4. Add a match arm to `providers::build(name)` in `src/llm/providers/mod.rs`.
5. Update `[llm] default = "{name}"` in `config/default.toml`.
6. Pass secrets via environment variable or `.env` (never in config files).

---

## Planned Provider Support

| Provider | Auth | Notes |
|----------|------|-------|
| OpenAI-compatible | `LLM_API_KEY` | Implemented (`default = "openai"`) |
| Dummy | none | Implemented (`default = "dummy"`) |
| Anthropic | `ANTHROPIC_API_KEY` | Planned |

---

<- 93 tests source: docs/architecture/subsystems/memory.md -->
# Memory Subsystem

**Status:** v0.5.0 — typed value model (`PrimaryValue`, `Obj`, `Value`, `Doc`, `Block`, `Collection`) · `Store` struct (labeled collection map) · `TmpStore` (ephemeral in-process store) · `SessionStore` trait · `BasicSessionStore` · `SessionRw` data ops layer · `SessionHandle` with `tmp_doc`/`tmp_block` accessors · **`SessionSpend` — per-session token and cost tracking in `spend.json`**.

---

## Overview

The Memory subsystem owns all session data for the bot instance.  It provides:

- A **typed value model** for structured, hashable agent memory.
- Two concrete **collection types** (`Doc` for scalars, `Block` for rich payloads).
- A **`TmpStore`** — ephemeral in-process storage backed by the new `Store` struct, ideal for scratch pads and default sessions.
- A **`BasicSessionStore`** — disk-backed JSON + Markdown transcript store for durable sessions.
- A **`SessionHandle`** — async-safe handle agents use to read and write session state, with direct typed accessors for `TmpStore` sessions.

Memory is **not bus-mediated** — agents receive a `SessionHandle` directly from `AgentsState.memory` rather than routing through bus messages. `subsystem-memory` remains a Cargo feature at product level; when agents are enabled, memory is available directly in agent code.

---

## Architecture

```
MemorySystem (owns session index + store factory)
    │
    ├── create_session(store_types, agent_id) → SessionHandle
    ├── load_session(session_id, agent_id)   → SessionHandle
    └── create_tmp_store()                   → Arc<TmpStore> (standalone)
            │
            └── SessionHandle (Arc-wrapped, cloneable, async-safe)
                    │
                    ├── stores: Vec<Arc<dyn SessionStore>>    ← kv / transcript I/O
                    └── tmp_store: Option<Arc<TmpStore>>      ← typed Doc/Block access
```

### Key types

| Type | Location | Role |
|------|----------|------|
| `MemorySystem` | `memory/mod.rs` | Session lifecycle: create, load, list. Maintains `sessions.json` index. |
| `SessionStore` | `memory/store.rs` | Trait for pluggable session backends. |
| `Store` | `memory/store.rs` | In-process `RwLock<HashMap<String, Collection>>` — the core collection map. |
| `SessionRw` | `memory/rw.rs` | Shared session read/write orchestration layer (kv, transcript, file listing, tmp collections). |
| `SessionHandle` | `memory/handle.rs` | Thin facade that delegates all data I/O to `SessionRw`; also owns spend accumulation. |
| `SessionInfo` | `memory/mod.rs` | Session metadata persisted in `sessions.json`; includes an optional `spend` summary. |
| `SessionSpend` | `memory/mod.rs` | Aggregate token counts and cumulative cost; persisted as `spend.json`. |
| `BasicSessionStore` | `memory/stores/basic_session.rs` | Capped JSON k-v + capped Markdown transcript, disk-backed. |
| `TmpStore` | `memory/stores/tmp.rs` | Ephemeral in-process store wrapping a `Store`. Implements `SessionStore`. |
| `Doc` | `memory/collections.rs` | String-keyed map of `PrimaryValue` scalars. |
| `Block` | `memory/collections.rs` | String-keyed map of `Value` (scalars + binary `Obj`). |
| `Collection` | `memory/collections.rs` | Enum: `Doc`, `Block`, and stubs for future variants. |
| `PrimaryValue` | `memory/types.rs` | `Bool` · `Int` · `Float` · `Str` — hashable, equatable. |
| `Value` | `memory/types.rs` | `Primary(PrimaryValue)` or `Obj(Obj)`. |
| `Obj` | `memory/types.rs` | Binary payload with `HashMap<String, String>` metadata sidecar. |

---

## Type System

### `PrimaryValue`

Scalar values suitable for indexing, hashing, and equality:

```rust
enum PrimaryValue { Bool(bool), Int(i64), Float(f64), Str(String) }
```

`Float` equality and hashing use bit patterns (`f64::to_bits()`).  `From` impls for all primitive types.

### `Obj`

Binary payload with a string-keyed metadata sidecar (MIME type, content hash, etc.):

```rust
struct Obj { pub data: Vec<u8>, pub metadata: HashMap<String, String> }
```

### `Value`

Union type for `Block` entries — either a scalar or an object:

```rust
enum Value { Primary(PrimaryValue), Obj(Obj) }
```

### `Doc` and `Block`

| | `Doc` | `Block` |
|--|-------|---------|
| Entry type | `PrimaryValue` | `Value` |
| Use for | Config, extracted facts, session metadata | Blobs, embeddings, intermediate results |
| Methods | `get`, `set`, `delete`, `keys`, `len`, `is_empty` | same |

### `Collection`

Enum wrapping all collection types.  `Doc` and `Block` are fully implemented.  `Set`, `List`, `Vec`, `Tuple`, `Tensor` are **stubs** that compile but `unimplemented!()` on access — reserved namespace, not silently wrong.

```rust
enum Collection { Doc(Doc), Block(Block), Set(()), List(()), Vec(()), Tuple(()), Tensor(()) }
```

Use `as_doc()` / `as_doc_mut()` / `into_doc()` / `as_block()` / ... to downcast.

---

## Store Abstractions

### `SessionStore` trait

Pluggable backend for session-scoped I/O.  All methods are default-no-op (return `AppError::Memory("unsupported")`); implementations override only what they support.

```rust
pub trait SessionStore: Send + Sync {
    fn store_type(&self) -> &str;
    fn init(&self, session_dir: &Path) -> Result<(), AppError>;
    fn kv_get(&self, session_dir: &Path, key: &str)   -> Result<Option<String>, AppError>;
    fn kv_set(&self, session_dir: &Path, key: &str, value: &str) -> Result<(), AppError>;
    fn kv_delete(&self, session_dir: &Path, key: &str) -> Result<bool, AppError>;
    fn transcript_append(&self, ...)  -> Result<(), AppError>;
    fn transcript_read_last(&self, ...) -> Result<Vec<TranscriptEntry>, AppError>;
}
```

### `Store` struct

An in-process labeled collection map, safe for concurrent reads:

```rust
let store = Store::new();
store.insert_collection("meta".into(), Collection::Doc(Doc::default()))?;
let col = store.get_collection("meta")?.unwrap(); // returns a clone
```

Operations: `get_collection`, `insert_collection`, `remove_collection`, `labels`, `len`, `is_empty`.

---

## TmpStore

`TmpStore` wraps a `Store` and provides two usage modes:

### Standalone (agent scratch pad)

```rust
let ts: Arc<TmpStore> = memory.create_tmp_store();
let mut doc = ts.doc()?;
doc.set("status".into(), PrimaryValue::from("active"));
ts.set_doc(doc)?;
```

`create_tmp_store()` always returns a fresh, independent store not tracked in the session index.

### Session-backed

When a session is created with `store_type = "tmp"`, `TmpStore` implements `SessionStore` using per-session namespaced collection labels (`"{session_dir}:doc"`, `"{session_dir}:block"`).  `kv_get`/`kv_set`/`kv_delete` delegate to the `"doc"` collection, serialising values as `PrimaryValue::Str`.

`init()` is a no-op — no files are written to disk.

---

## Session Lifecycle

Sessions are **bot-scoped** — any agent with the session ID can access it.

1. **Create:** `MemorySystem::create_session(&["tmp"], "chat")` — or `&["basic_session"]` for disk persistence.
2. **Use:** Returns a `SessionHandle` for k-v and transcript operations.
3. **Load:** `MemorySystem::load_session(session_id, "chat")` re-opens an existing session.
4. **Tmp sessions** can be reloaded within the same process run (data is in-process); they do not survive restart.

Session IDs are UUIDv7 (time-ordered).  The `sessions.json` index tracks all sessions including tmp ones.

## Next phases

- Introduce `AgentHandle` for agent-scoped memory roots (`memory/agents/{agent_id}/`) while keeping session handles for conversation-scoped state.
- Agent identity model for primary agents: `hash(prv:pub, id.md|{json})`.
- Allow multiple sessions per primary agent identity.
- Treat subagents as delegated workers without a unique persistent identity.

---

## Data Layout (disk-backed sessions)

```
{identity_dir}/
└── memory/
    ├── sessions.json              session index (includes spend summary)
    └── sessions/
        └── {uuid}/                only created for non-tmp sessions
            ├── kv.json            capped key-value store
            ├── transcript.md      capped Markdown transcript
            └── spend.json         aggregate token and cost totals (created on first LLM turn)
```

### `spend.json` shape

```json
{
  "total_input_tokens": 1240,
  "total_output_tokens": 380,
  "total_cached_tokens": 0,
  "total_cost_usd": 0.000694,
  "last_updated": "2026-02-21T10:59:42Z"
}
```

The file is created on the first LLM turn that carries token usage. `sessions.json` mirrors the latest totals in `SessionInfo.spend` so aggregate spend can be queried without opening individual sidecar files.

---

## SessionHandle (async API)

String-based k-v and transcript operations (work for both `basic_session` and `tmp` sessions):

```rust
pub async fn kv_get(&self, key: &str)               -> Result<Option<String>, AppError>;
pub async fn kv_set(&self, key: &str, value: &str)  -> Result<(), AppError>;
pub async fn kv_delete(&self, key: &str)             -> Result<bool, AppError>;
pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError>;
pub async fn transcript_read_last(&self, n: usize)  -> Result<Vec<TranscriptEntry>, AppError>;
pub async fn working_memory_read(&self)              -> Result<String, AppError>;
pub async fn list_files(&self)                       -> Result<Vec<SessionFileInfo>, AppError>;
```

Spend accumulation (disk-backed sessions only; no-op for tmp sessions without a directory):

```rust
pub async fn accumulate_spend(
    &self,
    usage: &LlmUsage,
    rates: &ModelRates,
) -> Result<SessionSpend, AppError>;
```

Reads `spend.json`, adds the new token counts, recomputes the incremental cost, writes back, and returns the updated totals.

Typed accessors for `tmp` sessions (synchronous — no file I/O):

```rust
pub fn tmp_doc(&self)                  -> Result<Doc, AppError>;    // snapshot clone
pub fn tmp_block(&self)                -> Result<Block, AppError>;  // snapshot clone
pub fn set_tmp_doc(&self, doc: Doc)    -> Result<(), AppError>;     // write back
pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError>;  // write back
```

These return `Err` for sessions without a `TmpStore` (i.e. `basic_session` sessions).

---

## Agent Integration

`SessionChatPlugin` demonstrates memory integration:

1. On first message, creates a session via `state.memory.create_session(store_types, "chat")`.
2. Appends user input as a `"user"` transcript entry.
3. Reads the last 20 transcript entries and injects them as LLM context.
4. Appends the LLM response as an `"assistant"` transcript entry.
5. If the response carries token `usage`, calls `handle.accumulate_spend(usage, &state.llm_rates)` to update `spend.json`.
6. The session handle is cached in `Arc<Mutex<Option<SessionHandle>>>` for reuse.

`state.llm_rates` is populated at startup by `AgentsSubsystem::with_llm_rates(rates)` using pricing values from `[llm.openai]` config.

```toml
[agents.chat]
memory = ["tmp"]   # use ephemeral in-process storage instead
```

---

## Config

```toml
[memory.basic_session]
# kv_cap = 200         # max key-value entries per session (default: 200)
# transcript_cap = 500 # max transcript entries per session (default: 500)

[agents.chat]
memory = ["basic_session"]  # store types this agent uses
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum k-v entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |
| `agents.{id}.memory` | array\<string\> | `[]` | Store types (`"basic_session"` or `"tmp"`). |

Memory is always compiled — there is no Cargo feature gate.

---

## Future

- **Default store type "tmp":** when `agents.{id}.memory` is empty, automatically use `"tmp"` instead of returning an error.
- **Observation store:** structured facts, summaries, reflections (JSONL or SQLite).
- **Cross-session search:** full-text or embedding-based retrieval across sessions.
- **Session expiry:** TTL-based cleanup of old sessions.
- **Mirror spend → sessions.json:** after `accumulate_spend`, update `SessionInfo.spend` in the index so listings include live totals without opening sidecars.


---

<- 93 tests source: docs/architecture/subsystems/tools.md -->
# Tools Subsystem

**Status:** Implemented (MVP) — `src/subsystems/tools/`

---

## Overview

The Tools subsystem owns tool execution on behalf of agents. Agents call the tools subsystem through the supervisor bus (no direct Gmail API access from agent plugins).

---

## Responsibilities

- Execute tools in response to `BusPayload::ToolRequest`
- Return structured results via `BusPayload::ToolResponse`
- Keep external integration logic (OAuth/API calls) in tool modules

---

## Tool Types

- **Built-in tools (current):** `gmail/read_latest`, `newsmail_aggregator/get`
- **Future:** additional built-ins and optional runtime-loaded external tools

---

## Current Gmail Tool

- Module: `src/subsystems/tools/gmail.rs`
- Action: `read_latest`
- OAuth: Desktop loopback auth (`GOOGLE_CLIENT_ID`, optional `GOOGLE_CLIENT_SECRET`)
- Token cache: `config/gmail_token.json`
- Optional redirect override: `GOOGLE_REDIRECT_URI` (default `http://127.0.0.1:8080/oauth2/callback`)

## Newsmail Aggregator Tool

- Module: `src/subsystems/tools/newsmail_aggregator.rs`
- Actions: `get`, `healthcheck`
- Transport: `tools/execute` (same as all tools)
- Uses Gmail core integration from `src/subsystems/tools/gmail.rs` (no duplicated OAuth/API stack)
- Optional LLM inputs: `label` (string or array of Gmail label IDs), `n_last`, `t_interval` (preferred), `tsec_last` (legacy), `q` (extra Gmail search terms)
- Config default: `label_ids = ["INBOX"]` — used when the LLM provides no label override
- `healthcheck` performs a minimal fetch (`maxResults=1`) with `labelIds={defaults}` and `q=newsletter`

---

## Message Protocol

- Request method: `tools/execute`
- Request payload: `ToolRequest { tool, action, args_json, channel_id, session_id }`
- Response payload: `ToolResponse { tool, action, ok, data_json, error }`

## Agent Integration

- Gmail agent plugin: `src/subsystems/agents/gmail.rs`
- Agent bus method: `agents/gmail/read`
- Flow: `agents/gmail/read` → `tools/execute` (`gmail/read_latest`) → agent formats summary reply
- News agent plugin: `src/subsystems/agents/news.rs`
- Agent bus methods: `agents/news` (default handle), `agents/news/read`
- Flow: `agents/news` → `tools/execute` (`newsmail_aggregator/get`) → agent returns raw tool payload

---

<- 93 tests source: docs/architecture/subsystems/ui.md -->
# UI Subsystem

**Status:** v0.1.0 — `UiServe` trait · `svui` backend · static file serving with SPA fallback · built-in placeholder page.

---

## Overview

The UI subsystem provides display-oriented interface backends. Unlike comms or agents, it does **not** run independent tasks. Instead it constructs a `UiServeHandle` — a trait-object that the HTTP channel calls per-request to serve static assets or rendered pages.

Each backend (e.g. *svui*) implements `UiServe` and is selected at startup based on config. Only one backend is active at a time.

---

## Backends

### svui — Implemented

Svelte-based web UI backend. Serves static files from a build directory, or a built-in placeholder page when no build is available.

**Behaviour:**

| Condition | Result |
|-----------|--------|
| `static_dir` configured and exists | Files served from disk; SPA fallback to `index.html` for non-asset paths |
| `static_dir` absent or missing | Built-in placeholder HTML page served for `/` and `/index.html` |
| Path contains `..` | Rejected with 400 Bad Request |

MIME types are inferred from file extensions (html, css, js, svg, png, woff2, wasm, etc.).

**Source:** `src/subsystems/ui/svui.rs`

---

## Architecture

### Module layout

```
src/
  subsystems/
    ui/
      mod.rs    — UiServe trait, UiServeHandle type, start(config) → Option<UiServeHandle>
      svui.rs   — SvuiBackend: UiServe
```

### Integration with HTTP channel

The UI subsystem is a **provider**, not a runtime component. `ui::start()` builds the active backend and returns an `Arc<dyn UiServe>`. This handle is passed to `comms::start()`, which injects it into the `HttpChannel`.

The HTTP channel dispatches requests as follows:

```
GET /api/health  → management bus (API route)
GET /api/*       → future API routes
GET /anything    → ui_handle.serve("/anything")  → static file or SPA fallback
GET /anything    → 404 (if no UI backend or serve returns None)
```

When the `subsystem-ui` feature is disabled at compile time, the HTTP channel has no UI handle and all non-API paths return 404.

---

## Config

```toml
[ui.svui]
enabled = true
# static_dir = "ui/build"
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Whether the svui backend is loaded. |
| `static_dir` | (none) | Path to the static build directory. If absent, built-in placeholder is served. |

---

## Features

| Feature | Requires | Description |
|---------|----------|-------------|
| `subsystem-ui` | — | UI subsystem scaffolding. |
| `ui-svui` | `subsystem-ui` | Svelte UI backend. |

Both are included in the default feature set.

---

# Development


---

<- 93 tests source: docs/development/contributing.md -->
# Contributing

## Prerequisites

- Rust toolchain 1.80+ (`rustup`)
- `cargo` (bundled with Rust)

## Workflow

```bash
# Check compilation (fast)
cargo check

# Run tests
cargo test

# Build
cargo build

# Run
cargo run
```

Always run `cargo check` and `cargo test` before committing changes.

## Code Style

- One concern per module — `config.rs` only loads config, `identity.rs` only manages identity
- `main.rs` is an orchestrator only — no business logic
- Errors via `thiserror` — no `unwrap()` in non-test code, no `Box<dyn Error>` in public APIs
- Logging via `tracing` macros (`info!`, `debug!`, `warn!`, `error!`) — not `println!`
- PTY user-facing console I/O is the exception and may write to stdout/stderr directly; keep it separate from diagnostic logs

## Adding a New Module

1. Create `src/{name}.rs`
2. Declare it in `main.rs`: `mod {name};`
3. Define a typed error variant in `error.rs` if needed
4. Add unit tests in a `#[cfg(test)]` block at the bottom of the module
5. Use `tempfile::TempDir` for any filesystem tests

## Subsystem Development (Future)

Each subsystem will live in `src/subsystems/{name}/` with its own `mod.rs`. See the [architecture overview](../architecture/overview.md) for the planned structure.

## Documentation

Update the relevant doc in `docs/` when making significant changes to a module or subsystem. Keep `docs/architecture/overview.md` current with module status.

---

<- 93 tests source: docs/development/gpui.md -->
# GPUI Desktop Client — Development Guide

The optional native desktop client (`araliya-gpui`) is built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), Zed's GPU-accelerated UI framework. It runs as a separate binary alongside the main bot daemon and communicates with it over the HTTP API.

## System Dependencies (Linux)

GPUI on Linux links against several native system libraries that are **not** bundled by Cargo and must be present on the build host.

### XCB — X protocol C-language Binding

XCB is the low-level C library for the X Window System protocol. It replaces the older Xlib with a smaller, asynchronous interface. GPUI uses it to create windows and handle X events on Linux.

**Required package:** `libxcb-dev` (Debian/Ubuntu) — provides `libxcb.so` and headers.

### XKB — X Keyboard Extension

XKB (X Keyboard Extension) is the X11 subsystem that handles keyboard layouts, key maps, and modifier state (Shift, Ctrl, etc.). Two libraries are needed:

- **libxkbcommon** — a standalone XKB keymap compiler and state machine, used without any X connection (also works on Wayland).
- **libxkbcommon-x11** — extends libxkbcommon to load keymaps directly from an X server via XCB.

**Required packages:** `libxkbcommon-dev`, `libxkbcommon-x11-dev` (Debian/Ubuntu).

### Install all at once

```bash
# Debian / Ubuntu / Mint
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev

# Fedora / RHEL
sudo dnf install -y libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel

# Arch Linux
sudo pacman -S libxcb libxkbcommon libxkbcommon-x11
```

These are development (`-dev`) packages — they provide the `.so` symlinks and headers that the linker needs at build time. The runtime `.so` files are almost always already present on any desktop Linux system.

## Feature Flag

The GPUI binary is gated behind the `ui-gpui` Cargo feature:

```bash
# Check only (fast)
cargo check --bin araliya-gpui --features ui-gpui

# Build
cargo build --bin araliya-gpui --features ui-gpui

# Run
cargo run --bin araliya-gpui --features ui-gpui
```

## Running

The GPUI client connects to the bot's HTTP API. Start the bot daemon first:

```bash
# Terminal 1 — bot API (default: http://127.0.0.1:8080)
cargo run

# Terminal 2 — desktop client
cargo run --bin araliya-gpui --features ui-gpui
```

The client target URL defaults to `http://127.0.0.1:8080`. See `config/default.toml` for the relevant API address.

## Architecture Notes

- `gpui`'s `Application::run()` takes over the **main thread**, so the tokio runtime runs on a background `std::thread`.
- `Config` and `Identity` are loaded before the runtime starts and passed to the UI as a `UiSnapshot` (owned, no lifetimes).
- A shared `Arc<AtomicU8>` carries `BotStatus` so the status panel reflects the bot's lifecycle without holding locks.
- Source lives in `src/bin/araliya-gpui/`:
	- `main.rs` — app bootstrap and window wiring
	- `components.rs` — UI shell and panel rendering
	- `state.rs` — view/layout/session state
	- `api.rs` — HTTP API client + DTOs

## Current UI Framework (PRD-aligned basic shell)

The GPUI client now uses a basic shell mirroring the UI/UX PRD framework:

- **Zone A (Activity rail):** section switcher for `Chat`, `Memory`, `Tools`, `Status`, `Settings`, `Docs`
- **Zone B (Header):** app identity, active section context, health summary, panel toggles
- **Zone C (Panel row):**
	- Left panel: sessions list
	- Main panel: section content (chat and status implemented; others scaffolded)
	- Right panel: optional context panel scaffold
- **Zone D (Bottom bar):** compact session/message/mode summary

This keeps layout extensibility in place while preserving existing API-backed chat and status behavior.

### Canvas-first surface mode

The main panel now supports two runtime surfaces:

- **Canvas:** renders a GPUI polygon scene (`canvas` + `Path`) as the primary interaction surface.
- **Shell:** renders the existing section-based content panels.

Behavior:

- Header action toggles between `Canvas` and `Shell`.
- In `Canvas`, footer buttons toggle Sessions/Context panels and move focus to `Chat`/`Status` sections.
- Canvas mode keeps the existing activity rail, header, and bottom status bar so shell controls stay available.

Implementation locations:

- `src/bin/araliya-gpui/canvas_scene.rs` — geometry + hit-test helpers
- `src/bin/araliya-gpui/components.rs` — canvas rendering and interaction wiring
- `src/bin/araliya-gpui/state.rs` — `SurfaceMode` state (`Canvas`/`Shell`)

## Responsive layout behavior

The GPUI shell now adapts to window width using a single responsive shell model:

- **Desktop** (`>= 1200px`): inline left sessions panel and inline right context panel.
- **Tablet** (`>= 860px` and `< 1200px`): compact shell with activity rail always visible; side panels open as focused drawers.
- **Compact** (`< 860px`): same drawer behavior as tablet with tighter content widths.

Current interaction model:

- Activity rail is always visible for section switching.
- Header toggles control Sessions and Context panel visibility.
- In tablet/compact modes, opening a side panel switches the center area into that panel view with a close action.

Layout preferences are persisted between runs in:

- `~/.config/araliya-bot/gpui-layout.json`

Persisted fields include:

- left/right panel open state
- left/right panel widths
- ISO-8601 `updated_at`

See [notes/gpui-plan.md](../../../notes/gpui-plan.md) for the original design notes.

---

<- 93 tests source: docs/development/testing.md -->
# Testing

## Running Tests

```bash
cargo test
```

## Test Coverage (v0.1)

| Module | Tests | Coverage |
|--------|-------|---------|
| `error.rs` | 4 | All variants: display, trait impl, IO conversion |
| `logger.rs` | 3 | Valid levels, invalid levels, init succeeds |
| `config.rs` | 6 | Parse, tilde expansion, absolute/relative paths, missing file, env overrides |
| `identity.rs` | 6 | bot_id format, unique keygen, save/load round-trip, dir creation, idempotency, file permissions |

**Total: 22 tests**

## Filesystem Tests

All tests that touch the filesystem use `tempfile::TempDir`. Tests never write to `~/.araliya` or any shared path. Each test gets an isolated temporary directory that is cleaned up automatically on drop.

```rust
use tempfile::TempDir;

let tmp = TempDir::new().unwrap();
let cfg = Config { work_dir: tmp.path().to_path_buf(), .. };
let identity = identity::setup(&cfg).unwrap();
// tmp cleaned up when it goes out of scope
```

## Env Var Tests

Config tests that need to verify override behaviour pass values directly into `load_from()` rather than mutating env vars — no `unsafe` required.

```rust
// Pass override directly — no env mutation
let cfg = load_from(f.path(), Some("/tmp/override"), None).unwrap();
assert_eq!(cfg.work_dir, PathBuf::from("/tmp/override"));
```

## Adding Tests

- Place tests in a `#[cfg(test)]` block at the bottom of the module file
- One assertion per test where possible — keep failures specific
- Use `tempfile::TempDir` for any test that creates files
- Test error paths as well as happy paths

## CI (Future)

```yaml
# .github/workflows/ci.yml (planned)
- cargo check
- cargo test
- cargo clippy -- -D warnings
- cargo fmt --check
```

---

# Operations


---

<- 93 tests source: docs/operations/deployment.md -->
# Deployment

## Development

```bash
cd araliya-bot
cargo run
```

Logs go to stderr. Data goes to `~/.araliya/`.

```bash
# Verbose output
RUST_LOG=debug cargo run

# Custom data directory
ARALIYA_WORK_DIR=/tmp/araliya-dev cargo run
```

## Docker

### Quick start

```bash
# Copy and edit the env file with your API keys.
cp .env.example .env
$EDITOR .env

# Build and run (data persisted in ./data/).
docker compose up --build
```

The HTTP/axum channel is available at `http://localhost:8080`.

### Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `ARALIYA_WORK_DIR` | `/data/araliya` | Persistent state (identity, memory) |
| `ARALIYA_HTTP_BIND` | `0.0.0.0:8080` | Bind address for the HTTP/axum channel |
| `ARALIYA_LOG_LEVEL` | *(from config)* | Log level override (`info`, `debug`, …) |
| `LLM_API_KEY` | *(none)* | API key for the configured LLM provider |
| `TELEGRAM_BOT_TOKEN` | *(none)* | Telegram bot token (when Telegram channel is enabled) |

### Persistent data

The bot stores its identity keypair, memory, and other state under `ARALIYA_WORK_DIR`.
Mount a host directory or a named volume over `/data/araliya` so the identity is preserved
across container restarts:

```yaml
volumes:
  - ./data:/data/araliya   # host-directory bind mount (default in docker-compose.yml)
  # or
  - araliya_data:/data/araliya   # named Docker volume
```

### Building the image manually

```bash
docker build -t araliya-bot:latest .
docker run --rm \
  -p 8080:8080 \
  -v "$(pwd)/data:/data/araliya" \
  -e LLM_API_KEY=sk-... \
  araliya-bot:latest
```

## Production (Single Machine)

### Download prebuilt release binary

Every `v*` tag publishes release assets on GitHub Releases.

```bash
# Example version
VERSION=v0.1.0
TIER=default

curl -LO https://github.com/xcorat/araliya-bot/releases/download/${VERSION}/araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/xcorat/araliya-bot/releases/download/${VERSION}/SHA256SUMS
sha256sum -c SHA256SUMS
tar -xzf araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu.tar.gz
cd araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu
install -m 755 bin/araliya-bot /usr/local/bin/araliya-bot
araliya-bot -f config/cfg.toml
```

`TIER` options: `minimal`, `default`, `full`.

Each tiered tarball includes `bin/araliya-bot`, `config/`, and `ui/svui/`.
Inside the bundle, `config/cfg.toml` points to the tier-specific default:

- `minimal` → `config/minimal.toml`
- `default` → `config/default.toml`
- `full` → `config/full.toml`

To create a release from this repository:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Actions workflow publishes the assets automatically.

Build a release binary:

```bash
cargo build --release --locked
cp target/release/araliya-bot /usr/local/bin/
```

Verify artifact details (optional):

```bash
ls -lh target/release/araliya-bot
file target/release/araliya-bot
ldd target/release/araliya-bot
```

### systemd Service

A ready-to-use unit file is provided at `deploy/araliya-bot.service` (see inline comments for full options). Quick setup:

```bash
cargo build --release
install -m 755 target/release/araliya-bot /usr/local/bin/araliya-bot
install -m 755 target/release/araliya-ctl /usr/local/bin/araliya-ctl
useradd -r -s /sbin/nologin araliya
mkdir -p /etc/araliya-bot && install -m 600 /dev/null /etc/araliya-bot/env
echo "LLM_API_KEY=sk-..." >> /etc/araliya-bot/env
cp deploy/araliya-bot.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now araliya-bot
journalctl -u araliya-bot -f
```

The service runs without `-i` — daemon mode, no stdin. Use `araliya-ctl` to interact with the running daemon:

```bash
araliya-ctl status
araliya-ctl health
araliya-ctl subsystems
araliya-ctl shutdown
```

### Environment File

`/etc/araliya-bot/env` (mode 0600, owned root):

```bash
LLM_API_KEY=sk-...
RUST_LOG=info              # optional; default comes from config log_level
# TELEGRAM_BOT_TOKEN=...  # if channel-telegram is enabled
```

## Data Backup

Back up the identity keypair to retain `bot_id` across reinstalls:

```bash
# Backup
cp -r ~/.araliya/bot-pkey*/ /secure/backup/

# Restore
mkdir -p ~/.araliya/
cp -r /secure/backup/bot-pkey*/ ~/.araliya/
chmod 600 ~/.araliya/bot-pkey*/id_ed25519
```

Losing the keypair generates a new identity with a different `bot_id`.
