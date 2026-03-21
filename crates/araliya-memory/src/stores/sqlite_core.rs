//! `sqlite_core` — shared SQLite helpers used by all document stores.
//!
//! [`IDocStore`](super::docstore::IDocStore),
//! [`IKGDocStore`](super::kg_docstore::IKGDocStore), and
//! [`SqliteStore`](super::sqlite_store::SqliteStore) all rely on this module.
//!
//! ## What lives here
//! - **Schema constants** — `DB_FILENAME`, `SCHEMA_VERSION`, `init_schema` (docstore-specific).
//! - **Connection factory** — `open_conn` (WAL + foreign-keys + busy timeout).
//! - **Utilities** — `sha256_hex`, `now_iso8601`, `escape_fts5_query`.
//! - **Shared public types** — `Document`, `DocMetadata`, `Chunk`, `SearchResult`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use araliya_core::error::AppError;

// ── Schema ────────────────────────────────────────────────────────────────────

/// SQLite database file name used by all document stores.
pub const DB_FILENAME: &str = "chunks.db";

/// Schema version stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// Execute the v1 schema DDL on a freshly-opened SQLite connection.
pub fn init_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS doc_metadata (
            doc_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            source TEXT NOT NULL,
            content_hash TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            metadata TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks USING fts5(
            id UNINDEXED,
            doc_id UNINDEXED,
            text,
            position UNINDEXED,
            metadata UNINDEXED
        );

        PRAGMA user_version = 1;
        ",
    )
    .map_err(|e| AppError::Memory(format!("docstore: initialize schema: {e}")))
}

// ── Connection helper ─────────────────────────────────────────────────────────

/// Open a SQLite connection to `db_path` and apply recommended pragmas.
pub fn open_conn(db_path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::Memory(format!("docstore: open {}: {e}", db_path.display())))?;

    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| AppError::Memory(format!("docstore: set journal_mode WAL: {e}")))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| AppError::Memory(format!("docstore: set foreign_keys ON: {e}")))?;
    conn.pragma_update(None, "busy_timeout", 5000)
        .map_err(|e| AppError::Memory(format!("docstore: set busy_timeout: {e}")))?;

    Ok(conn)
}

// ── Utility functions ─────────────────────────────────────────────────────────

/// Return the lowercase hex-encoded SHA-256 digest of `content`.
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Return the current UTC time as an RFC 3339 string with second precision.
pub fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Escape a user-supplied string for use in an FTS5 `MATCH` query.
pub fn escape_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|tok| {
            if tok.chars().all(|c| c.is_alphanumeric()) {
                tok.to_string()
            } else {
                let escaped = tok.replace('"', "\"\"");
                format!("\"{}\"", escaped)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Shared public types ───────────────────────────────────────────────────────

/// A document as stored and retrieved by the document store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Document {
    /// Unique document identifier (UUID v7).
    pub id: String,
    pub title: String,
    /// Free-form origin tag (file path, URL, …).
    pub source: String,
    /// Full raw text of the document.
    pub content: String,
    /// SHA-256 hex digest of `content` — used to detect duplicates.
    pub content_hash: String,
    /// ISO 8601 timestamp of first insertion.
    pub created_at: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Lightweight document descriptor stored in `doc_metadata` (no `content`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocMetadata {
    pub doc_id: String,
    pub title: String,
    pub source: String,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// A single text chunk produced by the Markdown splitter.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    /// Unique chunk identifier (UUID v7).
    pub id: String,
    /// ID of the parent `Document`.
    pub doc_id: String,
    /// Raw text of this chunk.
    pub text: String,
    /// Byte offset of this chunk in the original document.
    pub position: usize,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// A single FTS result: the matched chunk, its relevance score, and its parent
/// document metadata.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk: Chunk,
    /// Relevance score (higher = more relevant).
    pub score: f32,
    pub doc_metadata: DocMetadata,
}
