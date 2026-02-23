//! `docstore` — persistent, agent-scoped document + chunk index for RAG.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use crate::error::AppError;

const DOCSTORE_DIR: &str = "docstore";
const DOCS_DIR: &str = "docs";
const DB_FILENAME: &str = "chunks.db";
const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct IDocStore {
    dir: PathBuf,
    docs_dir: PathBuf,
    db_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub source: String,
    pub content: String,
    pub content_hash: String,
    pub created_at: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    pub id: String,
    pub doc_id: String,
    pub text: String,
    pub position: usize,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,
    pub doc_metadata: DocMetadata,
}

impl IDocStore {
    pub fn open(agent_identity_dir: &Path) -> Result<Self, AppError> {
        let dir = agent_identity_dir.join(DOCSTORE_DIR);
        let docs_dir = dir.join(DOCS_DIR);
        fs::create_dir_all(&docs_dir).map_err(|e| {
            AppError::Memory(format!("docstore: cannot create {}: {e}", docs_dir.display()))
        })?;

        let db_path = dir.join(DB_FILENAME);
        let store = Self {
            dir,
            docs_dir,
            db_path,
        };
        store.init_db()?;
        Ok(store)
    }

    pub fn add_document(&self, mut doc: Document) -> Result<String, AppError> {
        if doc.id.is_empty() {
            doc.id = uuid::Uuid::now_v7().to_string();
        }
        if doc.content_hash.is_empty() {
            doc.content_hash = Self::sha256_hex(&doc.content);
        }
        if doc.created_at.is_empty() {
            doc.created_at = now_iso8601();
        }

        let metadata_json = serde_json::to_string(&doc.metadata)
            .map_err(|e| AppError::Memory(format!("docstore: serialize metadata: {e}")))?;

        let mut conn = self.open_conn()?;
        if let Some(existing_id) = Self::find_doc_id_by_hash(&conn, &doc.content_hash)? {
            return Ok(existing_id);
        }

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("docstore: begin tx: {e}")))?;

        tx.execute(
            "INSERT INTO doc_metadata (doc_id, title, source, content_hash, created_at, updated_at, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                doc.id,
                doc.title,
                doc.source,
                doc.content_hash,
                doc.created_at,
                now_iso8601(),
                metadata_json,
            ],
        )
        .map_err(|e| AppError::Memory(format!("docstore: insert metadata: {e}")))?;

        tx.commit()
            .map_err(|e| AppError::Memory(format!("docstore: commit add_document: {e}")))?;

        fs::write(self.doc_content_path(&doc.id), doc.content).map_err(|e| {
            AppError::Memory(format!("docstore: write document content for {}: {e}", doc.id))
        })?;

        Ok(doc.id)
    }

    pub fn get_document(&self, doc_id: &str) -> Result<Document, AppError> {
        let mut conn = self.open_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT title, source, content_hash, created_at, metadata FROM doc_metadata WHERE doc_id = ?1",
            )
            .map_err(|e| AppError::Memory(format!("docstore: prepare get_document: {e}")))?;

        let row = stmt
            .query_row(params![doc_id], |row| {
                let metadata_json: String = row.get(4)?;
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    metadata,
                ))
            })
            .map_err(|e| AppError::Memory(format!("docstore: get_document {doc_id}: {e}")))?;

        let content = fs::read_to_string(self.doc_content_path(doc_id)).map_err(|e| {
            AppError::Memory(format!("docstore: read document content for {doc_id}: {e}"))
        })?;

        Ok(Document {
            id: doc_id.to_string(),
            title: row.0,
            source: row.1,
            content,
            content_hash: row.2,
            created_at: row.3,
            metadata: row.4,
        })
    }

    pub fn list_documents(&self) -> Result<Vec<DocMetadata>, AppError> {
        let mut conn = self.open_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT doc_id, title, source, content_hash, created_at, updated_at, metadata FROM doc_metadata ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::Memory(format!("docstore: prepare list_documents: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let metadata_json: String = row.get(6)?;
                let metadata = serde_json::from_str::<HashMap<String, String>>(&metadata_json)
                    .unwrap_or_default();
                Ok(DocMetadata {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    source: row.get(2)?,
                    content_hash: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    metadata,
                })
            })
            .map_err(|e| AppError::Memory(format!("docstore: query list_documents: {e}")))?;

        let mut docs = Vec::new();
        for row in rows {
            docs.push(
                row.map_err(|e| AppError::Memory(format!("docstore: map list_documents row: {e}")))?,
            );
        }
        Ok(docs)
    }

    pub fn delete_document(&self, doc_id: &str) -> Result<(), AppError> {
        let mut conn = self.open_conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("docstore: begin delete tx: {e}")))?;

        tx.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::Memory(format!("docstore: delete chunks for {doc_id}: {e}")))?;

        tx.execute("DELETE FROM doc_metadata WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::Memory(format!("docstore: delete metadata for {doc_id}: {e}")))?;

        tx.commit()
            .map_err(|e| AppError::Memory(format!("docstore: commit delete tx: {e}")))?;

        let content_path = self.doc_content_path(doc_id);
        if content_path.exists() {
            fs::remove_file(&content_path).map_err(|e| {
                AppError::Memory(format!("docstore: remove {}: {e}", content_path.display()))
            })?;
        }
        Ok(())
    }

    pub fn chunk_document(&self, doc_id: &str, chunk_size: usize) -> Result<Vec<Chunk>, AppError> {
        if chunk_size == 0 {
            return Err(AppError::Memory("docstore: chunk_size must be > 0".to_string()));
        }

        let content = fs::read_to_string(self.doc_content_path(doc_id)).map_err(|e| {
            AppError::Memory(format!("docstore: read document content for {doc_id}: {e}"))
        })?;

        let mut chunks = Vec::new();
        let mut start = 0usize;
        let mut current_bytes = 0usize;

        for (idx, ch) in content.char_indices() {
            current_bytes += ch.len_utf8();
            if current_bytes >= chunk_size {
                let text = content[start..idx + ch.len_utf8()].to_string();
                if !text.trim().is_empty() {
                    chunks.push(Chunk {
                        id: uuid::Uuid::now_v7().to_string(),
                        doc_id: doc_id.to_string(),
                        text,
                        position: start,
                        metadata: HashMap::new(),
                    });
                }
                start = idx + ch.len_utf8();
                current_bytes = 0;
            }
        }

        if start < content.len() {
            let text = content[start..].to_string();
            if !text.trim().is_empty() {
                chunks.push(Chunk {
                    id: uuid::Uuid::now_v7().to_string(),
                    doc_id: doc_id.to_string(),
                    text,
                    position: start,
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(chunks)
    }

    pub fn index_chunks(&self, chunks: Vec<Chunk>) -> Result<(), AppError> {
        if chunks.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("docstore: begin index tx: {e}")))?;

        let mut doc_ids = HashSet::new();
        for chunk in &chunks {
            doc_ids.insert(chunk.doc_id.clone());
        }

        for doc_id in &doc_ids {
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])
                .map_err(|e| {
                    AppError::Memory(format!("docstore: clear chunks for {doc_id} before reindex: {e}"))
                })?;
        }

        for chunk in chunks {
            let metadata_json = serde_json::to_string(&chunk.metadata)
                .map_err(|e| AppError::Memory(format!("docstore: serialize chunk metadata: {e}")))?;
            tx.execute(
                "INSERT INTO chunks (id, doc_id, text, position, metadata) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![chunk.id, chunk.doc_id, chunk.text, chunk.position as i64, metadata_json],
            )
            .map_err(|e| AppError::Memory(format!("docstore: insert chunk: {e}")))?;
        }

        tx.commit()
            .map_err(|e| AppError::Memory(format!("docstore: commit index tx: {e}")))?;
        Ok(())
    }

    pub fn search_by_text(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>, AppError> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        let conn = self.open_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    chunks.id,
                    chunks.doc_id,
                    chunks.text,
                    chunks.position,
                    chunks.metadata,
                    bm25(chunks) AS rank,
                    doc_metadata.title,
                    doc_metadata.source,
                    doc_metadata.content_hash,
                    doc_metadata.created_at,
                    doc_metadata.updated_at,
                    doc_metadata.metadata
                 FROM chunks
                 JOIN doc_metadata ON doc_metadata.doc_id = chunks.doc_id
                 WHERE chunks MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(|e| AppError::Memory(format!("docstore: prepare search_by_text: {e}")))?;

        let rows = stmt
            .query_map(params![query, top_k as i64], |row| {
                let chunk_metadata_json: String = row.get(4)?;
                let doc_metadata_json: String = row.get(11)?;

                let chunk_metadata =
                    serde_json::from_str::<HashMap<String, String>>(&chunk_metadata_json)
                        .unwrap_or_default();
                let doc_metadata =
                    serde_json::from_str::<HashMap<String, String>>(&doc_metadata_json)
                        .unwrap_or_default();

                let score = {
                    let bm25_score: f64 = row.get(5)?;
                    (-bm25_score) as f32
                };

                Ok(SearchResult {
                    chunk: Chunk {
                        id: row.get(0)?,
                        doc_id: row.get(1)?,
                        text: row.get(2)?,
                        position: row.get::<_, i64>(3)? as usize,
                        metadata: chunk_metadata,
                    },
                    score,
                    doc_metadata: DocMetadata {
                        doc_id: row.get(1)?,
                        title: row.get(6)?,
                        source: row.get(7)?,
                        content_hash: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        metadata: doc_metadata,
                    },
                })
            })
            .map_err(|e| AppError::Memory(format!("docstore: execute search_by_text: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(
                row.map_err(|e| AppError::Memory(format!("docstore: map search row: {e}")))?,
            );
        }
        Ok(results)
    }

    fn init_db(&self) -> Result<(), AppError> {
        let conn = self.open_conn()?;
        let version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .map_err(|e| AppError::Memory(format!("docstore: read schema version: {e}")))?;

        if version == 0 {
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
            .map_err(|e| AppError::Memory(format!("docstore: initialize schema: {e}")))?;
            return Ok(());
        }

        if version != SCHEMA_VERSION {
            return Err(AppError::Memory(format!(
                "docstore: unsupported schema version {version}, expected {SCHEMA_VERSION}"
            )));
        }

        Ok(())
    }

    fn open_conn(&self) -> Result<Connection, AppError> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| AppError::Memory(format!("docstore: open {}: {e}", self.db_path.display())))?;

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| AppError::Memory(format!("docstore: set journal_mode WAL: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| AppError::Memory(format!("docstore: set foreign_keys ON: {e}")))?;
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| AppError::Memory(format!("docstore: set busy_timeout: {e}")))?;

        Ok(conn)
    }

    fn doc_content_path(&self, doc_id: &str) -> PathBuf {
        self.docs_dir.join(format!("{doc_id}.txt"))
    }

    fn find_doc_id_by_hash(conn: &Connection, content_hash: &str) -> Result<Option<String>, AppError> {
        let mut stmt = conn
            .prepare("SELECT doc_id FROM doc_metadata WHERE content_hash = ?1")
            .map_err(|e| AppError::Memory(format!("docstore: prepare find by hash: {e}")))?;

        let mut rows = stmt
            .query(params![content_hash])
            .map_err(|e| AppError::Memory(format!("docstore: query find by hash: {e}")))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| AppError::Memory(format!("docstore: read find by hash row: {e}")))?
        {
            let doc_id: String = row
                .get(0)
                .map_err(|e| AppError::Memory(format!("docstore: decode find by hash row: {e}")))?;
            return Ok(Some(doc_id));
        }
        Ok(None)
    }

    fn sha256_hex(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn root_dir(&self) -> &Path {
        &self.dir
    }
}

fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, IDocStore) {
        let temp = TempDir::new().expect("tempdir");
        let identity_dir = temp.path().join("agent");
        fs::create_dir_all(&identity_dir).expect("create identity dir");
        let store = IDocStore::open(&identity_dir).expect("open docstore");
        (temp, store)
    }

    #[test]
    fn add_document_deduplicates_by_hash() {
        let (_temp, store) = make_store();
        let content = "alpha beta gamma".to_string();
        let doc_a = Document {
            id: String::new(),
            title: "A".to_string(),
            source: "unit".to_string(),
            content: content.clone(),
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        };
        let doc_b = Document {
            id: String::new(),
            title: "B".to_string(),
            source: "unit".to_string(),
            content,
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        };

        let first_id = store.add_document(doc_a).expect("insert first");
        let second_id = store.add_document(doc_b).expect("dedup second");

        assert_eq!(first_id, second_id);
        let docs = store.list_documents().expect("list docs");
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn chunk_and_search_returns_ranked_results() {
        let (_temp, store) = make_store();

        let doc = Document {
            id: String::new(),
            title: "Rust Search".to_string(),
            source: "unit".to_string(),
            content: "rust async memory store with bm25 search and chunk indexing".to_string(),
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        };
        let doc_id = store.add_document(doc).expect("add document");
        let chunks = store.chunk_document(&doc_id, 20).expect("chunk document");
        assert!(!chunks.is_empty());
        store.index_chunks(chunks).expect("index chunks");

        let results = store.search_by_text("bm25", 5).expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk.doc_id, doc_id);
    }

    #[test]
    fn delete_document_removes_metadata_chunks_and_file() {
        let (_temp, store) = make_store();
        let doc = Document {
            id: String::new(),
            title: "Delete".to_string(),
            source: "unit".to_string(),
            content: "content to delete from store".to_string(),
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        };
        let doc_id = store.add_document(doc).expect("add");
        let chunks = store.chunk_document(&doc_id, 8).expect("chunk");
        store.index_chunks(chunks).expect("index");

        store.delete_document(&doc_id).expect("delete document");
        let docs = store.list_documents().expect("list docs");
        assert!(docs.is_empty());

        let results = store.search_by_text("delete", 5).expect("search after delete");
        assert!(results.is_empty());
        assert!(!store.doc_content_path(&doc_id).exists());
    }
}
