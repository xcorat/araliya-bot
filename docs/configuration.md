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
# skills = ["gmail", "newsmail_aggregator"]  # bus tools this agent may invoke

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

### Launch Profiles (`config/profiles/`)

Named launch configurations live in `config/profiles/`.  Each is a **partial overlay** that inherits from `config/default.toml` via `[meta] base = "../default.toml"`.  Only the keys that differ from the base are listed.

`config/profiles/full.toml` — all features enabled (Telegram, Gmail, news, docs):

```toml
[meta]
base = "../default.toml"  # path relative to this file

[comms.telegram]
enabled = true

[agents]
default = "chat"
# … only changed entries …
```

To use a profile:

```bash
cargo run -- -f config/profiles/full.toml
cargo run -- -f config/profiles/news.toml
```

Available profiles: `full`, `docker`, `llm-test`, `docs_agent`, `news`, `newsroom`, `runtime_cmd`, `test-gdelt`, `uniweb`.

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
model = "gpt-5-nano"
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
| `plugin-webbuilder` | `--features plugin-webbuilder` | No, iterative Svelte page builder with Node.js runtime |
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
| `GET /preview/{session_id}/{*path}` | Serves webbuilder workspace `dist/` files (requires `plugin-webbuilder`). |

## UI Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ui.svui.enabled` | bool | `true` | Enables the Svelte-based web UI backend. |
| `ui.svui.static_dir` | string (optional) | none | Path to the static build directory. Relative to the bot's working directory. If absent, a built-in placeholder is served. |

The UI is a SvelteKit SPA built with shadcn-svelte, served at `/ui/`. Build it with:

```bash
cd frontend/svui && pnpm install && pnpm build
```

The build output goes to `frontend/build/`, which matches the default `static_dir` setting.

## Agents Configuration

The agents subsystem routes inbound messages to registered agents. In v0.6, every agent has an explicit **runtime class** that describes its execution model. The `agents/list` bus method (and `GET /api/agents/list` over HTTP) returns each registered agent's `runtime_class` label alongside its ID, session count, and store types.

| Runtime class | Execution model |
|---|---|
| `request_response` | Stateless single-turn exchange — no session state. |
| `session` | Persistent multi-turn conversation with transcript memory. |
| `agentic` | Bounded multi-step orchestration: instruction → tools → response. |
| `specialized` | Built-in agents with specific delegation or passthrough patterns. |

See [Agents Subsystem](architecture/subsystems/agents.md) for a full description of runtime classes and the orchestration model.

### Core Settings

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.default` | string | `"basic_chat"` | Agent that handles messages with no explicit routing. Must be present in `enabled` when `enabled` is non-empty. |
| `agents.enabled` | array\<string\> | `[]` | Agent IDs reachable via routing. An empty list means all registered agents are reachable. |
| `agents.debug_logging` | bool | `false` | Write per-turn intermediate data (`instruct_prompt`, `tool_calls_json`, `context`, etc.) to session KV for all agentic agents. Read via `GET /api/sessions/{id}/debug`. |

### Routing

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.routing` | map\<string, string\> | `{}` | `channel_id → agent_id` overrides. Takes priority over the default agent. Example: `pty0 = "echo"`. |

### Per-Agent Settings

These fields appear under `[agents.{id}]` for any agent ID:

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.{id}.enabled` | bool | `true` | Set to `false` to disable this agent without removing its config section. |
| `agents.{id}.memory` | array\<string\> | `[]` | Memory store types this agent requires. Example: `["basic_session"]`. |
| `agents.{id}.skills` | array\<string\> | `[]` | Bus tools this agent may invoke. Only listed tools appear in the instruction manifest. Agents without this field cannot call any bus tools. |

### Agentic Chat (`agentic-chat`)

Runtime class: `agentic`. Dual-model instruction loop — a fast model selects tools, the main model generates the response.

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.agentic-chat.use_instruction_llm` | bool | `false` | Route the instruction pass through `llm/instruct`. When `[llm.instruction]` is configured, that provider handles tool selection; otherwise falls back to the main provider. |

### Docs Agent (`docs`)

