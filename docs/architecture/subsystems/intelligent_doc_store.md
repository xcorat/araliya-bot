# Intelligent Document Store (IDocStore)

**Status:** Phase 1 (2026-02-23) — Feature-gated document store · SQLite + FTS5 backend · document metadata · chunk-based indexing · BM25 text search · hash-based deduplication · agent-scoped persistence · **background `DocstoreManager`** (auto-index + orphan cleanup).

**Cargo Feature:** `idocstore`

---

## Overview

`IDocStore` is an optional, feature-gated document indexing subsystem within the memory layer that enables agents to store, chunk, and retrieve documents using full-text search. It provides a simple RAG-ready API without external dependencies beyond `rusqlite` (bundled SQLite).

Each agent can host its own document store at `{agent_identity_dir}/docstore/`, enabling:
- **Document ingestion** with automatic SHA-256 content deduplication
- **Fixed-size chunking** with byte-position tracking
- **Full-text indexing** via SQLite FTS5 virtual tables
- **BM25 ranking** for relevance-sorted search results
- **Persistent metadata** for documents and chunks

---

## Architecture

### Storage Layout

```
{agent_identity_dir}/
└── docstore/
    ├── chunks.db                  # SQLite database (metadata + FTS5 chunks)
    └── docs/
        └── {doc_id}.txt           # Raw document content on disk
```

**Storage split:**
- **Raw document content** → disk files at `{agent_identity_dir}/docstore/docs/{doc_id}.txt`
- **Document metadata** → SQLite `doc_metadata` table (title, source, content_hash, timestamps)
- **Indexed chunks** → SQLite FTS5 `chunks` virtual table (searchable text + position)

This hybrid approach keeps large documents out of the database while retaining efficient metadata/query access.

### Core Types

```rust
pub struct IDocStore {
    dir: PathBuf,
    docs_dir: PathBuf,
    db_path: PathBuf,
}

pub struct Document {
    pub id: String,                // UUIDv7
    pub title: String,
    pub source: String,            // e.g., "user", "email", "calendar"
    pub content: String,           // Full text
    pub content_hash: String,      // SHA-256 (for dedup)
    pub created_at: String,        // ISO-8601
    pub metadata: HashMap<String, String>,
}

pub struct Chunk {
    pub id: String,                // UUIDv7
    pub doc_id: String,
    pub text: String,
    pub position: usize,           // Byte offset in original doc
    pub metadata: HashMap<String, String>,
}

pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,                // BM25 score (negative, sorted ascending)
    pub doc_metadata: DocMetadata,
}
```

---

## Phase 1 API

### Initialization

```rust
let store = IDocStore::open(agent_identity_dir)?;
```

Opens or creates the document store at `{agent_identity_dir}/docstore/`. Initializes SQLite schema on first use with `PRAGMA user_version = 1` for schema versioning.

### Document Management

| Method | Signature | Behavior |
|--------|-----------|----------|
| `add_document` | `Document → Result<String>` | Insert document, return `doc_id`. Checks `content_hash` for dedup; returns existing `doc_id` if found. Auto-generates `id`, `content_hash`, `created_at` if empty. |
| `get_document` | `&str → Result<Document>` | Fetch full document by `doc_id` (loads content from disk). |
| `list_documents` | `() → Result<Vec<DocMetadata>>` | List all documents ordered by `created_at DESC`. |
| `delete_document` | `&str → Result<()>` | Delete document, all chunks, and raw file. |

### Chunking & Indexing

| Method | Signature | Behavior |
|--------|-----------|----------|
| `chunk_document` | `&str, usize → Result<Vec<Chunk>>` | Split document by byte size (deterministic, UTF-8 aware). Skips empty chunks. Returns chunks with `position` offset set. |
| `index_chunks` | `Vec<Chunk> → Result<()>` | Insert chunks into FTS5 index. Re-indexes by `doc_id` (clears old chunks for same doc). |

### Search

| Method | Signature | Behavior |
|--------|-----------|----------|
| `search_by_text` | `&str, usize → Result<Vec<SearchResult>>` | BM25 search via FTS5. Returns up to `top_k` results sorted by relevance. Empty query returns `Vec::new()`. |

---

## Database Schema

### `doc_metadata` table

```sql
CREATE TABLE doc_metadata (
    doc_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    source TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,        -- ISO-8601
    updated_at TEXT NOT NULL,        -- ISO-8601
    metadata TEXT NOT NULL           -- JSON
);
```

### `chunks` FTS5 virtual table

```sql
CREATE VIRTUAL TABLE chunks USING fts5(
    id UNINDEXED,
    doc_id UNINDEXED,
    text,                            -- Full-text indexed
    position UNINDEXED,
    metadata UNINDEXED
);
```

FTS5 provides automatic BM25 scoring; queries use `bm25(chunks)` ranking function. Queries are automatically quoted/escaped to avoid syntax errors from punctuation (e.g. `?`, `"`, parentheses).

### SQLite Pragmas

- `journal_mode = WAL` — write-ahead logging for concurrent reads
- `foreign_keys = ON` — integrity enforcement
- `busy_timeout = 5000` — 5-second contention timeout
- `user_version = 1` — schema version for migrations

---

## Integration Patterns

### Agent-Local Usage (Recommended for Phase 1)

Agents can manually initialize and use their docstore:

