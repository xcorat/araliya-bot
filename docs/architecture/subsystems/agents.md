# Agents Subsystem

**Version:** v0.7 — agent definitions · Unix-like directory layering · manifests (`agent.toml`) · co-located prompts · user agent overrides · runtime-classified agents · `AgentRegistration` wrapper · `AgentRuntimeClass` taxonomy · `AgentsState` capability boundary · `AgenticLoop` dual-model orchestration · `ChatCore` composition · session queries · docs RAG/KG-RAG · per-turn debug logging · externalized prompt templates.

---

## Overview

The Agents subsystem is the policy and execution layer of the bot. It receives agent-targeted requests from the supervisor bus, resolves which agent should handle each request, and delegates execution to that agent's runtime.

An agent is not just a function — it is a named entity that couples:

- a stable cryptographic identity and identity-bound working area
- memory stores (sessions, transcripts, key-value data)
- prompt files that define behavioral policy
- a declared set of tools it may invoke
- access to one or more LLM completion paths
- I/O routing from comms channels
- a **runtime class** that defines its execution model

The runtime class is the central organizing concept in v0.6. Every agent — built-in or future config-defined — has a runtime class that describes how it processes work: whether it handles a single stateless exchange, maintains a conversation, runs a multi-step orchestration loop, or something else entirely.

Agents are non-blocking by design. Each agent receives ownership of `reply_tx` and resolves it in its own time — inline for simple agents, via a spawned async task for agents that perform I/O or multi-step orchestration.

---

## Runtime Classes

The runtime class of an agent describes its execution model — not what the agent does, but the shape in which it does it. Runtime classes are represented by the `AgentRuntimeClass` type and are recorded on every registered agent.

### RequestResponse

A `RequestResponse` agent handles a single inbound message and produces a single reply. No session state is created or required. Execution is synchronous from the perspective of the conversation: the caller sends a message and receives a response.

This is the simplest runtime class. It is appropriate for agents that perform stateless transformations, simple LLM pass-throughs, or pure lookups.

Built-in agents classified as `RequestResponse`:

- **`echo`** — returns the input unchanged; the zero-dependency fallback agent
- **`basic_chat`** — forwards the message to the LLM and returns the response

### Session

A `Session` agent maintains persistent conversation state across turns. Each interaction belongs to a session identified by a session ID, which is either supplied by the caller or created automatically on the first message.

Session agents persist a transcript of user and assistant messages. On each turn, recent history is injected into the LLM prompt to provide multi-turn context. Sessions are stored in the agent's identity-bound working area and survive restarts.

Built-in agents classified as `Session`:

- **`chat`** — session-aware LLM conversation via `SessionChatPlugin`

### Agentic

An `Agentic` agent runs a bounded multi-step orchestration loop on each request. The typical sequence is: an instruction pass that selects and parameterizes tools, tool execution, context assembly, and a final response pass. The agent may use session memory to persist state across requests.

What distinguishes `Agentic` from `Session` is that the agent's internal turns are driven by tool calls, not by the user's conversational messages. The user sends one message; the agent may internally run multiple LLM and tool steps before replying.

Built-in agents classified as `Agentic`:

- **`agentic-chat`** — dual-model instruction loop: a fast model selects tools, the main model generates the response
- **`docs`** — retrieval-augmented QA: the agent formulates a search query, retrieves documentation chunks, and answers with the retrieved context
- **`webbuilder`** — iterative Svelte page builder: the LLM writes files and runs Node.js commands in a loop until the page builds, emitting streaming progress events

### Specialized

`Specialized` is a transitional runtime class for built-in agents whose execution model does not cleanly map to `RequestResponse`, `Session`, or `Agentic`. These agents use specific delegation and passthrough patterns that predate the v0.6 classification model.

Built-in agents classified as `Specialized`:

- **`news`** — fetches email via the newsmail aggregator tool and summarizes with the LLM
- **`gmail`** — delegates to the Gmail tool and formats the result as a comms reply
- **`gdelt_news`** — fetches recent global events from the GDELT v2 BigQuery dataset and summarizes them via the LLM; uses content-hash-keyed KV caching so identical event sets are summarized only once
- **`runtime_cmd`** — passes user messages directly to an external language runtime (Node.js, Python, Bash) via the runtimes subsystem; no LLM is involved

### Planned: Workflow and Background