Runtime class: `agentic`. Retrieval-augmented document QA. Supports two retrieval paths selected by `use_kg`.

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.docs.docsdir` | string | none | Source directory to import into the agent's document store on startup. |
| `agents.docs.index` | string | `"index.md"` | Fallback document (relative to `docsdir`) when no search result is returned. |
| `agents.docs.use_kg` | bool | `false` | Enable KG+FTS retrieval via `IKGDocStore`. Requires the `ikgdocstore` Cargo feature. |
| `agents.docs.kg.min_entity_mentions` | integer | `2` | Minimum occurrences for an entity to enter the knowledge graph. |
| `agents.docs.kg.bfs_max_depth` | integer | `2` | BFS hop limit from seed entities during graph traversal. |
| `agents.docs.kg.edge_weight_threshold` | float | `0.15` | Minimum relation weight to follow during BFS. |
| `agents.docs.kg.max_chunks` | integer | `8` | Total chunk budget in the assembled retrieval context. |
| `agents.docs.kg.fts_share` | float | `0.5` | Fraction of `max_chunks` reserved for FTS results. |
| `agents.docs.kg.max_seeds` | integer | `5` | Maximum seed entities used for BFS per query. |

### Runtime Command Agent (`runtime_cmd`)

Runtime class: `specialized`. Passes every user message directly to an external language runtime as source code. No LLM is involved.

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.runtime_cmd.runtime` | string | `"bash"` | Runtime environment name, used as the working directory under the agent's identity area. |
| `agents.runtime_cmd.command` | string | `"bash"` | Interpreter binary passed to `runtimes/exec`. |
| `agents.runtime_cmd.setup_script` | string | none | Optional shell script run once to initialize the runtime environment on first use. |

### Web Builder Agent (`webbuilder`)

Runtime class: `agentic`. Iterative Svelte page builder — the LLM writes files and runs Node.js commands in a loop until the page builds. Progress events stream via SSE as `>>STEP<<{...}` prefixed content chunks. Built pages are served at `GET /preview/{session_id}/`.

Requires: `plugin-webbuilder` Cargo feature + `subsystem-runtimes`.

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.webbuilder.max_iterations` | integer | `10` | Maximum LLM → tool → feedback cycles per request. |
| `agents.webbuilder.scaffold` | string | `"vite-svelte"` | Scaffold template for new workspaces. |

### News Agent (`news`)

Runtime class: `specialized`. Fetches recent emails from the newsmail aggregator tool and summarizes them with the LLM.

Bus methods: `agents/news` (default handle), `agents/news/read`, `agents/news/health`.

Default query arguments for the newsmail aggregator can be set under `[agents.news.query]`:

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.news.query.label` | string | none | Gmail label name to filter (e.g. `n/News`). |
| `agents.news.query.n_last` | integer | none | Maximum number of recent emails to fetch. |
| `agents.news.query.t_interval` | string | none | Recency window as a duration string (e.g. `1min`, `1d`, `1mon`). |
| `agents.news.query.tsec_last` | integer | none | Recency window in seconds (legacy fallback for `t_interval`). |

### GDELT News Agent (`gdelt_news`)

Runtime class: `specialized`. Fetches recent global events from the GDELT v2 BigQuery dataset (`events_partitioned`) and summarises them via the LLM. Results are cached by content hash so identical event sets are summarised only once. The prompt renders country flag emojis, event-type status emotes, and a 🚨 crisis flag for high-impact events.

Query parameters are set under `[agents.gdelt_news.gdelt_query]`:

| Field | Type | Default | Description |
|---|---|---|---|
| `lookback_minutes` | integer | `60` | How many minutes back to include. |
| `limit` | integer | `50` | Maximum rows to return. |
| `min_articles` | integer | none | Only include events covered by at least this many articles. |
| `min_importance` | float | none | Only include events where `ABS(GoldsteinScale) >= value` (0–10 scale). |
| `sort_by_importance` | bool | `false` | Sort by `ABS(GoldsteinScale) DESC, NumArticles DESC` instead of `NumArticles DESC` only. |
| `english_only` | bool | `false` | Restrict to events with at least one English-language mention (joins `gdeltv2.eventmentions_partitioned` on `MentionDocTranslationInfo IS NULL`). |

### Newsroom Agent (`newsroom`)

