# Memory Subsystem

**Status:** v0.5.0 — typed value model (`PrimaryValue`, `Obj`, `Value`, `Doc`, `Block`, `Collection`) · `Store` struct (labeled collection map) · `TmpStore` (ephemeral in-process store) · `SessionStore` trait · `BasicSessionStore` · `SessionRw` data ops layer · `SessionHandle` with `tmp_doc`/`tmp_block` accessors · **`SessionSpend` — per-session token and cost tracking in `spend.json`**.

---

## Overview

The Memory subsystem owns all session data for the bot instance.  It provides:

- A **typed value model** for structured, hashable agent memory.
- Two concrete **collection types** (`Doc` for scalars, `Block` for rich payloads).
- A **`TmpStore`** — ephemeral in-process storage backed by the new `Store` struct, ideal for scratch pads and default sessions.
- A **`BasicSessionStore`** — disk-backed JSON + Markdown transcript store for durable sessions.
- A **`SessionHandle`** — async-safe handle agents use to read and write session state, with direct typed accessors for `TmpStore` sessions.

Memory is **not bus-mediated** — agents receive a `SessionHandle` directly from `AgentsState.memory` rather than routing through bus messages. `subsystem-memory` remains a Cargo feature at product level; when agents are enabled, memory is available directly in agent code.

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

| Type | Location | Role |
|------|----------|------|
| `MemorySystem` | `memory/mod.rs` | Session lifecycle: create, load, list. Maintains `sessions.json` index. |
| `SessionStore` | `memory/store.rs` | Trait for pluggable session backends. |
| `Store` | `memory/store.rs` | In-process `RwLock<HashMap<String, Collection>>` — the core collection map. |
| `SessionRw` | `memory/rw.rs` | Shared session read/write orchestration layer (kv, transcript, file listing, tmp collections). |
| `SessionHandle` | `memory/handle.rs` | Thin facade that delegates all data I/O to `SessionRw`; also owns spend accumulation. |
| `SessionInfo` | `memory/mod.rs` | Session metadata persisted in `sessions.json`; includes an optional `spend` summary. |
| `SessionSpend` | `memory/mod.rs` | Aggregate token counts and cumulative cost; persisted as `spend.json`. |
| `BasicSessionStore` | `memory/stores/basic_session.rs` | Capped JSON k-v + capped Markdown transcript, disk-backed. |
| `TmpStore` | `memory/stores/tmp.rs` | Ephemeral in-process store wrapping a `Store`. Implements `SessionStore`. |
| `Doc` | `memory/collections.rs` | String-keyed map of `PrimaryValue` scalars. |
| `Block` | `memory/collections.rs` | String-keyed map of `Value` (scalars + binary `Obj`). |
| `Collection` | `memory/collections.rs` | Enum: `Doc`, `Block`, and stubs for future variants. |
| `PrimaryValue` | `memory/types.rs` | `Bool` · `Int` · `Float` · `Str` — hashable, equatable. |
| `Value` | `memory/types.rs` | `Primary(PrimaryValue)` or `Obj(Obj)`. |
| `Obj` | `memory/types.rs` | Binary payload with `HashMap<String, String>` metadata sidecar. |

---

## Type System

### `PrimaryValue`

Scalar values suitable for indexing, hashing, and equality:

```rust
enum PrimaryValue { Bool(bool), Int(i64), Float(f64), Str(String) }
```

`Float` equality and hashing use bit patterns (`f64::to_bits()`).  `From` impls for all primitive types.

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
| Methods | `get`, `set`, `delete`, `keys`, `len`, `is_empty` | same |

### `Collection`

Enum wrapping all collection types.  `Doc` and `Block` are fully implemented.  `Set`, `List`, `Vec`, `Tuple`, `Tensor` are **stubs** that compile but `unimplemented!()` on access — reserved namespace, not silently wrong.

```rust
enum Collection { Doc(Doc), Block(Block), Set(()), List(()), Vec(()), Tuple(()), Tensor(()) }
```

Use `as_doc()` / `as_doc_mut()` / `into_doc()` / `as_block()` / ... to downcast.

---

## Store Abstractions

### `SessionStore` trait

Pluggable backend for session-scoped I/O.  All methods are default-no-op (return `AppError::Memory("unsupported")`); implementations override only what they support.

```rust
pub trait SessionStore: Send + Sync {
    fn store_type(&self) -> &str;
    fn init(&self, session_dir: &Path) -> Result<(), AppError>;
    fn kv_get(&self, session_dir: &Path, key: &str)   -> Result<Option<String>, AppError>;
    fn kv_set(&self, session_dir: &Path, key: &str, value: &str) -> Result<(), AppError>;
    fn kv_delete(&self, session_dir: &Path, key: &str) -> Result<bool, AppError>;
    fn transcript_append(&self, ...)  -> Result<(), AppError>;
    fn transcript_read_last(&self, ...) -> Result<Vec<TranscriptEntry>, AppError>;
}
```

### `Store` struct

An in-process labeled collection map, safe for concurrent reads:

```rust
let store = Store::new();
store.insert_collection("meta".into(), Collection::Doc(Doc::default()))?;
let col = store.get_collection("meta")?.unwrap(); // returns a clone
```

Operations: `get_collection`, `insert_collection`, `remove_collection`, `labels`, `len`, `is_empty`.

---

## TmpStore

`TmpStore` wraps a `Store` and provides two usage modes:

### Standalone (agent scratch pad)

```rust
let ts: Arc<TmpStore> = memory.create_tmp_store();
let mut doc = ts.doc()?;
doc.set("status".into(), PrimaryValue::from("active"));
ts.set_doc(doc)?;
```

`create_tmp_store()` always returns a fresh, independent store not tracked in the session index.

### Session-backed

When a session is created with `store_type = "tmp"`, `TmpStore` implements `SessionStore` using per-session namespaced collection labels (`"{session_dir}:doc"`, `"{session_dir}:block"`).  `kv_get`/`kv_set`/`kv_delete` delegate to the `"doc"` collection, serialising values as `PrimaryValue::Str`.

`init()` is a no-op — no files are written to disk.

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

Memory is always compiled — there is no Cargo feature gate.

---

## Future

- **Default store type "tmp":** when `agents.{id}.memory` is empty, automatically use `"tmp"` instead of returning an error.
- **Observation store:** structured facts, summaries, reflections (JSONL or SQLite).
- **Cross-session search:** full-text or embedding-based retrieval across sessions.
- **Session expiry:** TTL-based cleanup of old sessions.
- **Mirror spend → sessions.json:** after `accumulate_spend`, update `SessionInfo.spend` in the index so listings include live totals without opening sidecars.