Two additional runtime classes are part of the v0.6 architecture but are not yet implemented:

**`Workflow`** covers bounded orchestrated processes with more explicit step transitions than freeform agentic interaction. Examples include document processing pipelines, bounded multi-step assistant flows, and delegated task graphs that may include approval steps or checkpointing.

**`Background`** covers event-driven long-running agents that operate independently of individual inbound messages. A background agent would subscribe to event sources, emit outputs asynchronously, maintain state over time, and have its own supervised start/stop lifecycle.

Both classes exist as enum variants in `AgentRuntimeClass` and are clearly marked as deferred in the implementation. No routing or execution path supports them yet. Their lifecycle semantics — supervision model, resource controls, output channels, security boundaries — will be designed in a dedicated later phase.

---

## Agent Families

### Built-in Agents

Built-in agents are implemented in Rust and compiled into the binary. They cover baseline functionality and serve as the reference implementations for each runtime class. Built-in agents are registered at subsystem startup and controlled by Cargo feature flags and config `enabled` lists.

The current built-in agents, their runtime classes, and their roles:

| Agent ID | Runtime class | Role |
|---|---|---|
| `echo` | `RequestResponse` | Stateless echo; safety fallback when `enabled` is empty |
| `basic_chat` | `RequestResponse` | Single-turn LLM pass-through |
| `chat` | `Session` | Multi-turn session-aware LLM conversation |
| `agentic-chat` | `Agentic` | Dual-model instruction → tool → response loop |
| `docs` | `Agentic` | RAG or KG-RAG document QA |
| `news` | `Specialized` | News email fetch and LLM summarization |
| `gmail` | `Specialized` | Gmail read via tool delegation |
| `gdelt_news` | `Specialized` | GDELT BigQuery news fetch and LLM summarization |
| `runtime_cmd` | `Specialized` | Direct passthrough to an external language runtime |
| `webbuilder` | `Agentic` | Iterative Svelte page builder with Node.js runtime access |

### Static Agents (Upcoming)

Static agents are config-defined agent instances loaded at startup. Rather than a dedicated Rust implementation, a static agent is assembled from a configuration section that declares its ID, runtime class, prompt files, memory requirements, and tool allowlist.

Static agents use the same runtime classes, session infrastructure, identity model, and capability boundary as built-in agents. They are the v0.6 path for adding new prompt-and-policy-driven behaviors without writing Rust code.

Static agent support is the next implementation phase. The runtime foundation introduced in v0.6 is designed to accommodate static agents directly alongside built-in ones.

---

## Routing

Agents are resolved from the inbound request in this priority order:

1. **Explicit agent ID** from the method path (e.g. `agents/chat/handle`)
2. **Channel mapping** — `channel_id → agent_id` override from `[agents.routing]` in config
3. **Default agent** — the agent named in `agents.default`, provided it is in the `enabled` set; falls back to `echo` if `enabled` is empty

The routing layer is not aware of runtime classes. It resolves a target agent ID and delegates; the registered agent's runtime handles the rest.

An explicit agent ID that is not in the `enabled` set is rejected with a not-found error. A default agent that is not in `enabled` is also rejected unless `enabled` is empty (empty `enabled` means no restriction — all registered agents are reachable).

---

## Method Grammar

The bus method path determines the target agent and action:

| Method | Effect |
|---|---|
| `agents` | Default agent, default action |
| `agents/{agent_id}` | Explicit agent, default action |
| `agents/{agent_id}/{action}` | Explicit agent, named action |

The `{action}` segment is forwarded to the agent's `handle` method as the `action` parameter. Most agents treat any non-special action as equivalent to the default. Specialized agents (such as `gmail` and `news`) use the action to distinguish `read`, `health`, and other operations.

