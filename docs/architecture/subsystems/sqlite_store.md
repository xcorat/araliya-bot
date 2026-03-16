# SqliteStore

**Status:** Phase 1 (2026-03-16) — Feature-gated general-purpose SQLite backend · agent-scoped named databases · DDL setup + schema migration helpers · typed query pipeline (`SELECT` / DML) · three `LocalTool` wrappers for LLM agents · shared `sqlite_core` connection helpers reused by `IDocStore` and `IKGDocStore`.

**Cargo Feature:** `isqlite`

> **See also:** [intelligent_doc_store.md](intelligent_doc_store.md) — `IDocStore`, which uses `sqlite_core` for its FTS5 document index.
> [kg_docstore.md](kg_docstore.md) — `IKGDocStore`, which also shares `sqlite_core`.

---

## Overview

`SqliteStore` gives agents a general-purpose, agent-scoped SQLite database with a minimal API surface:

- **Bootstrap** — `open` creates the database and applies recommended pragmas.
- **Setup** — `execute_ddl` for arbitrary DDL, `migrate` for version-guarded migrations.
- **Inspection** — `tables`, `table_schema`, `schema_version` for schema introspection.
- **Query pipeline** — `execute` for DML, `query_rows` / `query_one` for typed `SELECT`.

Multiple named databases per agent are supported; each lives in its own file.

Three `LocalTool` wrappers (`SqliteQueryTool`, `SqliteExecuteTool`, `SqliteSchemaTool`) expose the store to LLM agents via the `AgenticLoop` tool system, enabling agents to manage and query structured data as part of their reasoning loop.

The underlying `sqlite_core` module provides the connection factory, pragmas, and shared document-store types that `IDocStore` and `IKGDocStore` also depend on.

---

## Storage Layout

```
{agent_identity_dir}/
└── sqlite/
    ├── {db_name}.db         one file per named database
    └── …
```

Multiple databases are independent — each is a separate SQLite file. No shared schema is imposed; agents own the schema entirely.

---

## Architecture

### `sqlite_core` — shared helpers

`sqlite_core` is the internal foundation for all three SQLite-backed stores. It lives at `src/subsystems/memory/stores/sqlite_core.rs` and is compiled when **any** of `isqlite`, `idocstore`, or `ikgdocstore` is enabled (the latter two depend on `isqlite`).

| Item | Description |
|------|-------------|
| `open_conn(path)` | Opens a SQLite connection with WAL + FK + busy-timeout pragmas. |
| `sha256_hex(content)` | SHA-256 hex digest — dedup fingerprint for `IDocStore`. |
| `now_iso8601()` | Current UTC time as RFC 3339 string. |
| `escape_fts5_query(query)` | Token-level quoting for FTS5 `MATCH` expressions. |
| `init_schema(conn)` | Creates `doc_metadata` + `chunks` FTS5 tables (docstore-specific). |
| `Document`, `DocMetadata`, `Chunk`, `SearchResult` | Shared types for `IDocStore` / `IKGDocStore`. |

`SqliteStore` uses only `open_conn` from this module; the schema and typed types are its own.

### Connection model

Every `SqliteStore` method opens a fresh connection via `open_conn` for each call. No persistent connection is stored in the struct, keeping `SqliteStore` `Send + Sync` without a `Mutex`. This is intentional for Phase 1 — connection reuse is a potential Phase 2 optimisation.

---

## API

### Initialization

```rust
let store = state.open_sqlite_store(agent_id, "data")?;
// or directly:
let store = SqliteStore::open(agent_identity_dir, "data")?;
```

Opens or creates `{agent_identity_dir}/sqlite/{db_name}.db`. Creates the `sqlite/` sub-directory if needed. Applies WAL + FK + busy-timeout pragmas.

### Setup

