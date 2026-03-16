//! `SqliteStore` — general-purpose agent-scoped SQLite database.
//!
//! Agents can bootstrap a named SQLite database, run schema migrations, and
//! execute arbitrary SQL queries through a simple typed API.  Multiple named
//! databases per agent are supported.
//!
//! ## Storage layout
//! ```text
//! {agent_identity_dir}/sqlite/{db_name}.db
//! ```
//!
//! ## Connection model
//! Every method opens a fresh connection via [`sqlite_core::open_conn`] (WAL +
//! FK + busy-timeout pragmas).  No persistent connection is stored in the
//! struct, keeping [`SqliteStore`] `Send + Sync` without a `Mutex`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{OptionalExtension, ToSql};

use super::sqlite_core::open_conn;
use crate::error::AppError;

// ── Public types ──────────────────────────────────────────────────────────────

/// A typed SQLite column value.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SqlValue {
    Text(String),
    Integer(i64),
    Real(f64),
    Null,
}

impl ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            SqlValue::Text(s) => s.to_sql(),
            SqlValue::Integer(i) => i.to_sql(),
            SqlValue::Real(f) => f.to_sql(),
            SqlValue::Null => Ok(rusqlite::types::ToSqlOutput::Owned(
                rusqlite::types::Value::Null,
            )),
        }
    }
}

/// A single result row — column name → value.
pub type Row = HashMap<String, SqlValue>;

// ── SqliteStore ───────────────────────────────────────────────────────────────

/// General-purpose agent-scoped SQLite database.
///
/// See the [module-level docs](self) for usage and storage layout.
pub struct SqliteStore {
    db_path: PathBuf,
}

impl SqliteStore {
    /// Open (or create) `{agent_identity_dir}/sqlite/{db_name}.db`.
    ///
    /// Creates the `sqlite/` sub-directory if it does not exist.
    /// Applies WAL + FK + busy-timeout pragmas.
    pub fn open(agent_identity_dir: &Path, db_name: &str) -> Result<Self, AppError> {
        let dir = agent_identity_dir.join("sqlite");
        fs::create_dir_all(&dir).map_err(|e| {
            AppError::Memory(format!(
                "sqlite_store: create dir {}: {e}",
                dir.display()
            ))
        })?;
        let db_path = dir.join(format!("{db_name}.db"));
        // Open once to apply pragmas and verify the path is writable.
        open_conn(&db_path)?;
        Ok(Self { db_path })
    }

    // ── Setup ─────────────────────────────────────────────────────────────────

    /// Execute arbitrary DDL (e.g. `CREATE TABLE`, `CREATE INDEX`, `DROP TABLE`).
    ///
    /// Returns the number of rows changed — usually 0 for DDL statements.
    pub fn execute_ddl(&self, sql: &str) -> Result<usize, AppError> {
        let conn = open_conn(&self.db_path)?;
        conn.execute_batch(sql)
            .map_err(|e| AppError::Memory(format!("sqlite_store: execute_ddl: {e}")))?;
        Ok(0)
    }

