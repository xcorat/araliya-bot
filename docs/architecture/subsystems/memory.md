# Memory Subsystem

**Status:** v0.2.0-alpha — typed value model (`PrimaryValue`, `Obj`, `Value`, `Doc`, `Block`, `Collection`) · `Store` struct (labeled collection map) · `TmpStore` (ephemeral in-process store) · `SessionStore` trait · `BasicSessionStore` · `SessionRw` data ops layer · `SessionHandle` with `tmp_doc`/`tmp_block` accessors · **`SessionSpend` — per-session token and cost tracking in `spend.json`** · **optional `SqliteStore` (`isqlite` Cargo feature) for general-purpose agent-scoped SQLite databases** · **optional `IDocStore` (`idocstore` Cargo feature) for BM25 document retrieval** · **optional `IKGDocStore` (`ikgdocstore` Cargo feature) for KG-augmented RAG retrieval**.

---

## Overview

The Memory subsystem owns all session data for the bot instance.  It provides:

- A **typed value model** for structured, hashable agent memory.
- Two concrete **collection types** (`Doc` for scalars, `Block` for rich payloads).
- A **`TmpStore`** — ephemeral in-process storage backed by the new `Store` struct, ideal for scratch pads and default sessions.
- A **`BasicSessionStore`** — disk-backed JSON + Markdown transcript store for durable sessions.
- A **`SessionHandle`** — async-safe handle agents use to read and write session state, with direct typed accessors for `TmpStore` sessions.

Session access is **not bus-mediated** — agents receive a `SessionHandle` directly from `AgentsState.memory` rather than routing through bus messages.

Read-only KG queries **are** bus-accessible via `MemoryBusHandler` (`subsystems/memory_bus.rs`), which registers the `memory/` prefix on the supervisor bus. This decouples HTTP layer KG reads from agent internals. Currently exposed methods:

| Method | Description |
|--------|-------------|
| `memory/kg_graph` | Return knowledge graph JSON for an agent's `kgdocstore/`. Returns `{"agent_id", "graph"}`. Empty graph when no data yet. |
| `memory/status` | Health status (convention requirement). |

`IDocStore` is currently **agent-scoped storage infrastructure** in the memory subsystem; active agent prompt augmentation is a follow-up phase.

---

## Architecture

```
MemorySystem (owns session index + store factory)
    │
    ├── create_session(store_types, agent_id) → SessionHandle
    ├── load_session(session_id, agent_id)   → SessionHandle
    └── create_tmp_store()                   → Arc<TmpStore> (standalone)
            │
            └── SessionHandle (Arc-wrapped, cloneable, async-safe)
                    │
                    ├── stores: Vec<Arc<dyn SessionStore>>    ← kv / transcript I/O
                    └── tmp_store: Option<Arc<TmpStore>>      ← typed Doc/Block access
```

### Key types

| Type | Role |
|------|------|
| `MemorySystem` | Session lifecycle: create, load, list. Maintains `sessions.json` index. |
| `SessionStore` | Trait for pluggable session backends. |
| `Store` | In-process `RwLock<HashMap<String, Collection>>` — the core collection map. |
| `SessionRw` | Shared session read/write orchestration layer. |
| `SessionHandle` | Thin facade that delegates all data I/O to `SessionRw`; also owns spend accumulation. |
| `BasicSessionStore` | Capped JSON k-v + capped Markdown transcript, disk-backed. |
| `TmpStore` | Ephemeral in-process store wrapping a `Store`. Implements `SessionStore`. |
| `Doc` | String-keyed map of `PrimaryValue` scalars. |
| `Block` | String-keyed map of `Value` (scalars + binary `Obj`). |
| `Collection` | Enum: `Doc`, `Block`, and stubs for future variants. |

---

## Type System

### `PrimaryValue`

Scalar values suitable for indexing, hashing, and equality:

```rust
enum PrimaryValue { Bool(bool), Int(i64), Float(f64), Str(String) }
```

### `Obj`

Binary payload with a string-keyed metadata sidecar (MIME type, content hash, etc.):

```rust
struct Obj { pub data: Vec<u8>, pub metadata: HashMap<String, String> }
```

### `Value`

Union type for `Block` entries — either a scalar or an object:

```rust
enum Value { Primary(PrimaryValue), Obj(Obj) }
```

### `Doc` and `Block`

