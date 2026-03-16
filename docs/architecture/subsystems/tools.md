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

## Per-Agent Tool Scoping

Bus tools are not globally visible to all agents. Each agent declares which tools it may invoke via `skills = [...]` in its config section. Only declared tools appear in the agent's instruction-pass tool manifest and response-pass system prompt. Agents without a `skills` declaration cannot call any bus tools — they can only use their own local tools (e.g. the docs agent's `docs_search`).

```toml
[agents.agentic-chat]
skills = ["gmail", "newsmail_aggregator"]  # can invoke both

[agents.docs]
# skills = []  # default — only local docs_search tool
```

## GDELT BigQuery Tool

- Module: `src/subsystems/tools/gdelt_bigquery.rs`
- Feature flag: `plugin-gdelt-tool` (enables `dep:jsonwebtoken` + `subsystem-tools`)
- Actions: `fetch`, `healthcheck`
- Auth: service-account JSON at `config/secrets/araliya-1012f47de255.json` → RS256 JWT (`jsonwebtoken` crate) → OAuth2 token exchange → BigQuery REST `runQuery`
- Dataset: `gdelt-bq.gdeltv2.events` (public GDELT v2 BigQuery dataset)
- Query scope: `https://www.googleapis.com/auth/bigquery.readonly`

Query arguments (`GdeltQueryArgs`):

| Field | Type | Default | Description |
|---|---|---|---|
| `days_back` | `u32` | `1` | How many days back to include |
| `limit` | `u32` | `50` | Maximum rows to return |
| `min_articles` | `u32` | none | Only include events with at least this many articles |

`fetch` returns a JSON array of `GdeltEvent` objects with fields: `date`, `actor1`, `actor2`, `event_code`, `goldstein`, `num_articles`, `avg_tone`, `source_url`.

`healthcheck` runs a minimal 1-row query to verify BigQuery connectivity.

---

## Agent Integration

- Gmail agent plugin: `src/subsystems/agents/gmail.rs`
- Agent bus method: `agents/gmail/read`
- Flow: `agents/gmail/read` → `tools/execute` (`gmail/read_latest`) → agent formats summary reply
- News agent plugin: `src/subsystems/agents/news.rs`
- Agent bus methods: `agents/news` (default handle), `agents/news/read`
- Flow: `agents/news` → `tools/execute` (`newsmail_aggregator/get`) → agent returns raw tool payload
- GDELT News agent plugin: `src/subsystems/agents/gdelt_news.rs`
- Agent bus methods: `agents/gdelt_news` (default handle), `agents/gdelt_news/read`
- Flow: `agents/gdelt_news/read` → `tools/execute` (`gdelt_bigquery/fetch`) → content hash → KV cache check → LLM summarization → cached reply