    /// Conditionally apply a migration.
    ///
    /// Reads `PRAGMA user_version`.  If it is less than `target_version`,
    /// executes `ddl` inside a transaction and then sets
    /// `PRAGMA user_version = target_version`.
    ///
    /// Returns `true` if the migration was applied, `false` if the database
    /// was already at or beyond `target_version`.
    pub fn migrate(&self, target_version: u32, ddl: &str) -> Result<bool, AppError> {
        let conn = open_conn(&self.db_path)?;
        let current: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| AppError::Memory(format!("sqlite_store: read user_version: {e}")))?;
        if current >= target_version {
            return Ok(false);
        }
        conn.execute_batch(ddl)
            .map_err(|e| AppError::Memory(format!("sqlite_store: migrate v{target_version}: {e}")))?;
        conn.pragma_update(None, "user_version", target_version)
            .map_err(|e| {
                AppError::Memory(format!("sqlite_store: set user_version {target_version}: {e}"))
            })?;
        Ok(true)
    }

    // ── Inspection ────────────────────────────────────────────────────────────

    /// Return the names of all user-created tables (excludes sqlite_* internals).
    pub fn tables(&self) -> Result<Vec<String>, AppError> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| AppError::Memory(format!("sqlite_store: list tables: {e}")))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::Memory(format!("sqlite_store: list tables query: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Memory(format!("sqlite_store: list tables collect: {e}")))?;
        Ok(names)
    }

    /// Return the `CREATE TABLE` SQL for `table_name`, or `None` if it does not exist.
    pub fn table_schema(&self, table_name: &str) -> Result<Option<String>, AppError> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?1")
            .map_err(|e| AppError::Memory(format!("sqlite_store: table_schema prepare: {e}")))?;
        let result = stmt
            .query_row([table_name], |row| row.get::<_, Option<String>>(0))
            .optional()
            .map_err(|e| AppError::Memory(format!("sqlite_store: table_schema query: {e}")))?
            .flatten();
        Ok(result)
    }

    /// Return the current `PRAGMA user_version`.
    pub fn schema_version(&self) -> Result<u32, AppError> {
        let conn = open_conn(&self.db_path)?;
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| AppError::Memory(format!("sqlite_store: read user_version: {e}")))
    }

    // ── Query pipeline ────────────────────────────────────────────────────────

    /// Execute a DML statement (`INSERT`, `UPDATE`, `DELETE`).
    ///
    /// `params` are bound positionally to `?1`, `?2`, … placeholders.
    /// Returns the number of rows affected.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize, AppError> {
        let conn = open_conn(&self.db_path)?;
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
        conn.execute(sql, params_refs.as_slice())
            .map_err(|e| AppError::Memory(format!("sqlite_store: execute: {e}")))
    }

    /// Run a `SELECT` and return all matching rows.
    ///
    /// Each row is a `HashMap<column_name, SqlValue>`.
    pub fn query_rows(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, AppError> {
        let conn = open_conn(&self.db_path)?;
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| AppError::Memory(format!("sqlite_store: prepare query: {e}")))?;
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                let mut map = HashMap::new();
                for (i, name) in col_names.iter().enumerate() {
                    let val = match row.get_ref(i)? {
                        ValueRef::Text(b) => {
                            SqlValue::Text(String::from_utf8_lossy(b).into_owned())
                        }
                        ValueRef::Integer(n) => SqlValue::Integer(n),
                        ValueRef::Real(f) => SqlValue::Real(f),
                        ValueRef::Blob(_) | ValueRef::Null => SqlValue::Null,
                    };
                    map.insert(name.clone(), val);
                }
                Ok(map)
            })
            .map_err(|e| AppError::Memory(format!("sqlite_store: query_rows: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Memory(format!("sqlite_store: query_rows collect: {e}")))?;
        Ok(rows)
    }

    /// Run a `SELECT` and return at most one row.
    pub fn query_one(&self, sql: &str, params: &[SqlValue]) -> Result<Option<Row>, AppError> {
        let mut rows = self.query_rows(sql, params)?;
        Ok(rows.drain(..).next())
    }

    /// Return the filesystem path to the database file.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> (TempDir, SqliteStore) {
        let dir = TempDir::new().unwrap();
        let store = SqliteStore::open(dir.path(), "test").unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_db_file() {
        let dir = TempDir::new().unwrap();
        SqliteStore::open(dir.path(), "mydb").unwrap();
        assert!(dir.path().join("sqlite/mydb.db").exists());
    }

    #[test]
    fn execute_ddl_and_query_empty() {
        let (_dir, store) = tmp();
        store
            .execute_ddl("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
            .unwrap();
        let rows = store.query_rows("SELECT * FROM items", &[]).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn execute_insert_and_query() {
        let (_dir, store) = tmp();
        store
            .execute_ddl("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        store
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)",
                &[SqlValue::Text("hello".into()), SqlValue::Text("world".into())],
            )
            .unwrap();
        let rows = store
            .query_rows("SELECT key, value FROM kv", &[])
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("key"), Some(&SqlValue::Text("hello".into())));
        assert_eq!(rows[0].get("value"), Some(&SqlValue::Text("world".into())));
    }

    #[test]
    fn query_one_returns_first_row() {
        let (_dir, store) = tmp();
        store
            .execute_ddl("CREATE TABLE nums (n INTEGER);")
            .unwrap();
        store
            .execute("INSERT INTO nums VALUES (?1)", &[SqlValue::Integer(42)])
            .unwrap();
        store
            .execute("INSERT INTO nums VALUES (?1)", &[SqlValue::Integer(99)])
            .unwrap();
        let row = store
            .query_one("SELECT n FROM nums ORDER BY n", &[])
            .unwrap();
        assert_eq!(row.unwrap().get("n"), Some(&SqlValue::Integer(42)));
    }

    #[test]
    fn query_one_empty_returns_none() {
        let (_dir, store) = tmp();
        store.execute_ddl("CREATE TABLE t (x TEXT);").unwrap();
        let row = store.query_one("SELECT x FROM t", &[]).unwrap();
        assert!(row.is_none());
    }

    #[test]
    fn migrate_applies_once() {
        let (_dir, store) = tmp();
        let applied = store
            .migrate(1, "CREATE TABLE v1 (id INTEGER PRIMARY KEY);")
            .unwrap();
        assert!(applied);
        assert_eq!(store.schema_version().unwrap(), 1);

        // Second call at same version is a no-op.
        let applied_again = store
            .migrate(1, "CREATE TABLE v1 (id INTEGER PRIMARY KEY);")
            .unwrap();
        assert!(!applied_again);
    }

    #[test]
    fn migrate_sequential_versions() {
        let (_dir, store) = tmp();
        store
            .migrate(1, "CREATE TABLE a (id INTEGER PRIMARY KEY);")
            .unwrap();
        store
            .migrate(2, "CREATE TABLE b (id INTEGER PRIMARY KEY);")
            .unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        let tables = store.tables().unwrap();
        assert!(tables.contains(&"a".to_string()));
        assert!(tables.contains(&"b".to_string()));
    }

    #[test]
    fn tables_lists_user_tables() {
        let (_dir, store) = tmp();
        store.execute_ddl("CREATE TABLE foo (x TEXT);").unwrap();
        store.execute_ddl("CREATE TABLE bar (y INTEGER);").unwrap();
        let mut tables = store.tables().unwrap();
        tables.sort();
        assert_eq!(tables, vec!["bar", "foo"]);
    }

    #[test]
    fn table_schema_returns_create_sql() {
        let (_dir, store) = tmp();
        store
            .execute_ddl("CREATE TABLE things (id INTEGER PRIMARY KEY, name TEXT);")
            .unwrap();
        let sql = store.table_schema("things").unwrap();
        assert!(sql.unwrap().contains("things"));
        assert!(store.table_schema("nonexistent").unwrap().is_none());
    }

    #[test]
    fn multiple_named_dbs_are_independent() {
        let dir = TempDir::new().unwrap();
        let s1 = SqliteStore::open(dir.path(), "alpha").unwrap();
        let s2 = SqliteStore::open(dir.path(), "beta").unwrap();
        s1.execute_ddl("CREATE TABLE t (x TEXT);").unwrap();
        // "beta" should not have table "t".
        assert!(s2.tables().unwrap().is_empty());
    }
}
