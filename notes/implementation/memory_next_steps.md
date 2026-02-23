# Memory subsystem — next steps / history

## Completed

### IDocStore Phase 1 (2026-02-22)

- Added feature-gated document store module `subsystems/memory/stores/docstore.rs` behind Cargo feature `idocstore`.
- Added SQLite (`rusqlite`) schema bootstrap with `PRAGMA user_version` and FTS5 `chunks` virtual table.
- Implemented Phase 1 APIs: add/get/list/delete documents, fixed-size chunking, chunk indexing, BM25 text search.
- Added SHA-256 content-hash deduplication at ingestion; duplicate inserts return existing `doc_id`.
- Added unit tests for dedup, chunk/index/search flow, and delete cleanup.

### BasicSessionStore migration to collection model (v0.4.1)

- **`kv.json` format changed** from v1 `{ "cap": N, "entries": [{key,value,ts},…] }` to v2
  `{ "cap": N, "order": [keys], "values": {key: value} }`.  The flat `values` map is the
  direct serialisation of a `Doc` collection.  `order` preserves insertion order for FIFO
  eviction.  Old v1 files are auto-migrated to v2 on the first read.
- **`transcript.md` unchanged** on disk (human-readable Markdown); now also exposed as a
  `Block` collection via `read_transcript_block()` (each entry keyed by padded index, value
  is `Value::Obj { data: content_bytes, metadata: { role, ts } }`).
- **`SessionStore` trait** gained two optional methods with default error impls:
  `read_kv_doc(&Path) -> Doc` and `read_transcript_block(&Path) -> Block`.
- **`BasicSessionStore`** overrides both.  **`TmpStore`** overrides `read_kv_doc` (returns
  the session-scoped doc collection directly).
- **`SessionHandle`** gained `kv_doc()` and `transcript_block()` async helpers that
  `spawn_blocking` to the default store's typed view methods.
- Tests: 3 new tests — `kv_as_doc`, `kv_migrate_v1_format`, `transcript_as_block` (92 total).

## Planned work

- Expand `Collection` implementations: `Set`, `List` (as simple ordered-set wrappers).
- Add higher-level APIs on `MemorySystem` for per-agent defaults and permissions.
- Add integration tests that exercise cross-store semantics (basic_session vs tmp parity).
- Future: structured transcript format (JSON on disk) for machine-readable replay.

## Owner

Core infra team.
