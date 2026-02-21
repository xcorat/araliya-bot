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
```

When `comms.http.enabled = true`, the HTTP channel exposes `GET /health` on
`comms.http.bind` and forwards the request to the management bus method
`manage/http/get`.

### Full-Featured Config (`full.toml`)

For deployments using the `full` Cargo feature flag (`cargo run --features full`), a pre-configured `config/full.toml` is provided. This configuration turns off dummy/basic components and enables the full suite of features:

- **Agents**: Uses the session-aware `chat` agent as default and enables the `gmail` agent.
- **LLM**: Uses the `openai` provider (requires `LLM_API_KEY`) with `gpt-4o`.
- **Comms**: Enables `pty`, `telegram` (requires `TELEGRAM_BOT_TOKEN`), and `axum_channel`.
- **UI**: Enables the Svelte-based web UI backend (`svui`).

To use it, replace `default.toml` with `full.toml` or point your configuration loader to it.

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

## Memory Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum key-value entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |

## LLM Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.default` | string | `"dummy"` | Active LLM provider (`"dummy"` or `"openai"`). Requires `subsystem-llm` feature. |

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
5. `config/default.toml`
6. Built-in defaults

## Data Directory Layout

All persistent data is stored under `work_dir` (default `~/.araliya`):

```
~/.araliya/
└── bot-pkey{8-hex-bot_id}/     bot identity directory
    ├── id_ed25519               ed25519 signing key seed (mode 0600)
    ├── id_ed25519.pub           ed25519 verifying key (mode 0644)
    └── memory/                  session data (when subsystem-memory enabled)
        ├── sessions.json        session index
        └── sessions/
            └── {uuid}/
                ├── kv.json
                └── transcript.md
```

See [Memory Subsystem](architecture/subsystems/memory.md) for details on session data layout.