| | `Doc` | `Block` |
|--|-------|---------|
| Entry type | `PrimaryValue` | `Value` |
| Use for | Config, extracted facts, session metadata | Blobs, embeddings, intermediate results |

---

## Store Abstractions

### `SessionStore` trait

Pluggable backend for session-scoped I/O.  All methods are default-no-op; implementations override only what they support.

### `Store` struct

An in-process labeled collection map, safe for concurrent reads.

---

## TmpStore

`TmpStore` wraps a `Store` and provides two usage modes:

### Standalone (agent scratch pad)

`create_tmp_store()` always returns a fresh, independent store not tracked in the session index.

### Session-backed

When a session is created with `store_type = "tmp"`, `TmpStore` implements `SessionStore` using per-session namespaced collection labels.

---

## Session Lifecycle

Sessions are **bot-scoped** — any agent with the session ID can access it.

1. **Create:** `MemorySystem::create_session(&["tmp"], "chat")` — or `&["basic_session"]` for disk persistence.
2. **Use:** Returns a `SessionHandle` for k-v and transcript operations.
3. **Load:** `MemorySystem::load_session(session_id, "chat")` re-opens an existing session.
4. **Tmp sessions** can be reloaded within the same process run (data is in-process); they do not survive restart.

Session IDs are UUIDv7 (time-ordered).  The `sessions.json` index tracks all sessions including tmp ones.

## Next phases

- Introduce `AgentHandle` for agent-scoped memory roots (`memory/agents/{agent_id}/`) while keeping session handles for conversation-scoped state.
- Agent identity model for primary agents: `hash(prv:pub, id.md|{json})`.
- Allow multiple sessions per primary agent identity.
- Treat subagents as delegated workers without a unique persistent identity.

---

## Data Layout (disk-backed sessions)

```
{identity_dir}/
└── memory/
    ├── sessions.json              session index (includes spend summary)
    └── sessions/
        └── {uuid}/                only created for non-tmp sessions
            ├── kv.json            capped key-value store
            ├── transcript.md      capped Markdown transcript
            └── spend.json         aggregate token and cost totals (created on first LLM turn)
```

## Data Layout (agent-scoped SQLite databases, optional)

When built with feature `isqlite`, agents can host one or more named SQLite databases under their identity root:

```
{agent_identity_dir}/
└── sqlite/
    ├── {db_name}.db             one file per named database
    └── …
```

Each database is schema-free from the platform's perspective — the agent owns the DDL entirely. Setup helpers (`execute_ddl`, `migrate`) and a typed query pipeline (`execute`, `query_rows`, `query_one`) are provided by `SqliteStore`. Three `LocalTool` wrappers (`sqlite_query`, `sqlite_execute`, `sqlite_schema`) expose it to LLM agents via the `AgenticLoop`.

## Data Layout (agent-scoped docstore, optional)

When built with feature `idocstore`, agents can host an indexed document store under their identity root:

```
{agent_identity_dir}/
└── docstore/
    ├── chunks.db                  SQLite + FTS5 (`doc_metadata`, `chunks`)
    └── docs/
        └── {doc_id}.txt           raw document payload
```

Phase 1 APIs include document add/list/get/delete, smart Markdown-aware chunking (`text-splitter`), indexing, hash-based deduplication (`SHA-256`), and BM25 text search.

## Data Layout (agent-scoped KG docstore, optional)

When built with feature `ikgdocstore`, agents can additionally host a knowledge-graph augmented store.  Both stores can coexist in the same identity directory without conflict — they write to separate sub-directories.

```
{agent_identity_dir}/
└── kgdocstore/
    ├── chunks.db                  SQLite + FTS5 (same schema as IDocStore)
    ├── docs/
    │   └── {doc_id}.txt           raw document payload
    └── kg/
        ├── entities.json          extracted entities (id, name, kind, mention_count)
        ├── relations.json         extracted relations (from, to, label, weight)
        └── graph.json             combined graph — fast-load file for query time
```

The KG is built offline via `rebuild_kg()` after documents are indexed, and consulted at query time by `search_with_kg()`.  Falls back to pure FTS when the graph is absent or no seed entities match the query.

### `spend.json` shape

```json
{
  "total_input_tokens": 1240,
  "total_output_tokens": 380,
  "total_cached_tokens": 0,
  "total_cost_usd": 0.000694,
  "last_updated": "2026-02-21T10:59:42Z"
}
```

