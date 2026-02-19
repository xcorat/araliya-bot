# Agents Subsystem

**Status:** Planned — not yet implemented.

---

## Overview

The Agents subsystem is responsible for all agent execution and orchestration. It receives work items from the supervisor, runs agent loops (LLM calls + tool execution), and manages session-level concurrency.

Any plugin with an "agentic" role is initialized and managed here. Each agent can manage sub-agents, and each agent/sub-agent maintains its own state.

---

## Responsibilities

- Receive `AgentWork` from supervisor
- Lane dispatcher: one active run per session, concurrent across sessions
- Agent loop: build prompt → LLM call → parse tool calls → execute tools → append transcript → repeat
- Run registry: track active runs, enforce timeouts, support abort
- Sub-agent spawning (future)
- Session log of all agents and sub-agents

---

## Run Lifecycle

```
AgentWork received
  ├─ resolve session via Memory Service
  ├─ register run in registry (timeout: 300s default)
  ├─ build prompt (transcript + working memory)
  ├─ LLM call
  ├─ parse response
  │   ├─ tool calls → execute via Tools subsystem
  │   │   └─ append tool result to transcript
  │   │   └─ loop
  │   └─ final text → append to transcript
  └─ return AgentResponse
```

---

## Message Protocol

- `AgentWork` — session_id, message, reply channel
- `AgentResponse` — reply text, intermediate steps, usage
- `AgentEvent` — streaming progress (tool calls, text chunks) (future)
- `AgentControl` — abort, steer, list runs
