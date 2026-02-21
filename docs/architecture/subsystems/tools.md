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
- Optional inputs: `mailbox`, `n_last`, `tsec_last`
- `healthcheck` performs a minimal fetch (`maxResults=1`) with filter `in:{mailbox} newsletter`

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
