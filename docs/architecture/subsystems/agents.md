# Agents Subsystem

**Status:** v0.5.0 — `Agent` trait (with `session_id`) · `AgentsState` capability boundary · `BusHandler` impl · agent dispatch · **`ChatCore` composition layer** · `SessionChatPlugin` with memory integration and session reload · session query handlers (`agents/sessions`, `agents/sessions/detail`, `agents/sessions/memory`, `agents/sessions/files`) · **`DocsAgentPlugin` with optional KG-RAG path (`IKGDocStore`) and externalised prompt templates**.

---

## Overview

The Agents subsystem receives agent-targeted requests from the supervisor bus and routes each message to an agent. Agent handlers are non-blocking: each handler receives ownership of `reply_tx` and resolves it in its own time — synchronously for simple agents, via `tokio::spawn` for agents that perform I/O.

---

## Agents

| Agent | Behaviour |
|-------|-----------|
| `basic_chat` | Calls `ChatCore::basic_complete` → `llm/complete` on the bus. |
| `chat` | Session-aware chat via `SessionChatPlugin`. Creates or reloads a memory session (via `session_id`), appends user/assistant turns to a Markdown transcript, and injects recent history as LLM context. Returns `session_id` in the reply. Default agent. Configured with `memory = ["basic_session"]`. |
| `news` | Calls `tools/execute` with `newsmail_aggregator/get` and returns the raw tool payload as comms content. |
| `docs` | Retrieves context from the agent's document store and answers questions with the LLM.  Uses `IKGDocStore` (KG+FTS) when `use_kg = true` is set in config and the `ikgdocstore` feature is compiled; falls back to plain `IDocStore` (FTS only) otherwise. |
| `echo` | Returns the input unchanged. Used as safety fallback when `enabled` is empty. |

---

## Routing

Agents are resolved in this priority order:

1. Explicit `{agent_id}` from the method path
2. Channel mapping: `channel_id → agent_id` in `[agents.routing]`
3. Default agent: first entry in `agents.enabled` (falls back to `echo` if `enabled` is empty)

---

## Method Grammar

- `agents` — default agent, default action
- `agents/{agent_id}` — explicit agent, default action
- `agents/{agent_id}/{action}` — explicit agent + action (`{action}` accepted but not yet differentiated)

---

## Handle Request Contract

`AgentsSubsystem` implements `BusHandler` with prefix `"agents"`. The supervisor
calls `handle_request` and returns immediately:

- **`echo`** — `EchoAgent::handle` resolves `reply_tx` inline; zero latency.
- **`basic_chat`** — `BasicChatPlugin::handle` moves `reply_tx` into a
  `tokio::spawn`ed task that calls `AgentsState::complete_via_llm`.
- **`chat`** — `SessionChatPlugin::handle` spawns a task that initialises a
  memory session on first use, appends to transcript, builds context, calls
  `ChatCore::basic_complete`, and appends the LLM reply.

---

## basic_chat Flow

```
handle_request("agents", CommsMessage { channel_id, content }, reply_tx)
  → resolve agent → "basic_chat"
  → tokio::spawn {
      bus.request("llm/complete", LlmRequest { channel_id, content }).await
        → LlmSubsystem.handle_request → DummyProvider::complete
        ← Ok(CommsMessage { channel_id, content: "[echo] {input}" })
      reply_tx.send(Ok(CommsMessage { .. }))
    }
```

---

## Agent Architecture

`Agent` is the extension trait for all agent implementations:

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

Agents are stored in a `HashMap<String, Box<dyn Agent>>` inside
`AgentsSubsystem`. Resolution order (by `id()`) maps to the routing priority
table above.

### Agent and Subagent Identities

Each registered agent is provisioned with its own cryptographic identity (`ed25519` keypair) during subsystem initialization. These identities are stored in `AgentsState::agent_identities` and persisted under `{memory_root}/agent/{agent_id}-{public_id}/`.

Agents can also spawn **subagents** — ephemeral or task-specific workers that operate under their parent's identity structure. Subagents are provisioned via `AgentsState::get_or_create_subagent(agent_id, subagent_name)`, which creates a nested identity at `{memory_root}/agent/{agent_id}-{public_id}/subagents/{subagent_name}-{public_id}/`.

> **Naming convention:** `Agent` for autonomous actors in the agents subsystem;
> `Plugin` is reserved for capability extensions in the future tools subsystem.

### Chat-family composition (`ChatCore`)

Chat-family agents (`basic_chat`, `chat`, and future variants) share logic
through composition rather than inheritance:

```
src/subsystems/agents/chat/
├── mod.rs           # feature-gated re-exports
├── core.rs          # ChatCore — shared building blocks
├── basic_chat.rs    # BasicChatPlugin (thin wrapper over ChatCore)
└── session_chat.rs  # SessionChatPlugin (ChatCore + future extensions)
```

`ChatCore` is a stateless struct providing composable methods:

```rust
impl ChatCore {
    pub async fn basic_complete(state, channel_id, content) -> BusResult;
    // Future: prompt_template(), inject_memory(), tool_dispatch(), ...
}
```

Each chat agent calls `ChatCore` methods and layers its own behaviour on top.
This avoids code duplication while allowing progressive enhancement:

```
ChatCore::basic_complete()        ← shared logic
    ↑                    ↑
BasicChatPlugin     SessionChatPlugin  (core + session/memory/tools)
                         ↑
                  AdvancedChatPlugin   (future — further extensions)
```

### Capability boundary — `AgentsState`

Agents receive `Arc<AgentsState>`, not a raw `BusHandle`. Available methods:

