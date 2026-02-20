# Memory Subsystem

**Status:** v0.3.0 — `MemorySystem` · `Store` trait · `BasicSessionStore` · `SessionHandle` · bot-scoped sessions with UUIDv7 IDs.

---

## Overview

The Memory subsystem owns all persistent session data for the bot instance. It provides a pluggable `Store` trait, a session lifecycle manager (`MemorySystem`), and an async-safe handle (`SessionHandle`) that agents use to read and write session state.

Memory is **not bus-mediated** — agents receive a `SessionHandle` directly from `AgentsState.memory` rather than routing through bus messages. This avoids unnecessary serialisation overhead for high-frequency read/write operations.

---

## Architecture

```
MemorySystem (owns session index + store factory)
    │
    ├── create_session(store_types, agent_id) → SessionHandle
    └── load_session(session_id, agent_id) → SessionHandle
            │
            └── SessionHandle (Arc-wrapped, cloneable, async-safe)
                    │
                    └── Store (trait object — e.g. BasicSessionStore)
                            │
                            ├── kv.json      capped key-value entries
                            └── transcript.md  capped Markdown transcript
```

### Key types

| Type | Location | Role |
|------|----------|------|
| `MemorySystem` | `subsystems/memory/mod.rs` | Session lifecycle: create, load, list. Maintains `sessions.json` index. |
| `Store` | `subsystems/memory/store.rs` | Trait for pluggable stores. Default methods return `AppError::Memory("unsupported")`. |
| `SessionHandle` | `subsystems/memory/handle.rs` | Async-safe wrapper. Wraps sync `Store` I/O in `tokio::task::spawn_blocking`. |
| `BasicSessionStore` | `subsystems/memory/stores/basic_session.rs` | Reference implementation: capped JSON k-v + capped Markdown transcript. |

---

## Session Lifecycle

Sessions are **bot-scoped** — each session belongs to one bot identity and one agent.

1. **Create:** `MemorySystem::create_session(&["basic_session"], "chat")` allocates a UUIDv7 session directory and registers it in `sessions.json`.
2. **Use:** Returns a `SessionHandle` for k-v and transcript operations.
3. **Load:** `MemorySystem::load_session(session_id, "chat")` re-opens an existing session.
4. **Ephemeral by default:** The current `SessionChatPlugin` creates a new session per process run. Persistent session resumption is possible via `load_session`.

Session IDs are UUIDv7 (time-ordered), providing natural chronological sorting.

---

## Data Layout

```
{identity_dir}/
└── memory/
    ├── sessions.json              session index (id, agent_id, store_types, created_at)
    └── sessions/
        └── {uuid}/
            ├── kv.json            capped key-value store
            └── transcript.md      capped Markdown transcript
```

### `sessions.json`

```json
{
  "sessions": [
    {
      "id": "01969c3a-...",
      "agent_id": "chat",
      "store_types": ["basic_session"],
      "created_at": "2025-04-15T10:30:00Z"
    }
  ]
}
```

### `kv.json`

```json
{
  "cap": 200,
  "entries": [
    { "key": "user_name", "value": "sachi", "ts": "2025-04-15T10:30:01Z" },
    { "key": "topic", "value": "rust memory systems", "ts": "2025-04-15T10:31:00Z" }
  ]
}
```

Entries are FIFO-evicted when count exceeds `cap`. Keys are unique — setting an existing key updates in-place (moves to end).

### `transcript.md`

```markdown
### user — 2025-04-15T10:30:01Z

Hello, tell me about your memory system.

### assistant — 2025-04-15T10:30:05Z

I can store key-value pairs and maintain a conversation transcript…
```

Entries are parsed by the `### {role} — {timestamp}` header pattern. FIFO-evicted when entry count exceeds `transcript_cap`.

---

## Store Trait

```rust
pub trait Store: Send + Sync {
    fn kv_get(&self, key: &str) -> Result<Option<String>, AppError> { /* default: unsupported */ }
    fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError> { /* default: unsupported */ }
    fn kv_delete(&self, key: &str) -> Result<bool, AppError> { /* default: unsupported */ }
    fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError> { /* default: unsupported */ }
    fn transcript_read_last(&self, n: usize) -> Result<Vec<TranscriptEntry>, AppError> { /* default: unsupported */ }
}
```

Stores implement only the operations they support. The default methods return `AppError::Memory("unsupported")`, so a store that only provides k-v can skip transcript methods.

### `BasicSessionStore`

The reference store implementation. Supports both k-v and transcript operations using flat files (`kv.json` + `transcript.md`). All I/O is synchronous (file read/write); async wrapping is handled by `SessionHandle`.

---

## SessionHandle (async API)

Agents interact with memory through `SessionHandle`, which wraps sync `Store` calls in `tokio::task::spawn_blocking`:

```rust
pub async fn kv_get(&self, key: &str) -> Result<Option<String>, AppError>;
pub async fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError>;
pub async fn kv_delete(&self, key: &str) -> Result<bool, AppError>;
pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError>;
pub async fn transcript_read_last(&self, n: usize) -> Result<Vec<TranscriptEntry>, AppError>;
pub async fn working_memory_read(&self) -> Result<String, AppError>;
pub async fn list_files(&self) -> Result<Vec<SessionFileInfo>, AppError>;
```

`SessionHandle` is `Clone + Send + Sync` — safe to share across tasks.

`working_memory_read()` currently reads the `working_memory` key from the session k-v store (empty string when not set). `list_files()` enumerates files in the session directory and returns name, size, and ISO-8601 modified timestamp metadata.

---

## Agent Integration

The `SessionChatPlugin` demonstrates memory integration:

1. On first message, creates a session via `state.memory.create_session(store_types, "chat")`.
2. Appends user input as a `"user"` transcript entry.
3. Reads the last 20 transcript entries and injects them as context for the LLM.
4. Appends the LLM response as an `"assistant"` transcript entry.
5. The session handle is cached in an `Arc<Mutex<Option<SessionHandle>>>` for reuse.

When the `subsystem-memory` feature is disabled, `SessionChatPlugin` falls back to stateless `ChatCore::basic_complete`.

---

## Config

```toml
[memory]
# Global memory settings (currently empty, reserved for future use)

[memory.basic_session]
# kv_cap = 200         # max key-value entries (default: 200)
# transcript_cap = 500 # max transcript entries (default: 500)

[agents.chat]
memory = ["basic_session"]  # store types this agent uses
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory.basic_session.kv_cap` | usize | 200 | Maximum k-v entries before FIFO eviction. |
| `memory.basic_session.transcript_cap` | usize | 500 | Maximum transcript entries before FIFO eviction. |
| `agents.{id}.memory` | array\<string\> | `[]` | Store types the agent requires. |

---

## Feature Gate

The memory subsystem is behind the `subsystem-memory` Cargo feature. The `plugin-chat` feature implies `subsystem-memory`.

```toml
[features]
subsystem-memory = []
plugin-chat = ["subsystem-agents", "subsystem-memory"]
```

---

## Future

- **Observation store:** structured facts, summaries, reflections (JSONL or SQLite)
- **Usage tracking:** token counts, cost estimates per session
- **Cross-session search:** full-text or embedding-based retrieval across sessions
- **Session expiry:** TTL-based cleanup of old sessions