```rust
let docstore = IDocStore::open(&agent_identity_dir)?;

// Add document
let doc = Document {
    title: "News Summary".into(),
    source: "email".into(),
    content: email_body,
    ..Default::new()
};
let doc_id = docstore.add_document(doc)?;

// Chunk and index
let chunks = docstore.chunk_document(&doc_id, 2048)?;  // 2KB chunks
docstore.index_chunks(chunks)?;

// Search before LLM call
let results = docstore.search_by_text("climate policy", 5)?;
let context = results.iter()
    .map(|r| r.chunk.text.clone())
    .collect::<Vec<_>>()
    .join("\n\n");

// Augment prompt with retrieved context
let augmented = format!("Context:\n{}\n\nQuestion: {}", context, user_query);
let llm_response = llm.complete(&augmented).await?;
```

### Memory Subsystem Hook (Future Phase)

When agents are integrated with the memory subsystem's optional docstore API:

```rust
pub async fn open_agent_docstore(agent_id: &str, memory: &MemorySystem) 
    -> Result<IDocStore, AppError> 
{
    let agent_dir = memory.agent_identity_dir(agent_id)?;
    tokio::task::spawn_blocking(move || IDocStore::open(&agent_dir))
        .await?
}
```

---

## Deduplication Strategy

Documents are deduplicated at ingestion by `content_hash`:

1. Compute `SHA-256(content)` on `add_document` (if not provided).
2. Query `doc_metadata` for existing `content_hash`.
3. If found, return existing `doc_id` without re-inserting.
4. If new, insert metadata + write content file.

This prevents index bloat when the same document is added multiple times.

---

## Chunking Strategy

Chunks are deterministic and UTF-8-aware:

- Fixed byte-size splits (parameter: `chunk_size`).
- Iterate by `char_indices()` to respect UTF-8 boundaries.
- Store `position` (byte offset) for each chunk to enable linking back to original.
- Skip empty/whitespace-only chunks.

Example:

```rust
let chunks = docstore.chunk_document("doc-123", 2048)?;
// Returns chunks: [
//   Chunk { text: "Chapter 1...", position: 0, ... },
//   Chunk { text: "Chapter 2...", position: 2048, ... },
//   ...
// ]
```

---

## Error Handling

All operations return `Result<T, AppError>` with contextual messages:

```rust
AppError::Memory("docstore: open {path}: {io_error}")
AppError::Memory("docstore: serialize metadata: {json_error}")
AppError::Memory("docstore: execute search_by_text: {sql_error}")
```

Logging is via standard `tracing` macros in the memory subsystem.

---

## Limitations & Future Work

### Phase 1 Limitations

- **No embeddings:** BM25 only; semantic search requires Phase 2.
- **Single connection per docstore:** No connection pooling; blocking I/O only.
- **No schema migrations:** Upgrade from v1 → v2 will require manual migration code.
- **No compression:** Large documents stored uncompressed.
- **No ACL:** All agents can read/write to their own docstore only (agent_id-scoped).

### Phase 2 (Future)

- Vector embeddings via LLM embedding API
- Cosine similarity search alongside BM25
- Hybrid ranking (BM25 + semantic)
- Connection pooling for high-concurrency scenarios
- Document compression (gzip)
- Cross-agent docstore queries (with permissions)

---

## Configuration

`IDocStore` is always available when the `idocstore` Cargo feature is enabled (off by default).

```toml
# Cargo.toml
[features]
idocstore = ["dep:rusqlite"]
```

Enable in builds:

```bash
cargo build --features idocstore
cargo test --features idocstore
```

---

## Testing

Unit tests cover:

- Document deduplication by content hash
- Chunk generation and indexing
- BM25 search ranking and result recall
- Deletion cleanup (metadata, chunks, files)

Run tests:

```bash
cargo test --features idocstore docstore
```

---

## Performance Considerations

- **SQLite + FTS5:** Suitable for ~100k–1M documents per agent. WAL mode enables concurrent reads.
- **BM25 scoring:** O(log n) for indexed queries; no per-document scanning.
- **Chunk storage:** Fixed 2KB chunks recommended (~500 chunks per 1MB document).
- **Position tracking:** Enables O(1) lookup of chunk location in original.

---

## DocstoreManager

A private background task (`pub(super)`) spawned by `MemorySystem::start_docstore_manager`. Invisible outside the `memory` module.

### Responsibilities

- **Auto-index:** Every 24 hours (and on demand), scans `{memory_root}/agent/*/docstore/` for documents that have no FTS5 chunk entries and indexes them automatically at 2 KB chunk size.
- **Orphan cleanup:** Removes `docstore/docs/*.txt` files that have no matching row in `doc_metadata`.

### Startup

```rust
// Called once before Arc::new(mem):
mem.start_docstore_manager(shutdown.clone());
```

`shutdown` is the global `CancellationToken`; the manager stops cleanly when it is cancelled.

### Triggering immediate maintenance

After ingesting a document, callers inside the memory subsystem can request immediate indexing:

```rust
memory.schedule_docstore_index(agent_identity_dir);
```

This is non-blocking — work is queued and executed in the background task.

### Visibility

`DocstoreManager` is `pub(super)` — it cannot be imported by agents, subsystems, or any code outside `src/subsystems/memory/`. The two public `MemorySystem` methods (`start_docstore_manager`, `schedule_docstore_index`) are the only gateway, and both are `#[cfg(feature = "idocstore")]`.

---

## Related Documentation

- [Memory Subsystem](memory.md) — parent system providing agent identity and persistence paths
- [Agent Identity](../identity.md) — how agent directories are initialized
- [IDocStore Design Proposal](../../../notes/implementation/idocstore-design.md) — detailed design rationale