| Method | Description |
|--------|-------------|
| `complete_via_llm(channel_id, content)` | Forward to `llm/complete` on the bus; return `BusResult`. |
| `memory` (field) | `Arc<MemorySystem>` — create/load sessions. In builds that include `subsystem-agents`, memory is available to agents directly. |
| `agent_memory` (field) | `HashMap<String, Vec<String>>` — per-agent memory store requirements from config. |

The raw bus is private to `AgentsState`. Agents cannot call arbitrary bus
targets.

## Session queries

The agents subsystem intercepts session query bus methods before agent routing:

| Method | Payload | Response |
|--------|---------|----------|
| `agents/sessions` | `Empty` | `JsonResponse` — JSON array of all sessions (id, created_at, store_types, last_agent) |
| `agents/sessions/detail` | `SessionQuery { session_id }` | `JsonResponse` — session metadata + full transcript |
| `agents/sessions/memory` | `SessionQuery { session_id }` | `JsonResponse` — `{ session_id, content }`, where `content` is current working memory |
| `agents/sessions/files` | `SessionQuery { session_id }` | `JsonResponse` — `{ session_id, files[] }` with `name`, `size_bytes`, `modified` |

These are handled directly by `AgentsSubsystem` (not routed to individual agents).

## Initialisation

`AgentsSubsystem::new(config: AgentsConfig, bus: BusHandle, memory: Arc<MemorySystem>)` — the `BusHandle`
is injected at init and wrapped inside `AgentsState`. Built-in agents
(`EchoAgent`, `BasicChatPlugin`, `SessionChatPlugin`) are registered behind
Cargo feature gates; the `enabled` list controls which ones are reachable via
routing.

## Next phases

- Primary agents will have a stable identity value derived from key material and identity payload, modeled as `hash(prv:pub, id.md|{json})`.
- A single primary agent identity can own multiple sessions concurrently.
- A subagent is a delegated worker without its own unique persistent identity; it executes under a parent agent context.

---


## Prompt Configuration

Agent prompts are now externalized as plain text files in `config/prompts/`. Each agent loads its default prompt template from a corresponding file (e.g., `news_summary.txt`, `docs_qa.txt`, `chat_context.txt`).

This approach improves maintainability and security (see [identity.md](../identity.md)), and allows prompt updates without code changes. Templates support variable interpolation using `{{variable}}` syntax (e.g., `{{items}}`, `{{docs}}`, `{{question}}`, `{{history}}`, `{{user_input}}`).

Example directory structure:

```
config/
  prompts/
    news_summary.txt
    docs_qa.txt
    chat_context.txt
```

Agents will fallback to a minimal built-in prompt if the file is missing.

```toml
[agents]
default = "chat"

[agents.routing]
# pty0 = "echo"

[agents.chat]
memory = ["basic_session"]
```

## Docs Agent — RAG & KG-RAG

The `docs` agent supports two retrieval paths, selected at runtime by the `use_kg` config flag:

### Path 1 — FTS only (`use_kg = false`, default)

Uses `IDocStore` (feature `idocstore`).  BM25 full-text search over indexed chunks returns the top-K passages which are injected into the LLM prompt.

### Path 2 — KG+FTS (`use_kg = true`, requires feature `ikgdocstore`)

Uses `IKGDocStore` (feature `ikgdocstore`).  At query time:

1. Load `kg/graph.json` built by the last `rebuild_kg` call.
2. Match entity names from the query against the graph.
3. BFS-traverse from matched seeds up to `bfs_max_depth` hops.
4. Collect chunk IDs from visited entities and relations.
5. Also run FTS for `ceil(max_chunks × fts_share)` passages.
6. Merge, rank (KG bonus + FTS bonus), trim to `max_chunks`.
7. Prepend a `## Knowledge Graph Context` summary (seed entities + their top neighbours).
8. Falls back to pure FTS if the graph is absent or no seeds match.

### Docs agent config

```toml
[agents.docs]
use_kg = true          # enable KG path (default: false)

[agents.docs.kg]
min_entity_mentions   = 2
bfs_max_depth         = 2
edge_weight_threshold = 0.15
max_chunks            = 8
fts_share             = 0.50
max_seeds             = 5
```

See [kg_docstore.md](kg_docstore.md) for full parameter reference.

### Future

- **Embeddings / vector store** — semantic search alongside BM25 and KG for a three-signal ranking.
- **Incremental KG update** — re-extract only changed/new chunks instead of full rebuild.
- **Cross-agent queries** — shared KG across agents of the same identity group.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `agents.default` | string | `"chat"` | Which agent handles unrouted messages. |
| `agents.routing` | map\<string,string\> | `{}` | Optional `channel_id → agent_id` routing overrides. |
| `agents.{id}.enabled` | bool | `true` | Set to `false` to disable without removing the section. |
| `agents.{id}.memory` | array\<string\> | `[]` | Memory store types this agent requires (e.g. `["basic_session"]`). |
| `agents.docs.path` | string | `docs/quick-intro.md` | Fallback Markdown file when KG is disabled. |
| `agents.docs.use_kg` | bool | `false` | Enable the KG+FTS retrieval path via `IKGDocStore`. Requires feature `ikgdocstore`. |
| `agents.docs.kg.min_entity_mentions` | usize | `2` | Minimum mentions for an entity to survive the KG filter. |
| `agents.docs.kg.bfs_max_depth` | usize | `2` | BFS hop limit from seed entities. |
| `agents.docs.kg.edge_weight_threshold` | f32 | `0.15` | Minimum edge weight to follow during BFS. |
| `agents.docs.kg.max_chunks` | usize | `8` | Total chunk budget in the assembled context. |
| `agents.docs.kg.fts_share` | f32 | `0.5` | Fraction of `max_chunks` reserved for FTS results. |
| `agents.docs.kg.max_seeds` | usize | `5` | Maximum seed entities used for BFS. |