| Method | Signature | Behaviour |
|--------|-----------|-----------|
| `execute_ddl` | `&str → Result<usize>` | Execute arbitrary DDL (`CREATE TABLE`, `DROP TABLE`, `CREATE INDEX`, …). Returns 0 (DDL has no row count). |
| `migrate` | `u32, &str → Result<bool>` | Read `PRAGMA user_version`. If below `target_version`, execute the DDL inside a transaction and set `user_version = target_version`. Returns `true` if the migration was applied, `false` if already at or beyond the target. |

Typical bootstrap pattern:

```rust
let store = state.open_sqlite_store(agent_id, "tracker")?;
store.migrate(1, "CREATE TABLE events (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, ts TEXT NOT NULL);")?;
store.migrate(2, "ALTER TABLE events ADD COLUMN payload TEXT;")?;
```

### Inspection

| Method | Signature | Behaviour |
|--------|-----------|-----------|
| `tables` | `() → Result<Vec<String>>` | Names of all user tables (excludes `sqlite_*` internal objects), ordered alphabetically. |
| `table_schema` | `&str → Result<Option<String>>` | `CREATE TABLE` SQL for `table_name`, or `None` if absent. |
| `schema_version` | `() → Result<u32>` | Current `PRAGMA user_version`. |
| `db_path` | `() → &Path` | Filesystem path to the `.db` file. |

### Query Pipeline

| Method | Signature | Behaviour |
|--------|-----------|-----------|
| `execute` | `&str, &[SqlValue] → Result<usize>` | Execute `INSERT`, `UPDATE`, or `DELETE`. Params are bound to `?1`, `?2`, … placeholders. Returns rows affected. |
| `query_rows` | `&str, &[SqlValue] → Result<Vec<Row>>` | Run `SELECT`; returns all matching rows as `Vec<HashMap<column_name, SqlValue>>`. |
| `query_one` | `&str, &[SqlValue] → Result<Option<Row>>` | As `query_rows` but returns at most the first row. |

---

## Type System

### `SqlValue`

A typed column value for binding and result mapping:

```rust
pub enum SqlValue {
    Text(String),
    Integer(i64),
    Real(f64),
    Null,
}
```

`SqlValue` implements `rusqlite::ToSql` for binding and is deserialized from `rusqlite::types::ValueRef` for result rows. BLOB columns are mapped to `Null` (unsupported in Phase 1).

### `Row`

```rust
pub type Row = HashMap<String, SqlValue>;
```

Column names are the string keys. The order of columns matches the `SELECT` projection.

---

## LocalTool Wrappers

Three `LocalTool` implementations expose the store to the `AgenticLoop`. They share a single `Arc<SqliteStore>` and are registered together:

```rust
use std::sync::Arc;
use araliya_bot::subsystems::agents::sqlite_tool::{SqliteQueryTool, SqliteExecuteTool, SqliteSchemaTool};

let store = Arc::new(state.open_sqlite_store(agent_id, "data")?);
let tools: Vec<Box<dyn LocalTool>> = vec![
    Box::new(SqliteQueryTool::new(Arc::clone(&store))),
    Box::new(SqliteExecuteTool::new(Arc::clone(&store))),
    Box::new(SqliteSchemaTool::new(Arc::clone(&store))),
];
```

### `SqliteQueryTool`

| | |
|--|--|
| **Action name** | `sqlite_query` |
| **Input** | `{ "sql": "SELECT ...", "params": ["value1", 42] }` |
| **Output** | JSON array of row objects |

Runs a `SELECT` and returns all matching rows serialized as a JSON array. `params` is optional.

### `SqliteExecuteTool`

| | |
|--|--|
| **Action name** | `sqlite_execute` |
| **Input** | `{ "sql": "INSERT/UPDATE/DELETE ...", "params": [...] }` |
| **Output** | `{ "rows_affected": N }` |

Executes a DML statement and returns the row count.

### `SqliteSchemaTool`

| | |
|--|--|
| **Action name** | `sqlite_schema` |
| **Input** | `{}` |
| **Output** | `{ "tables": ["t1", …], "schemas": { "t1": "CREATE TABLE t1 …", … } }` |