Runtime class: `specialized`. Persistent GDELT newsroom: fetches events from BigQuery, deduplicates at the individual-event level in SQLite (capped at 2 500 rows), and summarises only the **newly detected** events via the LLM. The last 10 summaries are retained; the most recent is returned on page load without a BigQuery query. Source outlets are tracked with an EMA tone score and a composite rank (50 % fetch frequency · 30 % tone · 20 % recency).

Requires: `plugin-newsroom-agent` Cargo feature (implies `plugin-gdelt-tool` + `isqlite`).

Bus methods: `agents/newsroom/read`, `agents/newsroom/latest`, `agents/newsroom/events`, `agents/newsroom/sources`, `agents/newsroom/status`, `agents/newsroom/health`.

Query parameters share the same fields as `gdelt_news` and are set under `[agents.newsroom.gdelt_query]`:

| Field | Type | Default | Description |
|---|---|---|---|
| `lookback_minutes` | integer | `60` | How many minutes back to include. |
| `limit` | integer | `50` | Maximum rows to return per fetch. |
| `min_articles` | integer | none | Drop events covered by fewer than this many articles. |
| `min_importance` | float | none | Drop events where `ABS(GoldsteinScale) < value` (0–10). |
| `sort_by_importance` | bool | `false` | Order by `ABS(GoldsteinScale) DESC, NumArticles DESC`. |
| `english_only` | bool | `false` | English-language filter (joins `eventmentions_partitioned`). |

### News Aggregator Agent (`news_aggregator`)

Runtime class: `specialized`. Reads source URLs from the newsroom's event store, fetches article HTML, strips tags, and summarises each article via the instruction LLM. Summaries are stored in an `IKGDocStore` inside the **newsroom agent's own identity directory** (`{newsroom_identity_dir}/kgdocstore/`) — no separate identity is created. After each batch the knowledge graph is rebuilt for KG-RAG search.

The agent is triggered automatically in the background every time `newsroom/read` produces a new summary. It can also be invoked directly.

**Forward cursor:** processed URLs are tracked with a persistent cursor in the `agg_state` table of the newsroom's `events.db`. Each cycle queries events with `id > last_processed_id ORDER BY id ASC LIMIT BATCH_LIMIT`, so every event is processed exactly once across restarts and cycles. The cursor is advanced even when all URLs in a batch are already present in the KGDocStore.

**Adaptive KG config:** `rebuild_kg_with_config` is called after each cycle with `min_entity_mentions = 1` when the store has fewer than 10 documents, and `2` otherwise. This prevents an empty graph on small corpora where entities rarely repeat across articles.

Requires: `plugin-news-aggregator` Cargo feature (implies `plugin-newsroom-agent` + `ikgdocstore`).

Bus methods: `agents/news_aggregator/aggregate`, `agents/news_aggregator/status`, `agents/news_aggregator/search`.

Constants (compile-time, not configurable via TOML):

| Constant | Value | Description |
|---|---|---|
| `MAX_ARTICLE_CHARS` | `4 000` | Input character cap per article sent to the LLM. |
| `BATCH_LIMIT` | `50` | Maximum URLs processed per `aggregate` cycle (matches GDELT fetch limit). |
| `FETCH_TIMEOUT_S` | `15` | Per-request HTTP timeout in seconds. |
| `FETCH_DELAY_MS` | `1 500` | Polite delay between consecutive article fetches. |
| `CHUNK_SIZE` | `512` | Byte chunk size for BM25 indexing. |

### Gmail Agent (`gmail`)

Runtime class: `specialized`. Reads the latest Gmail messages via the Gmail tool and returns a formatted summary.

Bus method: `agents/gmail/read`. Internally dispatches `tools/execute` with `tool = "gmail"`, `action = "read_latest"`.

### Newsmail Aggregator Tool Endpoints

The newsmail aggregator is a tool (not an agent) invoked by the `news` agent and available for `skills`-configured agents:

- Bus method: `tools/execute` with `tool = "newsmail_aggregator"`, `action = "get"`
- Optional request keys: `label`, `mailbox`, `n_last`, `t_interval` (preferred), `tsec_last` (legacy)
- Healthcheck: `action = "healthcheck"` returns one `newsletter`-filtered sample when available

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
| `--log-file <PATH>` | Write logs to the given file (append mode) instead of stderr. |
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
