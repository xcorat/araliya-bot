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
| `GET /api/tree` | Component tree JSON (no private data); see [Bus Protocol](architecture/standards/bus-protocol.md#management-routes-manage). |
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
| `llm.default` | string | `"dummy"` | Active LLM provider (`"dummy"`, `"openai"`, or `"qwen"`). Requires `subsystem-llm` feature. |
| `llm.openai.api_base_url` | string | OpenAI endpoint | Chat completions URL. Override for Ollama / LM Studio. |
| `llm.openai.model` | string | `"gpt-4o-mini"` | Model name sent in each request. |
| `llm.openai.temperature` | float | `0.2` | Sampling temperature (omitted automatically for `gpt-5` family). |
| `llm.openai.timeout_seconds` | integer | `60` | Per-request HTTP timeout in seconds. |
| `llm.openai.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.openai.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.openai.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |
| `llm.qwen.api_base_url` | string | `"http://127.0.0.1:8081/v1/chat/completions"` | Qwen-style chat completions URL. |
| `llm.qwen.model` | string | `"qwen2.5-instruct"` | Model name sent in each request. |
| `llm.qwen.temperature` | float | `0.2` | Sampling temperature. |
| `llm.qwen.timeout_seconds` | integer | `60` | Per-request HTTP timeout in seconds. |
| `llm.qwen.input_per_million_usd` | float | `0.0` | Input token price (USD per 1M tokens). |
| `llm.qwen.output_per_million_usd` | float | `0.0` | Output token price (USD per 1M tokens). |
| `llm.qwen.cached_input_per_million_usd` | float | `0.0` | Cached input token price (USD per 1M tokens). |

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