Lists all user tables and returns the `CREATE TABLE` SQL for each. Useful for priming the LLM with the current schema before issuing queries.

---

## Integration with `AgentsState`

`AgentsState::open_sqlite_store(agent_id, db_name)` is the preferred entrypoint — it resolves the agent's identity directory automatically:

```rust
#[cfg(feature = "isqlite")]
pub fn open_sqlite_store(&self, agent_id: &str, db_name: &str)
    -> Result<SqliteStore, AppError>;
```

This call is synchronous (blocking I/O). Wrap in `tokio::task::spawn_blocking` when called from an async context.

---

## Cargo Feature

`isqlite` is the base feature that enables `SqliteStore` and `sqlite_core`. `IDocStore` and `IKGDocStore` depend on it via their own feature declarations:

```toml
[features]
isqlite   = ["dep:rusqlite"]
idocstore = ["isqlite", "dep:text-splitter"]
ikgdocstore = ["isqlite", "dep:text-splitter"]
```

Enabling any of `idocstore`, `ikgdocstore`, or `isqlite` makes `SqliteStore` available.

```bash
cargo build --features isqlite
cargo test  --features isqlite
```

---

## Error Handling

All methods return `Result<T, AppError>` with contextual messages prefixed `"sqlite_store: …"`:

```
AppError::Memory("sqlite_store: create dir {path}: {io_error}")
AppError::Memory("sqlite_store: execute_ddl: {sql_error}")
AppError::Memory("sqlite_store: migrate v{N}: {sql_error}")
AppError::Memory("sqlite_store: prepare query: {sql_error}")
AppError::Memory("sqlite_store: query_rows: {sql_error}")
```

---

## Testing

Unit tests are in `#[cfg(test)]` at the bottom of `sqlite_store.rs`. Each test uses a `tempfile::TempDir` — no writes to `~/.araliya`.

| Test | Covers |
|------|--------|
| `open_creates_db_file` | Database file created at correct path. |
| `execute_ddl_and_query_empty` | DDL applied; empty table returns zero rows. |
| `execute_insert_and_query` | Insert + SELECT round-trip with typed values. |
| `query_one_returns_first_row` | First row of ordered result returned. |
| `query_one_empty_returns_none` | `None` returned for empty table. |
| `migrate_applies_once` | Migration skipped on second call at same version. |
| `migrate_sequential_versions` | Two sequential migrations; both tables present. |
| `tables_lists_user_tables` | Only user tables listed; `sqlite_*` excluded. |
| `table_schema_returns_create_sql` | DDL returned for known table; `None` for unknown. |
| `multiple_named_dbs_are_independent` | Two named databases do not share tables. |

Run:

```bash
cargo test --features isqlite sqlite_store
```

---

## Limitations & Future Work

### Phase 1 Limitations

- **No connection pooling:** a fresh connection is opened per call.
- **No BLOB support:** BLOB columns are returned as `SqlValue::Null`.
- **No transaction API:** multi-statement transactions must be expressed as a single DDL batch in `execute_ddl`.
- **No query builder:** SQL strings are passed verbatim; the caller is responsible for correctness.

### Phase 2 (Future)

- Persistent connection or a lightweight pool for high-frequency workloads.
- BLOB support (`SqlValue::Blob(Vec<u8>)`).
- Explicit `begin_transaction` / `commit` / `rollback` helpers.
- `execute_many` for batched DML.

---

## Related Documentation

- [intelligent_doc_store.md](intelligent_doc_store.md) — `IDocStore` (uses `sqlite_core`)
- [kg_docstore.md](kg_docstore.md) — `IKGDocStore` (uses `sqlite_core`)
- [memory.md](memory.md) — Memory subsystem and agent identity directories
- [agents.md](agents.md) — `AgentsState::open_sqlite_store` and the `AgenticLoop` tool system
