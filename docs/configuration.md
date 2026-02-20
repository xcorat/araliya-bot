# Configuration

## Config File

Primary config: `config/default.toml` (relative to working directory).

```toml
[supervisor]
bot_name = "araliya"
work_dir = "~/.araliya"
identity_dir = "bot-pkey51aee87e" # optional, absolute path or relative to work_dir
log_level = "info"

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
provider = "dummy"
```

## Modular Features (Cargo Flags)

Araliya Bot is built with **compile-time modularity**. If a subsystem or plugin is disabled via Cargo feature, it will not be loaded even if configured in `default.toml`.

| Feature | Enable/Disable | Mandatory |
|---------|----------------|----------|
| `subsystem-agents` | `--features subsystem-agents` | Yes, for agent logic |
| `subsystem-llm` | `--features subsystem-llm` | Yes, for completion tools |
| `subsystem-comms` | `--features subsystem-comms` | Yes, for PTY/HTTP I/O |
| `subsystem-memory` | `--features subsystem-memory` | No, for session memory |
| `channel-pty` | `--features channel-pty` | No, for terminal console |
| `channel-telegram` | `--features channel-telegram` | No, for Telegram bot |

If you disable a subsystem but leave its configuration in `default.toml`, the bot will proceed normally but will not initialize the corresponding handler.

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_name` | string | `"araliya"` | Human-readable name for this instance |
| `work_dir` | path | `"~/.araliya"` | Root directory for all persistent data. `~` expands to `$HOME`. |
| `identity_dir` | path (optional) | none | Explicit identity directory. Required to disambiguate when multiple `bot-pkey*` dirs exist. |
| `log_level` | string | `"info"` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |

## Agents Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.default` | string | `"basic_chat"` | Which agent handles unrouted messages. |
| `agents.routing` | map<string,string> | `{}` | Optional `channel_id -> agent_id` routing overrides. |
| `agents.{id}.memory` | array<string> | `[]` | Memory store types this agent requires (e.g. `["basic_session"]`). |

## Memory Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum key-value entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |

## LLM Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `llm.provider` | string | `"dummy"` | Named LLM provider. requires `subsystem-llm` feature. |

Provider API keys are never stored in config — supply them via environment or `.env`:

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