Several method paths are intercepted by the subsystem before agent routing and never reach individual agents — see [Session Queries](#session-queries) below.

---

## Request Handling Contract

`AgentsSubsystem` implements `BusHandler` with prefix `"agents"`. The supervisor calls `handle_request` and returns immediately; ownership of `reply_tx` is transferred to the handler.

- **Synchronous agents** (`echo`) resolve `reply_tx` inline on the calling thread.
- **Async agents** (`basic_chat`, `chat`, `agentic-chat`, `docs`, `news`, `gmail`, `runtime_cmd`) move `reply_tx` into a `tokio::spawn`ed task and resolve it when the async work completes.

The supervisor is never blocked waiting for a reply. Every code path must resolve `reply_tx` exactly once — either with a success payload or an error.

---

## Internal Architecture

### Agent Registration

Each agent in `AgentsSubsystem` is stored as an `AgentRegistration`:

```
AgentRegistration {
    runtime_class: AgentRuntimeClass,   // RequestResponse | Session | Agentic | Specialized | …
    agent: Box<dyn Agent>,              // the implementation
}
```

This pairing is the v0.6 structural foundation. Runtime class is not stored inside the `Agent` trait itself — it lives alongside the implementation in the registration record. This keeps existing agent implementations unchanged while making runtime class a first-class attribute of every registered agent.

`AgentsSubsystem` maintains a `HashMap<String, AgentRegistration>` keyed by agent ID. The agent's own `id()` method is the single source of truth for its key.

The `agents/list` bus method returns all registered agents, including the `runtime_class` label for each entry.

### The `Agent` Trait

All agent plugins implement the `Agent` trait:

```
trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(action, channel_id, content, session_id, reply_tx, state);
    fn handle_stream(channel_id, content, session_id, reply_tx, state);  // default: falls back to handle
}
```

`handle_stream` is provided with a default implementation that falls back to `handle`. Agents that support streaming LLM output override it to call `llm/stream` on the bus and reply with `BusPayload::LlmStreamResult`.

### AgentsState Capability Boundary

Agents receive `Arc<AgentsState>`, not a raw bus handle. The capability surface available to every agent is:

| Method or field | Description |
|---|---|
| `complete_via_llm(channel_id, content)` | Forward to `llm/complete`; return `BusResult` |
| `complete_via_llm_with_system(channel_id, content, system)` | Forward to `llm/complete` with a system prompt |
| `complete_via_instruct_llm(channel_id, content, system)` | Forward to `llm/instruct`; routes to `[llm.instruction]` if configured, else falls back to the main provider |
| `stream_via_llm_with_system(channel_id, content, system, reply_tx)` | Forward to `llm/stream` for streaming responses |
| `execute_tool(tool, action, params_json, channel_id, session_id)` | Dispatch a tool call through `tools/execute` |
| `open_agent_store(agent_id)` | Open the agent's `AgentStore` (sessions index, KV store, text files) |
| `open_sqlite_store(agent_id, db_name)` | Open (or create) a named SQLite database for `agent_id` at `{identity_dir}/sqlite/{db_name}.db`. Requires `isqlite` feature. Synchronous — wrap in `spawn_blocking` in async context. |
| `get_or_create_subagent(agent_id, subagent_name)` | Provision a subagent identity under the given agent |
| `runtime_init(…)` | Initialize an external runtime environment via `runtimes/init` |
| `runtime_exec(…)` | Execute source code in an external runtime via `runtimes/exec` |
| `memory` | `Arc<MemorySystem>` — create or load session handles |
| `agent_memory` | `HashMap<String, Vec<String>>` — per-agent declared memory store types |
| `agent_identities` | `HashMap<String, Identity>` — cryptographic identities per agent |
| `agent_skills` | `HashMap<String, Vec<String>>` — per-agent bus-tool allowlists |
| `llm_rates` | `ModelRates` — current LLM token pricing for spend accounting |
| `agents_dir` | `String` — root directory for system agent definitions (`config/agents/`) |
| `user_agents_dir` | `Option<String>` — optional user agent definitions directory (`~/.araliya/agents/`); when present, user agents override system ones by ID |
| `debug_logging` | `bool` — whether per-turn debug data should be written to session KV |

The raw bus handle is private. Agents cannot address arbitrary bus targets. This boundary keeps agent implementations testable in isolation and limits accidental subsystem coupling.

### Agentic Loop

`AgenticLoop` is the shared orchestration engine for multi-step agent plugins. Both `agentic-chat` and `docs` use it. It implements a three-phase execution model per request:

**Phase 1 — Instruction pass**

The agent renders an instruction prompt that includes the user message, a manifest of action tools the agent is permitted to call, and a list of available memory sources. The prompt is sent to the instruction LLM (`llm/instruct` or `llm/complete` depending on configuration). The response is parsed as a JSON array of `{tool, action, params}` objects. If parsing fails, the phase degrades gracefully to an empty tool list.

**Phase 2 — Tool execution**

Each parsed tool call is dispatched in sequence. Local tools (both action and memory tools, such as `docs_search`) run via `tokio::task::spawn_blocking`. Bus tools run via `AgentsState::execute_tool`. Outputs are collected into a context string.

**Phase 3 — Response pass**

The context from tool execution, recent conversation history, and the original user message are combined into a response prompt. This is sent to the main LLM (`llm/complete`) with an optional system preamble. The reply is returned to the caller with the session ID attached.

**Session lifecycle**

`AgenticLoop` manages session state across the three phases. On the first request for a session, a new session is created in the agent's identity-scoped area. On subsequent requests with the same session ID, the existing session is reloaded. The transcript is appended after each complete turn.

**Configuration**

`AgenticLoop` is constructed with:

- `agent_id` — used for identity and session scoping
- `use_instruction_llm` — when `true`, routes the instruction pass through `llm/instruct`; when `false`, both passes use `llm/complete`
- `instruct_prompt_file` — prompt template filename for the instruction pass (resolved via `agent_layer()`)
- `context_prompt_file` — prompt template filename for the response pass (resolved via `agent_layer()`)
- `local_tools` — in-process action tools implementing the `LocalTool` trait
- `memory_tools` — in-process memory tools (e.g., document retrieval) implementing the `LocalTool` trait
- `allowed_tools` — bus tool allowlist from agent skills config
- `agents_dir` — root directory for agent definitions (`config/agents/`); used to resolve prompt files via `agent_layer()`
- `debug_logging` — whether to write per-turn debug data to session KV

**Instruction prompt structure**

The instruction prompt is built using a template (`agentic_instruct.txt`) with three primary sections:

- `Available tools:` — action tools the agent can invoke (displayed with names and descriptions)
- `Available memory:` — memory sources the agent can consult (document stores, knowledge graphs, etc.)
- `User message:` — the original user query

This separation helps the LLM reason about data retrieval (memory) versus actions that modify state or interact with external systems.

**Routing the instruction pass**

When `use_instruction_llm = false` (the default), both passes use the same LLM provider. When `use_instruction_llm = true`, the instruction pass is routed through `llm/instruct`. If `[llm.instruction]` is configured with a separate provider, that provider handles the instruction pass; otherwise it falls back to the main provider. This allows a small fast model to handle tool selection while a larger model handles final response generation.

### Chat-Family Composition

Chat-family agents (`basic_chat`, `chat`) share logic through `ChatCore`, a stateless composition layer rather than a shared base class:

```
src/subsystems/agents/chat/
├── mod.rs           — feature-gated re-exports
├── core.rs          — ChatCore: shared async building blocks
├── basic_chat.rs    — BasicChatPlugin: thin wrapper over ChatCore
└── session_chat.rs  — SessionChatPlugin: ChatCore + session/memory
```

`ChatCore::basic_complete` handles the common case: build an LLM request from the message content, dispatch to `llm/complete` on the bus, and return the result. `BasicChatPlugin` calls it directly. `SessionChatPlugin` calls it after loading (or creating) a session, appending the user message to the transcript, and injecting recent history as context.

### Agent and Subagent Identities

Each registered agent is provisioned with its own persistent ed25519 cryptographic identity during subsystem initialization. Identities are stored under `{memory_root}/agent/{agent_id}-{public_id}/` and survive restarts. The agent's identity directory is the root of its working area — session indexes, KV stores, document stores, and subagent directories all live under it.

A subagent is a delegated worker provisioned under a parent agent's identity structure. Subagents are created via `AgentsState::get_or_create_subagent(agent_id, subagent_name)`, which creates a nested identity at `{agent_dir}/subagents/{subagent_name}-{public_id}/`. Subagents are lightweight delegated workers in the current design; a later phase may evolve them into full runtime-managed children with their own lifecycle.

---

## Session Queries

The following bus methods are intercepted by the subsystem before agent routing and handled directly by `AgentsSubsystem`. They never reach individual agents.

| Method | Payload | Response |
|---|---|---|
| `agents/sessions` | `Empty` | JSON array of all sessions: `session_id`, `created_at`, `updated_at`, `store_types`, `last_agent` |
| `agents/sessions/detail` | `SessionQuery { session_id }` | Session metadata and full transcript |
| `agents/sessions/memory` | `SessionQuery { session_id }` | `{ session_id, content }` — current working memory |
| `agents/sessions/files` | `SessionQuery { session_id }` | `{ session_id, files[] }` with `name`, `size_bytes`, `modified` |
| `agents/sessions/debug` | `SessionQuery { session_id, agent_id? }` | Per-turn debug data (see below) |
| `agents/list` | `Empty` | JSON array of all registered agents: `agent_id`, `name`, `runtime_class`, `session_count`, `store_types`, `last_fetched` |
| `agents/kg_graph` | `SessionQuery { agent_id }` | The agent's knowledge graph as JSON |
| `agents/health` | `Empty` | Subsystem health status |
| `agents/status` | `Empty` | Operational status |
| `agents/detailed_status` | `Empty` | Extended status including session count and enabled agents |
| `agents/{agent_id}/status` | `Empty` | Per-agent status |
| `agents/{agent_id}/detailed_status` | `Empty` | Per-agent extended status including session count and last fetch |

---

## Per-Turn Debug Logging

When `debug_logging = true` is set in `[agents]` config, the `AgenticLoop` writes intermediate execution data into the session KV store after each turn. This data is readable via the `agents/sessions/debug` bus method and `GET /api/sessions/{session_id}/debug` over HTTP.

Each turn's data is stored under `debug:turn:{n}:*` keys:

| Key | Content |
|---|---|
| `debug:turn:{n}:user_input` | The raw user message for this turn |
| `debug:turn:{n}:instruct_prompt` | The rendered instruction prompt sent to the LLM |
| `debug:turn:{n}:instruction_response` | The raw LLM output from the instruction pass |
| `debug:turn:{n}:tool_calls_json` | The parsed tool call array |
| `debug:turn:{n}:tool_outputs_json` | JSON array of tool results with `ok`/`error` per call |
| `debug:turn:{n}:context` | The assembled context string passed to the response pass |
| `debug:turn:{n}:response_prompt` | The rendered response prompt sent to the main LLM |

Debug logging is off by default. It is intended for development, troubleshooting, and session inspection — not for production use where session storage is a concern.

---

## Agent Definitions and Manifests

**Phase 6 introduces agent definitions** — a Unix-like layered approach to agent identity, configuration, and prompts.

Each agent is now defined by a directory under `config/agents/{agent_id}/` containing:

- `agent.toml` — manifest declaring the agent's identity, extends, enabled status, tools/skills, memory stores, and LLM settings
- `*.md` — co-located prompt files (instruct.md, context.md, summary.md, etc.)

### Manifest Structure

Each `agent.toml` declares:

```toml
[agent]
extends = ""                    # optional: inherit from another agent (e.g., "docs_agent" extends "docs")
enabled = true

[tools]
skills = ["web_search"]         # tools this agent can invoke

[memory]
stores = ["basic_session"]      # memory store types required

[llm]
use_instruction_llm = false     # route instruction pass through llm/instruct when true
```

Agent-specific config sections (e.g., `[docs]`, `[news.query]`) are supported and passed to the agent at runtime.

### Directory Layering

Two agent directories are scanned and merged at startup, following Unix-like conventions:

1. **System agents** — `config/agents/` (distributed with the binary)
2. **User agents** — `~/.araliya/agents/` (local overrides + custom definitions)

When a user agent ID matches a system agent, the user definition completely overrides the system one (for both prompts and runtime configuration). User agents can also use `extends` to build on core agents without copying all their configuration.

### Shared Prompt Layers

Common prompt layers live in `config/agents/_shared/`:

```
config/agents/
├── _shared/
│   ├── id.md                   — bot identity / persona (who it is)
│   ├── agent.md                — agent-level instructions (what it does)
│   ├── memory_and_tools.md     — memory access & tool guidance
│   ├── subagent.md             — (optional) subagent delegation constraints
│   └── tools.ms                — tool manifest template
├── docs/
│   ├── agent.toml
│   ├── instruct.md
│   ├── context.md
│   └── qa.md
├── news/
│   ├── agent.toml
│   └── summary.md
└── …
```

### Prompt Resolution

The `PromptBuilder.agent_layer()` method resolves prompts using a three-tier lookup:

1. User agent directory: `~/.araliya/agents/{agent_id}/{filename}`
2. System agent directory: `config/agents/{agent_id}/{filename}`
3. Shared directory: `config/agents/_shared/{filename}`

This allows users to override individual prompts without reimplementing entire agents, and enables agent inheritance through shared base configurations.

### Examples

**Minimal manifest** (e.g., `echo`, `basic-chat`):

```toml
[agent]
enabled = true
```

**Agent with tools** (e.g., `agentic-chat`):

```toml
[agent]
enabled = true

[tools]
skills = ["web_search", "docs_search"]

[memory]
stores = ["basic_session"]
```

**Extending an agent** (e.g., `docs_agent` extends `docs`):

```toml
[agent]
extends = "docs"
enabled = true
```

**User override** (e.g., `~/.araliya/agents/my-docs/agent.toml`):

```toml
[agent]
extends = "docs"

[docs]
docsdir = "~/my-project/docs"  # use custom doc directory
```

---

## Prompt Configuration

Agent prompts are co-located with their definitions under `config/agents/{agent_id}/`. Templates use `{{variable}}` syntax for interpolation (e.g. `{{items}}`, `{{docs}}`, `{{question}}`, `{{history}}`, `{{user_input}}`).

Co-locating prompts with agent definitions means behavioral policy can be updated without code changes or recompilation. Agents fall back to a minimal built-in prompt if the expected file is absent.

The `PromptBuilder` utility handles file loading, variable substitution, and tool manifest rendering via two methods:

- `layer(filename)` — load from the prompts directory (for backward compat and shared layers)
- `agent_layer(agents_dir, agent_id, filename)` — load from agent definition directory with `_shared/` fallback
- `agent_layer_with_user(agents_dir, agent_id, filename, user_agents_dir)` — check user override first, then system agent, then shared

Agents that need a preamble summarizing available tools (used by the instruction pass) use `preamble(agents_dir, tools)` to generate the standard layers from `_shared/`.

---

## Document-Backed Agents — RAG and KG-RAG

Any agent can be configured with a document store for retrieval-augmented generation. When an agent has `docsdir` set in its config section, it gains access to a `docs_search` memory tool that can be invoked during the instruction pass.

The `docs` agent is the canonical example — it is built specifically for QA over documents. But `agentic-chat` and other agents can also be configured with document stores to add project-specific knowledge to their context.

### Retrieval Paths

Two retrieval paths are supported, selected at runtime by the `use_kg` configuration flag:

**Path 1 — Full-Text Search (default)**

Requires the `idocstore` Cargo feature. The agent's document store is indexed using BM25 full-text search. At query time, a search query is formulated and the top-K matching document chunks are retrieved and injected as context.

**Path 2 — Knowledge Graph + Full-Text Search**

Requires the `ikgdocstore` Cargo feature and `use_kg = true` in config. At index time, an entity and relation graph is extracted from the documentation and stored as `kg/graph.json` in the agent's identity directory. At query time:

1. Entity names from the query are matched against the graph.
2. Matched entities become BFS seeds; the graph is traversed up to `bfs_max_depth` hops.
3. Chunk IDs associated with visited entities and relations are collected.
4. A separate FTS pass retrieves `ceil(max_chunks × fts_share)` additional passages.
5. Results are merged, ranked (KG signal + FTS score), and trimmed to `max_chunks`.
6. A knowledge graph context summary is prepended, listing seed entities and their top neighbours.
7. If the graph is absent or no seeds match, the agent falls back to pure FTS.

### Document Store Configuration

Any agent can be configured with a document store by adding these fields to its config section:

```toml
[agents.{id}]
docsdir = "docs/"           # source directory to import into the agent's document store on startup
index   = "index.md"        # fallback document when no search result is returned
use_kg  = false             # enable KG+FTS retrieval (default: false)

[agents.{id}.kg]
min_entity_mentions   = 2     # minimum occurrences for an entity to enter the graph
bfs_max_depth         = 2     # hop limit from seed entities during graph traversal
edge_weight_threshold = 0.15  # minimum relation weight to follow during BFS
max_chunks            = 8     # total chunk budget in the assembled context
fts_share             = 0.50  # fraction of max_chunks allocated to FTS results
max_seeds             = 5     # maximum BFS seed entities per query
```

Each agent's document store is stored in its own identity directory, isolated from other agents. See [Knowledge Graph Doc Store](kg_docstore.md) and [Intelligent Doc Store](intelligent_doc_store.md) for full parameter reference and indexing details.

---

## Configuration Reference

### Core Agent Settings

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.default` | string | `"basic_chat"` | Agent that handles messages with no explicit routing. Must be in `enabled` when `enabled` is non-empty. |
| `agents.enabled` | array\<string\> | `[]` | Agent IDs that are reachable via routing. An empty list means all registered agents are reachable. Set explicitly to restrict. |
| `agents.debug_logging` | bool | `false` | Write per-turn intermediate data to session KV for all agentic agents. Read via `GET /api/sessions/{id}/debug`. |

### Routing

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.routing` | map\<string, string\> | `{}` | `channel_id → agent_id` overrides. Takes priority over the default agent. |

### Per-Agent Settings

These fields appear under `[agents.{id}]` sections:

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.{id}.enabled` | bool | `true` | Set to `false` to disable this agent without removing its config section. |
| `agents.{id}.memory` | array\<string\> | `[]` | Memory store types this agent requires. Example: `["basic_session"]`. |
| `agents.{id}.skills` | array\<string\> | `[]` | Bus tools this agent may invoke. Only listed tools appear in the instruction manifest. Agents without this field cannot call any bus tools. |

### Document Store Settings

These optional fields can be added to any agent's config section (`[agents.{id}]`) to enable retrieval-augmented generation:

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.{id}.docsdir` | string | none | Source directory to import into the agent's document store on startup. When set, the agent gains access to the `docs_search` memory tool. |
| `agents.{id}.index` | string | `"index.md"` | Fallback document path (relative to `docsdir`) when no search result is returned for a query. |
| `agents.{id}.use_kg` | bool | `false` | Enable the KG+FTS retrieval path. Requires the `ikgdocstore` Cargo feature. |
| `agents.{id}.kg.min_entity_mentions` | integer | `2` | Minimum mentions for an entity to be retained in the knowledge graph. |
| `agents.{id}.kg.bfs_max_depth` | integer | `2` | BFS hop limit from seed entities during graph traversal. |
| `agents.{id}.kg.edge_weight_threshold` | float | `0.15` | Minimum relation weight to follow during BFS. |
| `agents.{id}.kg.max_chunks` | integer | `8` | Total chunk budget in the assembled retrieval context. |
| `agents.{id}.kg.fts_share` | float | `0.5` | Fraction of `max_chunks` reserved for FTS results. |
| `agents.{id}.kg.max_seeds` | integer | `5` | Maximum seed entities used for BFS per query. |

### Agentic Chat Settings

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.agentic-chat.use_instruction_llm` | bool | `false` | Route the instruction pass through `llm/instruct`. Requires `[llm.instruction]` for a separate provider; falls back to the main provider otherwise. |
| `agents.agentic-chat.docsdir` | string | none | (Optional) Source directory for document-backed context. When set, the agent can invoke `docs_search` as a memory tool. |

### Runtime Command Agent Settings

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.runtime_cmd.runtime` | string | `"bash"` | Runtime environment name. Used as the working directory under the agent's identity area. |
| `agents.runtime_cmd.command` | string | `"bash"` | Interpreter binary passed to `runtimes/exec`. |
| `agents.runtime_cmd.setup_script` | string | none | Optional shell script run once to initialize the runtime environment. |

### Web Builder Agent (`webbuilder`)

Runtime class: `agentic`. Iterative Svelte page builder — the LLM writes files and runs Node.js commands in a bounded loop until the page builds successfully.

The agent scaffolds a Vite + Svelte 5 workspace on first use, then enters an LLM-driven loop: the LLM emits structured JSON commands (`WriteFile`, `RunCmd`, `Finish`) which the agent executes. Progress events are streamed to the caller as `>>STEP<<{...}` prefixed `StreamChunk::Content` messages, allowing the frontend to show real-time build status.

Built pages are served at `GET /preview/{session_id}/{*path}` from the workspace's `dist/` directory.

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.webbuilder.max_iterations` | integer | `10` | Maximum LLM → tool → feedback cycles per request before the loop terminates. |
| `agents.webbuilder.scaffold` | string | `"vite-svelte"` | Scaffold template used when creating a new workspace. Currently only `vite-svelte` is supported. |

Requires: `plugin-webbuilder` Cargo feature + `subsystem-runtimes`.

#### Streaming Event Protocol

Events are emitted as `StreamChunk::Content` with a `>>STEP<<` sentinel prefix:

```
>>STEP<<{"type":"init","message":"Scaffolding Svelte workspace..."}
>>STEP<<{"type":"tool","tool":"node_exec","cmd":"npm install"}
>>STEP<<{"type":"cmd_result","ok":true,"stdout":"added 42 packages"}
>>STEP<<{"type":"file_write","path":"src/App.svelte"}
>>STEP<<{"type":"done","preview_url":"/preview/abc123/"}
```

Non-prefixed chunks are plain LLM text shown in chat as normal.

#### Preview Serving

- Route: `GET /preview/{session_id}/{*path}`
- Serves from: `{identity_dir}/runtimes/webbuilder-{session_id}/dist/{path}`
- SPA fallback: if the requested path does not exist, serves `index.html`
- Path traversal protection: resolved paths must stay within the workspace `dist/` directory

### News Agent Settings

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.news.query.label` | string | none | Gmail label name to filter (e.g. `n/News`). |
| `agents.news.query.n_last` | integer | none | Maximum number of recent emails to fetch. |
| `agents.news.query.t_interval` | string | none | Recency window as a duration string (e.g. `1d`, `1mon`). |
| `agents.news.query.tsec_last` | integer | none | Recency window in seconds (legacy fallback). |

### GDELT News Agent Settings

The `gdelt_news` agent fetches recent global events from the GDELT v2 BigQuery dataset (authenticated via `config/secrets/araliya-1012f47de255.json`) and summarises them with the LLM. Results are cached by content hash so identical event sets are only summarised once.

Requires the `plugin-gdelt-news-agent` Cargo feature (includes `plugin-gdelt-tool`). The summarisation prompt is loaded from `config/agents/gdelt_news/summary.md` and renders country flag emojis, event-type status emotes, and a 🚨 crisis flag for high-impact events.

Query parameters are configured under `[agents.gdelt_news.gdelt_query]`:

| Field | Type | Default | Description |
|---|---|---|---|
| `agents.gdelt_news.gdelt_query.lookback_minutes` | integer | `60` | How many minutes back to include. |
| `agents.gdelt_news.gdelt_query.limit` | integer | `50` | Maximum rows to return. |
| `agents.gdelt_news.gdelt_query.min_articles` | integer | none | Only include events covered by at least this many articles. |
| `agents.gdelt_news.gdelt_query.min_importance` | float | none | Only include events where `ABS(GoldsteinScale) >= value` (0–10 scale). |
| `agents.gdelt_news.gdelt_query.sort_by_importance` | bool | `false` | When `true`, sort results by `ABS(GoldsteinScale) DESC, NumArticles DESC` instead of `NumArticles DESC` only. |
| `agents.gdelt_news.gdelt_query.english_only` | bool | `false` | When `true`, restrict to events that have at least one English-language mention (joins `gdeltv2.eventmentions`). |

```toml
[agents]
enabled = ["gdelt_news"]

[agents.gdelt_news]
memory = ["basic_session"]

[agents.gdelt_news.gdelt_query]
lookback_minutes  = 120
limit             = 30
min_articles      = 2
min_importance    = 2.0   # only events with |GoldsteinScale| >= 2
sort_by_importance = true  # highest-impact events first
english_only      = true   # English-language sources only
```

### Example Configuration

```toml
[agents]
default       = "agentic-chat"
debug_logging = false

[agents.routing]
# pty0 = "echo"    # map a specific channel directly to an agent

[agents.chat]
memory = ["basic_session"]
skills = []   # no bus tools for plain chat

[agents.agentic-chat]
memory              = ["basic_session"]
skills              = ["gmail", "newsmail_aggregator"]
use_instruction_llm = false
# Optional: enable document-backed context for this agent
docsdir = "docs/"
index   = "index.md"
use_kg  = false

[agents.docs]
memory  = ["basic_session"]
docsdir = "docs/"
index   = "index.md"
use_kg  = false

[agents.runtime_cmd]
runtime      = "node"
command      = "node"
setup_script = "npm init -y"

[agents.webbuilder]
max_iterations = 12
scaffold       = "vite-svelte"

[agents.news.query]
label  = "n/News"
n_last = 20
```