The file is created on the first LLM turn that carries token usage. `sessions.json` mirrors the latest totals in `SessionInfo.spend` so aggregate spend can be queried without opening individual sidecar files.

---

## SessionHandle (async API)

String-based k-v and transcript operations (work for both `basic_session` and `tmp` sessions):

```rust
pub async fn kv_get(&self, key: &str)               -> Result<Option<String>, AppError>;
pub async fn kv_set(&self, key: &str, value: &str)  -> Result<(), AppError>;
pub async fn kv_delete(&self, key: &str)             -> Result<bool, AppError>;
pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError>;
pub async fn transcript_read_last(&self, n: usize)  -> Result<Vec<TranscriptEntry>, AppError>;
pub async fn working_memory_read(&self)              -> Result<String, AppError>;
pub async fn list_files(&self)                       -> Result<Vec<SessionFileInfo>, AppError>;
```

Spend accumulation (disk-backed sessions only; no-op for tmp sessions without a directory):

```rust
pub async fn accumulate_spend(
    &self,
    usage: &LlmUsage,
    rates: &ModelRates,
) -> Result<SessionSpend, AppError>;
```

Reads `spend.json`, adds the new token counts, recomputes the incremental cost, writes back, and returns the updated totals.

Typed accessors for `tmp` sessions (synchronous — no file I/O):

```rust
pub fn tmp_doc(&self)                  -> Result<Doc, AppError>;    // snapshot clone
pub fn tmp_block(&self)                -> Result<Block, AppError>;  // snapshot clone
pub fn set_tmp_doc(&self, doc: Doc)    -> Result<(), AppError>;     // write back
pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError>;  // write back
```

These return `Err` for sessions without a `TmpStore` (i.e. `basic_session` sessions).

---

## Agent Integration

`SessionChatPlugin` demonstrates memory integration:

1. On first message, creates a session via `state.memory.create_session(store_types, "chat")`.
2. Appends user input as a `"user"` transcript entry.
3. Reads the last 20 transcript entries and injects them as LLM context.
4. Appends the LLM response as an `"assistant"` transcript entry.
5. If the response carries token `usage`, calls `handle.accumulate_spend(usage, &state.llm_rates)` to update `spend.json`.
6. The session handle is cached in `Arc<Mutex<Option<SessionHandle>>>` for reuse.

`state.llm_rates` is populated at startup by `AgentsSubsystem::with_llm_rates(rates)` using pricing values from `[llm.openai]` config.

```toml
[agents.chat]
memory = ["tmp"]   # use ephemeral in-process storage instead
```

---

## Config

```toml
[memory.basic_session]
# kv_cap = 200         # max key-value entries per session (default: 200)
# transcript_cap = 500 # max transcript entries per session (default: 500)

[agents.chat]
memory = ["basic_session"]  # store types this agent uses
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum k-v entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |
| `agents.{id}.memory` | array\<string\> | `[]` | Store types (`"basic_session"` or `"tmp"`). |

Core memory is always compiled. `SqliteStore` is behind the `isqlite` Cargo feature; `IDocStore` behind `idocstore` (implies `isqlite`); `IKGDocStore` behind `ikgdocstore` (implies `isqlite`).

---

## Related Documentation

- [sqlite_store.md](sqlite_store.md) — SqliteStore API, migration helpers, LocalTool wrappers
- [intelligent_doc_store.md](intelligent_doc_store.md) — IDocStore API, schema, integration patterns
- [kg_docstore.md](kg_docstore.md) — IKGDocStore API, KG build/query pipeline, configuration
- [../../identity.md](../../identity.md) — agent identity provisioning and directory layout
- [../../standards/index.md](../../standards/index.md) — bus protocol and subsystem patterns

---

## Future

- **Default store type "tmp":** when `agents.{id}.memory` is empty, automatically use `"tmp"` instead of returning an error.
- **Observation store:** structured facts, summaries, reflections (JSONL or SQLite).
- **Cross-session search:** full-text or embedding-based retrieval across sessions.
- **Session expiry:** TTL-based cleanup of old sessions.
- **Mirror spend → sessions.json:** after `accumulate_spend`, update `SessionInfo.spend` in the index so listings include live totals without opening sidecars.

