# Agents Subsystem

**Status:** v0.0.4 (minimal) — implemented with default `basic_chat` agent.

---

## Overview

The Agents subsystem receives agent-targeted requests from the supervisor messaging service and routes each message to an agent.

Current implementation is intentionally minimal:
- implemented agents: `basic_chat`, `echo`
- default agent: `basic_chat`
- payload type: `CommsMessage { channel_id, content }`
- method grammar: `agents/{agent_id?}/{action?}`

---

## Responsibilities (current)

- Resolve target agent in this order:
  1) explicit `{agent_id}` from method path
  2) channel mapping `channel_id -> agent_id`
  3) default agent (first enabled, `basic_chat` by default)
- Execute selected agent handler
- Return `BusResult` to supervisor for one-shot reply delivery

---

## Routing Lifecycle (current)

```
Request received (`agents/...`)
  ├─ parse method path
  ├─ resolve target agent (method agent > channel map > default)
  ├─ run agent handler
  └─ return `BusPayload::CommsMessage` reply
```

---

## Method Grammar

- `agents`
  - Uses default agent + default action.
- `agents/{agent_id}`
  - Uses explicit agent + default action.
- `agents/{agent_id}/{action}`
  - Uses explicit agent + explicit action.

`{action}` is currently accepted but not differentiated by implemented agents.

## Config

```toml
[agents]
enabled = ["basic_chat"]

[agents.channel_map]
# pty0 = "echo"
```

First entry in `enabled` is the default fallback agent. If `enabled` is empty, runtime auto-enables `echo` as a safety fallback.
