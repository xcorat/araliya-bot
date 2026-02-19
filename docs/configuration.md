# Configuration

## Config File

Primary config: `config/default.toml` (relative to working directory).

```toml
[supervisor]
bot_name = "araliya"
work_dir = "~/.araliya"
identity_dir = "bot-pkey51aee87e" # optional, absolute path or relative to work_dir
log_level = "info"
```

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_name` | string | `"araliya"` | Human-readable name for this instance |
| `work_dir` | path | `"~/.araliya"` | Root directory for all persistent data. `~` expands to `$HOME`. |
| `identity_dir` | path (optional) | none | Explicit identity directory. Required to disambiguate when multiple `bot-pkey*` dirs exist. |
| `log_level` | string | `"info"` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |

## CLI Verbosity Flags

You can override log level at runtime with `-v` flags:

| Flags | Effective level |
|-------|------------------|
| *(none)* | config/env resolution |
| `-v` / `--verbose` | `debug` |
| `-vv` / `-vvv` | `trace` |

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
| *(planned)* `LLM_API_KEY` | LLM provider API key |
| *(planned)* `TELEGRAM_BOT_TOKEN` | Telegram channel token |

A `.env` file in `araliya-bot/` is loaded automatically at startup if present. It is gitignored — never commit it.

```bash
# .env
LLM_API_KEY=sk-...
```

## Resolution Order

Highest precedence wins:

1. CLI `-v` / `-vv` / `-vvv` flags (log level only)
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
    └── id_ed25519.pub           ed25519 verifying key (mode 0644)
```

Future subsystems will add their own subdirectories here (sessions, memory, etc.).
