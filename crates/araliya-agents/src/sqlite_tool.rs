//! SQLite [`LocalTool`] wrappers for LLM agents.
//!
//! Three tools are provided, each backed by a [`SqliteStore`]:
//!
//! | Tool | Purpose |
//! |---|---|
//! | [`SqliteQueryTool`] | `SELECT` — returns rows as a JSON array |
//! | [`SqliteExecuteTool`] | `INSERT` / `UPDATE` / `DELETE` — returns rows affected |
//! | [`SqliteSchemaTool`] | Inspect tables and their `CREATE TABLE` SQL |
//!
//! ## Usage
//!
//! Construct the tools with a [`SqliteStore`] reference and register them with
//! an [`AgenticLoop`] tool set.  The store is shared across all three via
//! `Arc` so they can be constructed independently.
//!
//! ```rust,ignore
//! let store = Arc::new(state.open_sqlite_store("my-agent", "data")?);
//! let tools: Vec<Box<dyn LocalTool>> = vec![
//!     Box::new(SqliteQueryTool::new(Arc::clone(&store))),
//!     Box::new(SqliteExecuteTool::new(Arc::clone(&store))),
//!     Box::new(SqliteSchemaTool::new(Arc::clone(&store))),
//! ];
//! ```

use std::sync::Arc;

use super::core::agentic::LocalTool;
use araliya_memory::stores::sqlite_store::{SqlValue, SqliteStore};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse an optional `"params"` JSON array into `Vec<SqlValue>`.
fn parse_params(value: &serde_json::Value) -> Vec<SqlValue> {
    value
        .get("params")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => SqlValue::Text(s.clone()),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            SqlValue::Integer(i)
                        } else if let Some(f) = n.as_f64() {
                            SqlValue::Real(f)
                        } else {
                            SqlValue::Null
                        }
                    }
                    serde_json::Value::Null => SqlValue::Null,
                    _ => SqlValue::Null,
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── SqliteQueryTool ───────────────────────────────────────────────────────────

/// Run a `SELECT` query and return all matching rows as a JSON array.
///
/// **Input schema**
/// ```json
/// { "sql": "SELECT ...", "params": ["value1", 42] }
/// ```
/// `params` is optional.
pub struct SqliteQueryTool {
    store: Arc<SqliteStore>,
}

impl SqliteQueryTool {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }
}

impl LocalTool for SqliteQueryTool {
    fn name(&self) -> &str {
        "sqlite_query"
    }

    fn description(&self) -> &str {
        "action: \"sqlite_query\", params: {\"sql\": \"SELECT ...\", \"params\": [...]}\n  \
         Description: Execute a SELECT query against the agent's SQLite database. \
         Returns a JSON array of row objects. \"params\" is optional positional binding."
    }

    fn call(&self, params: &serde_json::Value) -> Result<String, String> {
        let sql = params
            .get("sql")
            .and_then(|s| s.as_str())
            .ok_or_else(|| "sqlite_query: missing \"sql\" field".to_string())?;
        let bind = parse_params(params);
        let rows = self
            .store
            .query_rows(sql, &bind)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&rows).map_err(|e| format!("sqlite_query: serialize: {e}"))
    }
}

// ── SqliteExecuteTool ─────────────────────────────────────────────────────────

/// Execute a DML statement (`INSERT`, `UPDATE`, `DELETE`) and return the
/// number of rows affected.
///
/// **Input schema**
/// ```json
/// { "sql": "INSERT INTO ...", "params": ["value1", 42] }
/// ```
pub struct SqliteExecuteTool {
    store: Arc<SqliteStore>,
}

impl SqliteExecuteTool {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }
}

impl LocalTool for SqliteExecuteTool {
    fn name(&self) -> &str {
        "sqlite_execute"
    }

    fn description(&self) -> &str {
        "action: \"sqlite_execute\", params: {\"sql\": \"INSERT/UPDATE/DELETE ...\", \"params\": [...]}\n  \
         Description: Execute an INSERT, UPDATE, or DELETE statement against the agent's SQLite \
         database. Returns {\"rows_affected\": N}."
    }

    fn call(&self, params: &serde_json::Value) -> Result<String, String> {
        let sql = params
            .get("sql")
            .and_then(|s| s.as_str())
            .ok_or_else(|| "sqlite_execute: missing \"sql\" field".to_string())?;
        let bind = parse_params(params);
        let n = self.store.execute(sql, &bind).map_err(|e| e.to_string())?;
        Ok(format!("{{\"rows_affected\":{n}}}"))
    }
}

// ── SqliteSchemaTool ──────────────────────────────────────────────────────────

/// Inspect the database schema — lists all tables and returns their
/// `CREATE TABLE` SQL.
///
/// **Input schema**
/// ```json
/// {}
/// ```
/// No parameters required.
pub struct SqliteSchemaTool {
    store: Arc<SqliteStore>,
}

impl SqliteSchemaTool {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }
}

impl LocalTool for SqliteSchemaTool {
    fn name(&self) -> &str {
        "sqlite_schema"
    }

    fn description(&self) -> &str {
        "action: \"sqlite_schema\", params: {}\n  \
         Description: Return the schema of the agent's SQLite database: a list of table names \
         and the CREATE TABLE SQL for each."
    }

    fn call(&self, _params: &serde_json::Value) -> Result<String, String> {
        let tables = self.store.tables().map_err(|e| e.to_string())?;
        let mut schemas: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::with_capacity(tables.len());
        for table in &tables {
            let sql = self
                .store
                .table_schema(table)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            schemas.insert(table.clone(), serde_json::Value::String(sql));
        }
        let out = serde_json::json!({
            "tables": tables,
            "schemas": schemas,
        });
        serde_json::to_string(&out).map_err(|e| format!("sqlite_schema: serialize: {e}"))
    }
}
