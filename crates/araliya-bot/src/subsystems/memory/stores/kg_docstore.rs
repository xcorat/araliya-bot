//! `kg_docstore` — intelligent knowledge-graph document store.
//!
//! `IKGDocStore` is a self-contained document store with the same base API as
//! [`IDocStore`](super::docstore::IDocStore) (add / chunk / index / FTS search)
//! **plus** a KG layer that extracts entities and relations from the indexed
//! chunks, persists them under `kgdocstore/kg/`, and uses them to augment
//! retrieval at query time.
//!
//! ## Storage layout
//! ```text
//! {agent_identity_dir}/
//! └── kgdocstore/
//!     ├── chunks.db          # SQLite — same schema as IDocStore
//!     ├── docs/              # raw document content files
//!     └── kg/
//!         ├── entities.json
//!         ├── relations.json
//!         └── graph.json     # combined, used for fast in-memory load
//! ```
//!
//! ## Build vs query split
//! - **Build** (`rebuild_kg`): reads every chunk from `chunks.db`, runs
//!   entity + relation extraction, writes `kg/`.  Run offline after import.
//! - **Query** (`search_with_kg`): loads `kg/graph.json`, matches prompt
//!   entities, BFS-traverses the graph, merges with FTS results, assembles
//!   the context string for the LLM.  Falls back to pure FTS if no graph
//!   is present.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::params;
use text_splitter::MarkdownSplitter;
use tracing::warn;

use crate::error::AppError;

use super::docstore_core::{
    DB_FILENAME, SCHEMA_VERSION, escape_fts5_query, init_schema, now_iso8601, open_conn,
    sha256_hex,
};

// Re-export the shared types so callers can use a single import.
pub use super::docstore_core::{Chunk, DocMetadata, Document, SearchResult};

// ── Storage constants ─────────────────────────────────────────────────────────

const KGDOCSTORE_DIR: &str = "kgdocstore";
const DOCS_DIR: &str = "docs";
const KG_DIR: &str = "kg";
const ENTITIES_FILE: &str = "entities.json";
const RELATIONS_FILE: &str = "relations.json";
const GRAPH_FILE: &str = "graph.json";

// ── KG Types ──────────────────────────────────────────────────────────────────

/// Semantic category assigned to each extracted entity.
///
/// The category influences how the entity is displayed in the KG summary and
/// can be used downstream for filtering or specialised prompting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// An abstract idea or principle (Title Case noun phrase).
    Concept,
    /// A named software system or component (CamelCase identifier).
    System,
    /// A named person.
    Person,
    /// A quoted or backtick-delimited technical term.
    Term,
    /// A 2–5 letter ALL-CAPS abbreviation.
    Acronym,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EntityKind::Concept => "concept",
            EntityKind::System => "system",
            EntityKind::Person => "person",
            EntityKind::Term => "term",
            EntityKind::Acronym => "acronym",
        };
        write!(f, "{s}")
    }
}

/// A node in the knowledge graph.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    /// Stable 16-hex-char ID derived from `sha256(name)`.
    pub id: String,
    /// Normalised (lowercase) canonical name.
    pub name: String,
    pub kind: EntityKind,
    /// Total number of times this entity was mentioned across all chunks.
    pub mention_count: usize,
    /// IDs of chunks that mention this entity (used to map back to text).
    pub source_chunks: Vec<String>,
}

/// A directed edge in the knowledge graph.
///
/// Edges are normalised: `weight` is in `(0, 1]` where 1.0 is the most
/// frequent co-occurrence pair.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Relation {
    /// `Entity::id` of the source node.
    pub from: String,
    /// `Entity::id` of the target node.
    pub to: String,
    /// Human-readable edge type, e.g. `"uses"`, `"implements"`, `"co-occurs"`.
    pub label: String,
    /// Normalised co-occurrence weight in `(0, 1]`.
    pub weight: f32,
    /// Chunk IDs that triggered this relation.
    pub source_chunks: Vec<String>,
}

/// The full in-memory knowledge graph: entity nodes and relation edges.
///
/// Serialised to `kg/graph.json` after each `rebuild_kg` call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KgGraph {
    /// Map of `entity_id -> Entity`.
    pub entities: HashMap<String, Entity>,
    pub relations: Vec<Relation>,
}

impl KgGraph {
    /// Construct an empty graph (used when no chunks are indexed yet).
    fn empty() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
        }
    }

    /// `true` when no entities have been extracted (graph not yet built or empty corpus).
    fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// Tuning parameters for the KG build and query phases.
#[derive(Debug, Clone)]
pub struct KgConfig {
    /// Minimum mention count to keep an entity (default 2).
    pub min_entity_mentions: usize,
    /// Maximum BFS depth from seed entities (default 2).
    pub bfs_max_depth: usize,
    /// Minimum edge weight to follow during BFS (default 0.15).
    pub edge_weight_threshold: f32,
    /// Total chunk budget returned in context (default 8).
    pub max_chunks: usize,
    /// Fraction of `max_chunks` reserved for FTS results (default 0.5).
    pub fts_share: f32,
    /// Maximum number of seed entities used for BFS (default 5).
    pub max_seeds: usize,
}

impl Default for KgConfig {
    fn default() -> Self {
        Self {
            min_entity_mentions: 2,
            bfs_max_depth: 2,
            edge_weight_threshold: 0.15,
            max_chunks: 8,
            fts_share: 0.5,
            max_seeds: 5,
        }
    }
}

/// Result returned by `search_with_kg`.
#[derive(Debug, Clone)]
pub struct KgSearchResult {
    /// Assembled context string: KG summary + ranked passages.
    pub context: String,
    /// `true` if the KG graph was used; `false` for pure FTS fallback.
    pub used_kg: bool,
    /// Entity names matched from the prompt (empty if used_kg is false).
    pub seed_entities: Vec<String>,
}

// ── IKGDocStore ───────────────────────────────────────────────────────────────

/// Intelligent Knowledge-Graph Document Store.
///
/// Wraps the same base document/chunk/FTS functionality as `IDocStore` and
/// adds an offline KG build phase (`rebuild_kg`) and an augmented query path
/// (`search_with_kg`).
///
/// Each instance is tied to one agent's identity directory.  Multiple agents
/// can each have their own `IKGDocStore` without conflict.  An `IDocStore` and
/// an `IKGDocStore` can coexist in the same identity directory because they
/// write to separate sub-directories (`docstore/` vs `kgdocstore/`).
#[derive(Debug, Clone)]
pub struct IKGDocStore {
    /// Root of this store's on-disk layout (`{agent_identity_dir}/kgdocstore/`).
    dir: PathBuf,
    /// Raw document content files (`dir/docs/`).
    docs_dir: PathBuf,
    /// KG JSON files (`dir/kg/`).
    kg_dir: PathBuf,
    /// Path to `chunks.db`.
    db_path: PathBuf,
}

impl IKGDocStore {
    // ── Lifecycle ─────────────────────────────────────────────────────────

    /// Open (or create) the store rooted at `{agent_identity_dir}/kgdocstore/`.
    ///
    /// Creates the `docs/` and `kg/` sub-directories if they do not exist, then
    /// initialises (or validates) the SQLite schema.  Safe to call repeatedly.
    pub fn open(agent_identity_dir: &Path) -> Result<Self, AppError> {
        let dir = agent_identity_dir.join(KGDOCSTORE_DIR);
        let docs_dir = dir.join(DOCS_DIR);
        let kg_dir = dir.join(KG_DIR);
        fs::create_dir_all(&docs_dir).map_err(|e| {
            AppError::Memory(format!("kgdocstore: cannot create {}: {e}", docs_dir.display()))
        })?;
        fs::create_dir_all(&kg_dir).map_err(|e| {
            AppError::Memory(format!("kgdocstore: cannot create {}: {e}", kg_dir.display()))
        })?;

        let db_path = dir.join(DB_FILENAME);
        let store = Self { dir, docs_dir, kg_dir, db_path };
        store.init_db()?;
        Ok(store)
    }

    /// Return the root directory of this store instance.
    pub fn root_dir(&self) -> &Path {
        &self.dir
    }

    // ── Document management ───────────────────────────────────────────────

    /// Insert a document into the store and return its ID.
    ///
    /// If a document with the same `content_hash` already exists the call
    /// is a no-op and the existing ID is returned (content-addressed dedup).
    /// Missing `id`, `content_hash`, and `created_at` fields are filled in
    /// automatically.
    pub fn add_document(&self, mut doc: Document) -> Result<String, AppError> {
        if doc.id.is_empty() {
            doc.id = uuid::Uuid::now_v7().to_string();
        }
        if doc.content_hash.is_empty() {
            doc.content_hash = sha256_hex(&doc.content);
        }
        if doc.created_at.is_empty() {
            doc.created_at = now_iso8601();
        }

        let metadata_json = serde_json::to_string(&doc.metadata)
            .map_err(|e| AppError::Memory(format!("kgdocstore: serialize metadata: {e}")))?;

        let mut conn = open_conn(&self.db_path)?;
        if let Some(existing_id) = Self::find_doc_id_by_hash(&conn, &doc.content_hash)? {
            return Ok(existing_id);
        }

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("kgdocstore: begin tx: {e}")))?;

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
        .map_err(|e| AppError::Memory(format!("kgdocstore: insert metadata: {e}")))?;

        tx.commit()
            .map_err(|e| AppError::Memory(format!("kgdocstore: commit add_document: {e}")))?;

        let content_path = self.doc_content_path(&doc.id);
        if let Some(parent) = content_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::Memory(format!("kgdocstore: create parent dirs {}: {e}", parent.display()))
            })?;
        }
        fs::write(&content_path, doc.content).map_err(|e| {
            AppError::Memory(format!("kgdocstore: write content for {}: {e}", doc.id))
        })?;
        Ok(doc.id)
    }

    /// Retrieve a document by ID, reading its content from disk.
    pub fn get_document(&self, doc_id: &str) -> Result<Document, AppError> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare("SELECT title, source, content_hash, created_at, metadata FROM doc_metadata WHERE doc_id = ?1")
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare get_document: {e}")))?;

        let row = stmt
            .query_row(params![doc_id], |row| {
                let meta_json: String = row.get(4)?;
                let metadata = serde_json::from_str(&meta_json).unwrap_or_default();
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    metadata,
                ))
            })
            .map_err(|e| AppError::Memory(format!("kgdocstore: get_document {doc_id}: {e}")))?;

        let content = fs::read_to_string(self.doc_content_path(doc_id)).map_err(|e| {
            AppError::Memory(format!("kgdocstore: read content for {doc_id}: {e}"))
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

    /// Return metadata for all documents, ordered by insertion time (newest first).
    pub fn list_documents(&self) -> Result<Vec<DocMetadata>, AppError> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare("SELECT doc_id, title, source, content_hash, created_at, updated_at, metadata FROM doc_metadata ORDER BY created_at DESC")
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare list_documents: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let meta_json: String = row.get(6)?;
                let metadata = serde_json::from_str(&meta_json).unwrap_or_default();
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
            .map_err(|e| AppError::Memory(format!("kgdocstore: query list_documents: {e}")))?;

        rows.map(|r| r.map_err(|e| AppError::Memory(format!("kgdocstore: list_documents row: {e}"))))
            .collect()
    }

    /// Remove a document, its chunks, and its content file.
    ///
    /// The KG is **not** automatically rebuilt; call `rebuild_kg` afterwards
    /// if you need the graph to reflect the deletion.
    pub fn delete_document(&self, doc_id: &str) -> Result<(), AppError> {
        let mut conn = open_conn(&self.db_path)?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("kgdocstore: begin delete tx: {e}")))?;

        tx.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::Memory(format!("kgdocstore: delete chunks for {doc_id}: {e}")))?;
        tx.execute("DELETE FROM doc_metadata WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::Memory(format!("kgdocstore: delete metadata for {doc_id}: {e}")))?;
        tx.commit()
            .map_err(|e| AppError::Memory(format!("kgdocstore: commit delete: {e}")))?;

        let content_path = self.doc_content_path(doc_id);
        if content_path.exists() {
            fs::remove_file(&content_path).map_err(|e| {
                AppError::Memory(format!("kgdocstore: remove {}: {e}", content_path.display()))
            })?;
        }
        Ok(())
    }

    // ── Chunking and indexing ─────────────────────────────────────────────

    /// Split a stored document into chunks using the Markdown-aware splitter.
    ///
    /// `chunk_size` is the approximate maximum character count per chunk.
    /// Returns the chunks but does **not** index them — call `index_chunks` next.
    pub fn chunk_document(&self, doc_id: &str, chunk_size: usize) -> Result<Vec<Chunk>, AppError> {
        if chunk_size == 0 {
            return Err(AppError::Memory("kgdocstore: chunk_size must be > 0".to_string()));
        }
        let content = fs::read_to_string(self.doc_content_path(doc_id)).map_err(|e| {
            AppError::Memory(format!("kgdocstore: read content for {doc_id}: {e}"))
        })?;
        let splitter = MarkdownSplitter::new(chunk_size);
        Ok(splitter
            .chunk_indices(&content)
            .filter(|(_, t)| !t.trim().is_empty())
            .map(|(pos, text)| Chunk {
                id: uuid::Uuid::now_v7().to_string(),
                doc_id: doc_id.to_string(),
                text: text.to_string(),
                position: pos,
                metadata: HashMap::new(),
            })
            .collect())
    }

    /// Write `chunks` into the FTS5 table, replacing any previously indexed chunks
    /// for the same `doc_id`(s).  Idempotent for the same set of chunks.
    pub fn index_chunks(&self, chunks: Vec<Chunk>) -> Result<(), AppError> {
        if chunks.is_empty() {
            return Ok(());
        }
        let mut conn = open_conn(&self.db_path)?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Memory(format!("kgdocstore: begin index tx: {e}")))?;

        let doc_ids: HashSet<String> = chunks.iter().map(|c| c.doc_id.clone()).collect();
        for doc_id in &doc_ids {
            tx.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])
                .map_err(|e| AppError::Memory(format!("kgdocstore: clear chunks for {doc_id}: {e}")))?;
        }
        for chunk in chunks {
            let meta_json = serde_json::to_string(&chunk.metadata)
                .map_err(|e| AppError::Memory(format!("kgdocstore: serialize chunk meta: {e}")))?;
            tx.execute(
                "INSERT INTO chunks (id, doc_id, text, position, metadata) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![chunk.id, chunk.doc_id, chunk.text, chunk.position as i64, meta_json],
            )
            .map_err(|e| AppError::Memory(format!("kgdocstore: insert chunk: {e}")))?;
        }
        tx.commit()
            .map_err(|e| AppError::Memory(format!("kgdocstore: commit index: {e}")))?;
        Ok(())
    }

    // ── FTS search ────────────────────────────────────────────────────────

    /// Full-text search over indexed chunks via SQLite FTS5 BM25 ranking.
    ///
    /// Returns up to `top_k` results ordered by relevance.  Returns an empty
    /// list (not an error) on FTS5 syntax errors so callers are not broken by
    /// unusual user queries.
    pub fn search_by_text(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>, AppError> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare(
                "SELECT chunks.id, chunks.doc_id, chunks.text, chunks.position, chunks.metadata,
                        bm25(chunks) AS rank,
                        doc_metadata.title, doc_metadata.source, doc_metadata.content_hash,
                        doc_metadata.created_at, doc_metadata.updated_at, doc_metadata.metadata
                 FROM chunks
                 JOIN doc_metadata ON doc_metadata.doc_id = chunks.doc_id
                 WHERE chunks MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare search_by_text: {e}")))?;

        let safe_query = escape_fts5_query(query);
        let rows_result = stmt.query_map(params![safe_query, top_k as i64], |row| {
            let chunk_meta: HashMap<String, String> =
                serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
            let doc_meta_map: HashMap<String, String> =
                serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default();
            let score = { let s: f64 = row.get(5)?; (-s) as f32 };
            Ok(SearchResult {
                chunk: Chunk {
                    id: row.get(0)?,
                    doc_id: row.get(1)?,
                    text: row.get(2)?,
                    position: row.get::<_, i64>(3)? as usize,
                    metadata: chunk_meta,
                },
                score,
                doc_metadata: DocMetadata {
                    doc_id: row.get(1)?,
                    title: row.get(6)?,
                    source: row.get(7)?,
                    content_hash: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    metadata: doc_meta_map,
                },
            })
        });

        let rows = match rows_result {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("fts5: syntax error") {
                    warn!(error=%msg, "kgdocstore: FTS5 syntax error, returning empty results");
                    return Ok(Vec::new());
                }
                return Err(AppError::Memory(format!("kgdocstore: execute search_by_text: {e}")));
            }
        };

        rows.map(|r| r.map_err(|e| AppError::Memory(format!("kgdocstore: search row: {e}"))))
            .collect()
    }

    // ── KG-specific read helpers ──────────────────────────────────────────

    /// Return every chunk currently indexed in the FTS table.
    /// Used by `rebuild_kg` to read all text for entity extraction.
    pub fn all_chunks(&self) -> Result<Vec<Chunk>, AppError> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn
            .prepare("SELECT id, doc_id, text, position, metadata FROM chunks")
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare all_chunks: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let meta: HashMap<String, String> =
                    serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
                Ok(Chunk {
                    id: row.get(0)?,
                    doc_id: row.get(1)?,
                    text: row.get(2)?,
                    position: row.get::<_, i64>(3)? as usize,
                    metadata: meta,
                })
            })
            .map_err(|e| AppError::Memory(format!("kgdocstore: query all_chunks: {e}")))?;

        rows.map(|r| r.map_err(|e| AppError::Memory(format!("kgdocstore: all_chunks row: {e}"))))
            .collect()
    }

    /// Fetch a specific set of chunks by their ids.
    /// Preserves the order of `ids`.
    pub fn get_chunks_by_ids(&self, ids: &[String]) -> Result<Vec<Chunk>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // Build a map first, then re-order by ids to preserve caller order.
        let conn = open_conn(&self.db_path)?;
        let placeholders = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT id, doc_id, text, position, metadata FROM chunks WHERE id IN ({placeholders})");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare get_chunks_by_ids: {e}")))?;

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

        let rows = stmt
            .query_map(params_ref.as_slice(), |row| {
                let meta: HashMap<String, String> =
                    serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
                Ok(Chunk {
                    id: row.get(0)?,
                    doc_id: row.get(1)?,
                    text: row.get(2)?,
                    position: row.get::<_, i64>(3)? as usize,
                    metadata: meta,
                })
            })
            .map_err(|e| AppError::Memory(format!("kgdocstore: query get_chunks_by_ids: {e}")))?;

        let mut map: HashMap<String, Chunk> = rows
            .filter_map(|r| r.ok())
            .map(|c| (c.id.clone(), c))
            .collect();

        Ok(ids.iter().filter_map(|id| map.remove(id)).collect())
    }

    // ── KG build pipeline ─────────────────────────────────────────────────

    /// Extract entities and relations from all indexed chunks and persist to `kg/`.
    ///
    /// Safe to call multiple times — each call overwrites the previous KG files.
    pub fn rebuild_kg(&self) -> Result<(), AppError> {
        self.rebuild_kg_with_config(&KgConfig::default(), &[])
    }

    /// Like `rebuild_kg` but accepts custom config and an optional domain seed list.
    ///
    /// `domain_seeds` is a list of `(name, kind)` pairs that are matched
    /// case-insensitively and are immune to the minimum-mention filter.
    pub fn rebuild_kg_with_config(
        &self,
        cfg: &KgConfig,
        domain_seeds: &[(&str, EntityKind)],
    ) -> Result<(), AppError> {
        let chunks = self.all_chunks()?;
        if chunks.is_empty() {
            // Nothing indexed yet — write empty graph and return.
            let empty = KgGraph::empty();
            self.write_graph(&empty)?;
            return Ok(());
        }

        // Build a lowercase seed name -> kind map for fast lookup.
        let seed_map: HashMap<String, EntityKind> = domain_seeds
            .iter()
            .map(|(name, kind)| (name.to_lowercase(), *kind))
            .collect();

        // ── Pass 1: entity extraction ─────────────────────────────────────
        // candidate_entities: normalized_name -> (kind, mention_count, set of chunk_ids)
        let mut candidates: HashMap<String, (EntityKind, usize, HashSet<String>)> = HashMap::new();

        for chunk in &chunks {
            let extracted = extract_entities_from_text(&chunk.text, &seed_map);
            let text_lower = chunk.text.to_lowercase();
            for (norm_name, kind) in extracted {
                // Count actual occurrences in the chunk text (not just +1 per chunk)
                // so that a term repeated N times in one chunk accrues N mentions.
                let occurrences = count_occurrences(&text_lower, &norm_name).max(1);
                let entry = candidates.entry(norm_name).or_insert((kind, 0, HashSet::new()));
                entry.1 += occurrences;
                entry.2.insert(chunk.id.clone());
            }
        }

        // ── Filter ────────────────────────────────────────────────────────
        let confirmed: HashMap<String, Entity> = candidates
            .into_iter()
            .filter(|(name, (_, count, _))| {
                if seed_map.contains_key(name) {
                    return true; // seeds are immune to min-mentions filter
                }
                if name.len() <= 1 {
                    return false;
                }
                if name.chars().all(|c| c.is_ascii_digit()) {
                    return false;
                }
                *count >= cfg.min_entity_mentions
            })
            .map(|(name, (kind, count, chunks_set))| {
                let id = sha256_hex(&name)[..16].to_string();
                let entity = Entity {
                    id: id.clone(),
                    name,
                    kind,
                    mention_count: count,
                    source_chunks: chunks_set.into_iter().collect(),
                };
                (entity.id.clone(), entity)
            })
            .collect();

        // name -> entity_id lookup for relation pass
        let name_to_id: HashMap<String, String> = confirmed
            .values()
            .map(|e| (e.name.clone(), e.id.clone()))
            .collect();

        // ── Pass 2: relation extraction ───────────────────────────────────
        // (from_id, to_id, label) -> (raw_weight, set of chunk_ids)
        let mut raw_relations: HashMap<(String, String, String), (usize, HashSet<String>)> =
            HashMap::new();

        for chunk in &chunks {
            let text_lower = chunk.text.to_lowercase();
            // Collect confirmed entity ids present in this chunk
            let present_ids: Vec<(&str, &str)> = confirmed
                .values()
                .filter(|e| text_lower.contains(e.name.as_str()))
                .map(|e| (e.name.as_str(), e.id.as_str()))
                .collect();

            if present_ids.len() < 2 {
                continue;
            }

            // Detect typed relations, fall back to co-occurrence
            for i in 0..present_ids.len() {
                for j in 0..present_ids.len() {
                    if i == j {
                        continue;
                    }
                    let (a_name, a_id) = present_ids[i];
                    let (b_name, b_id) = present_ids[j];
                    let label = detect_relation_label(&chunk.text, a_name, b_name);
                    let key = (a_id.to_string(), b_id.to_string(), label);
                    let entry = raw_relations.entry(key).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    entry.1.insert(chunk.id.clone());
                }
            }
        }

        // ── Normalize weights ─────────────────────────────────────────────
        let max_weight = raw_relations.values().map(|(w, _)| *w).max().unwrap_or(1);

        let relations: Vec<Relation> = raw_relations
            .into_iter()
            .map(|((from, to, label), (raw, chunks_set))| Relation {
                from,
                to,
                label,
                weight: raw as f32 / max_weight as f32,
                source_chunks: chunks_set.into_iter().collect(),
            })
            .collect();

        // ── Persist ───────────────────────────────────────────────────────
        let graph = KgGraph { entities: confirmed, relations };
        self.write_graph(&graph)?;
        Ok(())
    }

    // ── KG query pipeline ─────────────────────────────────────────────────

    /// Search using the KG+FTS pipeline. Falls back to pure FTS if no graph is available.
    pub fn search_with_kg(
        &self,
        query: &str,
        cfg: &KgConfig,
    ) -> Result<KgSearchResult, AppError> {
        let graph = self.load_graph()?;

        if graph.is_empty() {
            return self.fts_only_result(query, cfg);
        }

        // ── Seed finding ──────────────────────────────────────────────────
        let query_lower = query.to_lowercase();
        let mut seeds: Vec<&Entity> = graph
            .entities
            .values()
            .filter(|e| {
                query_lower.contains(e.name.as_str())
                    || e.name.split_whitespace().any(|w| query_lower.contains(w))
            })
            .collect();

        if seeds.is_empty() {
            return self.fts_only_result(query, cfg);
        }

        // Cap seeds by mention count
        seeds.sort_by(|a, b| b.mention_count.cmp(&a.mention_count));
        seeds.truncate(cfg.max_seeds);

        let seed_ids: HashSet<String> = seeds.iter().map(|e| e.id.clone()).collect();
        let seed_names: Vec<String> = seeds.iter().map(|e| e.name.clone()).collect();

        // ── BFS traversal ─────────────────────────────────────────────────
        // Build adjacency: entity_id -> Vec<(neighbour_id, weight, label)>
        let mut adj: HashMap<&str, Vec<(&str, f32, &str)>> = HashMap::new();
        for rel in &graph.relations {
            if rel.weight >= cfg.edge_weight_threshold {
                adj.entry(rel.from.as_str())
                    .or_default()
                    .push((rel.to.as_str(), rel.weight, rel.label.as_str()));
                adj.entry(rel.to.as_str())
                    .or_default()
                    .push((rel.from.as_str(), rel.weight, rel.label.as_str()));
            }
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        for id in &seed_ids {
            queue.push_back((id.clone(), 0));
            visited.insert(id.clone());
        }

        while let Some((eid, depth)) = queue.pop_front() {
            if depth >= cfg.bfs_max_depth {
                continue;
            }
            if let Some(neighbours) = adj.get(eid.as_str()) {
                for (nid, _, _) in neighbours {
                    if !visited.contains(*nid) {
                        visited.insert(nid.to_string());
                        queue.push_back((nid.to_string(), depth + 1));
                    }
                }
            }
        }

        // ── Collect KG chunk pool + scores ────────────────────────────────
        let mut chunk_kg_score: HashMap<String, f32> = HashMap::new();

        for eid in &visited {
            if let Some(entity) = graph.entities.get(eid) {
                for cid in &entity.source_chunks {
                    *chunk_kg_score.entry(cid.clone()).or_insert(0.0) += 0.5;
                }
            }
        }
        for rel in &graph.relations {
            if visited.contains(&rel.from) || visited.contains(&rel.to) {
                for cid in &rel.source_chunks {
                    chunk_kg_score.entry(cid.clone()).or_insert(0.0);
                }
            }
        }

        // ── FTS retrieval ─────────────────────────────────────────────────
        let fts_k = ((cfg.max_chunks as f32 * cfg.fts_share).ceil() as usize).max(1);
        let fts_results = self.search_by_text(query, fts_k)?;
        let fts_ids: HashSet<String> = fts_results.iter().map(|r| r.chunk.id.clone()).collect();

        // Build doc_title lookup from FTS results
        let doc_title_map: HashMap<String, String> = fts_results
            .iter()
            .map(|r| (r.chunk.id.clone(), r.doc_metadata.title.clone()))
            .collect();

        // ── Merge + rank + trim ───────────────────────────────────────────
        let mut all_ids: HashSet<String> = chunk_kg_score.keys().cloned().collect();
        all_ids.extend(fts_ids.iter().cloned());

        let mut scored: Vec<(String, f32)> = all_ids
            .into_iter()
            .map(|cid| {
                let kg_bonus = chunk_kg_score.get(&cid).cloned().unwrap_or(0.0);
                let fts_bonus = if fts_ids.contains(&cid) { 1.0 } else { 0.0 };
                let score = 1.0 + kg_bonus + fts_bonus;
                (cid, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(cfg.max_chunks);

        let selected_ids: Vec<String> = scored.iter().map(|(id, _)| id.clone()).collect();

        // ── Fetch chunk texts ─────────────────────────────────────────────
        let mut fetched = self.get_chunks_by_ids(&selected_ids)?;

        // For chunks not in FTS results, fetch doc_title from DB
        let missing_doc_titles: HashSet<String> = fetched
            .iter()
            .filter(|c| !doc_title_map.contains_key(&c.id))
            .map(|c| c.doc_id.clone())
            .collect();

        let extra_titles: HashMap<String, String> = if !missing_doc_titles.is_empty() {
            self.list_documents()
                .unwrap_or_default()
                .into_iter()
                .filter(|m| missing_doc_titles.contains(&m.doc_id))
                .map(|m| (m.doc_id, m.title))
                .collect()
        } else {
            HashMap::new()
        };

        // Keep original scored order
        let score_map: HashMap<&str, f32> = scored.iter().map(|(id, s)| (id.as_str(), *s)).collect();
        fetched.sort_by(|a, b| {
            let sa = score_map.get(a.id.as_str()).cloned().unwrap_or(0.0);
            let sb = score_map.get(b.id.as_str()).cloned().unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        // ── KG summary ────────────────────────────────────────────────────
        let mut summary_lines: Vec<String> = Vec::new();
        for seed in &seeds {
            let neighbours = top_neighbours(&graph, &seed.id, &adj, 5);
            let neighbour_names: Vec<&str> = neighbours
                .iter()
                .filter_map(|nid| graph.entities.get(*nid).map(|e| e.name.as_str()))
                .collect();
            if neighbour_names.is_empty() {
                summary_lines.push(format!("{} [{}]", seed.name, seed.kind));
            } else {
                summary_lines.push(format!(
                    "{} [{}] — related to: {}",
                    seed.name,
                    seed.kind,
                    neighbour_names.join(", ")
                ));
            }
        }

        // ── Context assembly ──────────────────────────────────────────────
        let mut context = String::new();
        if !summary_lines.is_empty() {
            context.push_str("## Knowledge Graph Context\n");
            for line in &summary_lines {
                context.push_str(line);
                context.push('\n');
            }
            context.push('\n');
        }
        context.push_str("## Relevant Passages\n");
        for chunk in &fetched {
            let title = doc_title_map
                .get(&chunk.id)
                .or_else(|| extra_titles.get(&chunk.doc_id))
                .map(|s| s.as_str())
                .unwrap_or(&chunk.doc_id);
            context.push_str(&format!("\n[{} | {}]\n{}\n", chunk.id, title, chunk.text));
        }

        Ok(KgSearchResult {
            context,
            used_kg: true,
            seed_entities: seed_names,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Initialise or validate the SQLite schema.
    ///
    /// - `user_version == 0`: fresh DB, run DDL.
    /// - `user_version == SCHEMA_VERSION`: already initialised, skip.
    /// - Anything else: unsupported version, return an error.
    fn init_db(&self) -> Result<(), AppError> {
        let conn = open_conn(&self.db_path)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .map_err(|e| AppError::Memory(format!("kgdocstore: read schema version: {e}")))?;

        if version == 0 {
            init_schema(&conn)?;
            return Ok(());
        }
        if version != SCHEMA_VERSION {
            return Err(AppError::Memory(format!(
                "kgdocstore: unsupported schema version {version}, expected {SCHEMA_VERSION}"
            )));
        }
        Ok(())
    }

    /// Resolve the filesystem path for a document's raw content file.
    fn doc_content_path(&self, doc_id: &str) -> PathBuf {
        let rel = Path::new(doc_id).with_extension("txt");
        self.docs_dir.join(rel)
    }

    /// Look up a document ID by its SHA-256 content hash.
    /// Returns `None` if no document with that hash exists.
    fn find_doc_id_by_hash(
        conn: &rusqlite::Connection,
        hash: &str,
    ) -> Result<Option<String>, AppError> {
        let mut stmt = conn
            .prepare("SELECT doc_id FROM doc_metadata WHERE content_hash = ?1")
            .map_err(|e| AppError::Memory(format!("kgdocstore: prepare find by hash: {e}")))?;
        let mut rows = stmt
            .query(params![hash])
            .map_err(|e| AppError::Memory(format!("kgdocstore: query find by hash: {e}")))?;
        if let Some(row) = rows.next().map_err(|e| AppError::Memory(e.to_string()))? {
            return Ok(Some(row.get(0).map_err(|e| AppError::Memory(e.to_string()))?));
        }
        Ok(None)
    }

    /// Persist the graph to disk, writing `entities.json`, `relations.json`, and
    /// `graph.json` (the combined file used for fast loading at query time).
    fn write_graph(&self, graph: &KgGraph) -> Result<(), AppError> {
        // Write entities.json
        let entities_json = serde_json::to_string_pretty(&graph.entities)
            .map_err(|e| AppError::Memory(format!("kgdocstore: serialize entities: {e}")))?;
        fs::write(self.kg_dir.join(ENTITIES_FILE), &entities_json)
            .map_err(|e| AppError::Memory(format!("kgdocstore: write entities.json: {e}")))?;

        // Write relations.json
        let relations_json = serde_json::to_string_pretty(&graph.relations)
            .map_err(|e| AppError::Memory(format!("kgdocstore: serialize relations: {e}")))?;
        fs::write(self.kg_dir.join(RELATIONS_FILE), &relations_json)
            .map_err(|e| AppError::Memory(format!("kgdocstore: write relations.json: {e}")))?;

        // Write combined graph.json
        let graph_json = serde_json::to_string_pretty(graph)
            .map_err(|e| AppError::Memory(format!("kgdocstore: serialize graph: {e}")))?;
        fs::write(self.kg_dir.join(GRAPH_FILE), &graph_json)
            .map_err(|e| AppError::Memory(format!("kgdocstore: write graph.json: {e}")))?;

        Ok(())
    }

    /// Load `graph.json` from disk.  Returns an empty graph when the file does
    /// not yet exist (i.e. `rebuild_kg` has not been called).
    fn load_graph(&self) -> Result<KgGraph, AppError> {
        let path = self.kg_dir.join(GRAPH_FILE);
        if !path.exists() {
            return Ok(KgGraph::empty());
        }
        let data = fs::read_to_string(&path)
            .map_err(|e| AppError::Memory(format!("kgdocstore: read graph.json: {e}")))?;
        serde_json::from_str(&data)
            .map_err(|e| AppError::Memory(format!("kgdocstore: parse graph.json: {e}")))
    }

    /// Construct a `KgSearchResult` using pure FTS (no KG).  Used as a fallback
    /// when the graph is empty or no seed entities match the query.
    fn fts_only_result(&self, query: &str, cfg: &KgConfig) -> Result<KgSearchResult, AppError> {
        let results = self.search_by_text(query, cfg.max_chunks)?;
        let mut context = String::from("## Relevant Passages\n");
        for r in &results {
            context.push_str(&format!(
                "\n[{} | {}]\n{}\n",
                r.chunk.id, r.doc_metadata.title, r.chunk.text
            ));
        }
        Ok(KgSearchResult { context, used_kg: false, seed_entities: Vec::new() })
    }
}

// ── Entity extraction helpers ─────────────────────────────────────────────────

/// Extract candidate entities from a chunk of text.
///
/// Returns a `Vec<(normalized_name, kind)>` with possible duplicates (caller
/// accumulates counts).  Cascade order: quoted/code-fenced terms first (highest
/// confidence), then CamelCase identifiers, then Title Case noun phrases.
/// Domain seeds are appended at the end.
fn extract_entities_from_text(
    text: &str,
    seeds: &HashMap<String, EntityKind>,
) -> Vec<(String, EntityKind)> {
    let mut results: Vec<(String, EntityKind)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut add = |name: String, kind: EntityKind| {
        if name.len() > 1 && seen.insert(name.clone()) {
            results.push((name, kind));
        }
    };

    // 1. Backtick-quoted terms: `Foo`
    for inner in BACKTICK_RE.find_iter(text) {
        let inner = inner.trim();
        if !inner.is_empty() {
            add(inner.to_lowercase(), EntityKind::Term);
        }
    }

    // 2. Double-quoted terms: "Foo Bar"
    for inner in DQUOTE_RE.find_iter(text) {
        let inner = inner.trim();
        if !inner.is_empty() && inner.split_whitespace().count() <= 4 {
            add(inner.to_lowercase(), EntityKind::Term);
        }
    }

    // 3. CamelCase identifiers
    for tok in text.split_whitespace() {
        let tok = tok.trim_matches(|c: char| !c.is_alphanumeric());
        if is_camel_case(tok) {
            add(tok.to_lowercase(), EntityKind::System);
        }
    }

    // 4. Title Case noun phrases (>= 2 words) — skip sentence-start positions
    extract_title_case_phrases(text, &mut |phrase: String| {
        add(phrase.to_lowercase(), EntityKind::Concept);
    });

    // 5. Acronyms: 2-5 uppercase letters
    for tok in text.split_whitespace() {
        let tok = tok.trim_matches(|c: char| !c.is_alphabetic());
        if tok.len() >= 2
            && tok.len() <= 5
            && tok.chars().all(|c| c.is_uppercase())
        {
            add(tok.to_lowercase(), EntityKind::Acronym);
        }
    }

    // 6. Domain seeds (case-insensitive match anywhere in text)
    let text_lower = text.to_lowercase();
    for (seed_name, seed_kind) in seeds {
        if text_lower.contains(seed_name.as_str()) && seen.insert(seed_name.clone()) {
            results.push((seed_name.clone(), *seed_kind));
        }
    }

    results
}

/// Count non-overlapping occurrences of `needle` in `haystack`.
///
/// Used so that an entity mentioned N times in a single chunk accrues N
/// mentions rather than 1, letting frequent terms pass `min_entity_mentions`
/// without needing to appear across multiple chunks.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

/// Returns true if `s` looks like a CamelCase identifier (at least one internal
/// uppercase after the first char, no spaces, mostly alphanumeric).
fn is_camel_case(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    // Must start with uppercase or lowercase letter
    if !chars[0].is_alphabetic() {
        return false;
    }
    // All chars must be alphanumeric or underscore
    if !chars.iter().all(|c| c.is_alphanumeric() || *c == '_') {
        return false;
    }
    // Must have at least one uppercase letter after position 0
    chars[1..].iter().any(|c| c.is_uppercase())
}

/// Walk the text collecting sequences of >= 2 consecutive Title Case words
/// that are not at a sentence start.
fn extract_title_case_phrases(text: &str, emit: &mut impl FnMut(String)) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    // Track which word indices are at a sentence start.
    let sentence_starts: HashSet<usize> = {
        let mut s = HashSet::new();
        s.insert(0);
        let mut prev_end = false;
        for (idx, w) in words.iter().enumerate() {
            if prev_end && w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                s.insert(idx);
            }
            prev_end = w.ends_with('.') || w.ends_with('!') || w.ends_with('?');
        }
        s
    };

    while i < words.len() {
        let w = words[i].trim_matches(|c: char| !c.is_alphanumeric());
        let starts_upper = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        if starts_upper && !sentence_starts.contains(&i) {
            // Collect consecutive Title Case words
            let mut j = i;
            while j < words.len() {
                let wj = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                let up = wj.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                if !up || wj.is_empty() {
                    break;
                }
                j += 1;
            }
            if j - i >= 2 {
                let phrase = words[i..j]
                    .iter()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
                    .collect::<Vec<_>>()
                    .join(" ");
                emit(phrase);
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

/// Detect the semantic relation label between entity A and entity B within `text`.
///
/// Searches the substring *between* the two entity mentions for known
/// keyword patterns (`RELATION_KEYWORDS`).  Falls back to `"defined_as"` or
/// `"instance_of"` for `is-a` phrases, and finally to the generic
/// `"relates_to"` for plain co-occurrence.
fn detect_relation_label(text: &str, a_name: &str, b_name: &str) -> String {
    let text_lower = text.to_lowercase();
    let a_lower = a_name.to_lowercase();
    let b_lower = b_name.to_lowercase();

    if let (Some(pa), Some(pb)) = (text_lower.find(&a_lower), text_lower.find(&b_lower)) {
        let between = if pa < pb {
            &text_lower[pa + a_lower.len()..pb]
        } else {
            &text_lower[pb + b_lower.len()..pa]
        };

        for (keyword, label) in RELATION_KEYWORDS {
            if between.contains(keyword) {
                return label.to_string();
            }
        }

        // "A is a B" / "A refers to B"
        if between.contains(" is a ") || between.contains(" is an ") {
            return "defined_as".to_string();
        }
        if between.contains(" refers to ") || between.contains(" instance of ") {
            return "instance_of".to_string();
        }
    }

    "relates_to".to_string()
}

/// Keyword patterns used by `detect_relation_label` to produce typed edge labels.
/// Each tuple is `(text_token_to_match, relation_label)`.
const RELATION_KEYWORDS: &[(&str, &str)] = &[
    ("uses", "uses"),
    ("implements", "implements"),
    ("extends", "extends"),
    ("calls", "calls"),
    ("depends on", "depends_on"),
    ("requires", "requires"),
];

/// Return top-N neighbour entity ids by edge weight from the adjacency map.
fn top_neighbours<'a>(
    graph: &'a KgGraph,
    entity_id: &str,
    adj: &HashMap<&str, Vec<(&'a str, f32, &'a str)>>,
    n: usize,
) -> Vec<&'a str> {
    let Some(neighbours) = adj.get(entity_id) else {
        return Vec::new();
    };
    let mut sorted = neighbours.clone();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted
        .into_iter()
        .take(n)
        .filter(|(nid, _, _)| graph.entities.contains_key(*nid))
        .map(|(nid, _, _)| nid)
        .collect()
}

// ── Pattern helpers ───────────────────────────────────────────────────────────
// Hand-rolled matchers to avoid adding the `regex` crate dependency.
// They work by splitting on the delimiter character: odd-indexed parts are
// "inside" the delimiter pair.

/// Extracts content between paired backticks (`` `…` ``).
struct BacktickFinder;

/// Extracts content between paired double-quotes (`"…"`).
struct DquoteFinder;

impl BacktickFinder {
    /// Return slices of text found between matched backtick pairs.
    fn find_iter<'a>(&self, text: &'a str) -> Vec<&'a str> {
        let mut results = Vec::new();
        let parts: Vec<&str> = text.split('`').collect();
        // Odd-indexed parts are inside backticks
        for (i, part) in parts.iter().enumerate() {
            if i % 2 == 1 {
                results.push(*part);
            }
        }
        results
    }
}

impl DquoteFinder {
    /// Return slices of text found between matched double-quote pairs.
    fn find_iter<'a>(&self, text: &'a str) -> Vec<&'a str> {
        let mut results = Vec::new();
        let parts: Vec<&str> = text.split('"').collect();
        for (i, part) in parts.iter().enumerate() {
            if i % 2 == 1 {
                results.push(*part);
            }
        }
        results
    }
}

static BACKTICK_RE: BacktickFinder = BacktickFinder;
static DQUOTE_RE: DquoteFinder = DquoteFinder;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::memory::AGENTS_DIRNAME;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, IKGDocStore) {
        let temp = TempDir::new().expect("tempdir");
        let identity_dir = temp.path().join(AGENTS_DIRNAME);
        fs::create_dir_all(&identity_dir).expect("create identity dir");
        let store = IKGDocStore::open(&identity_dir).expect("open kgdocstore");
        (temp, store)
    }

    fn make_doc(title: &str, content: &str) -> Document {
        Document {
            id: String::new(),
            title: title.to_string(),
            source: "unit".to_string(),
            content: content.to_string(),
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn open_creates_dirs() {
        let (_temp, store) = make_store();
        assert!(store.dir.exists());
        assert!(store.docs_dir.exists());
        assert!(store.kg_dir.exists());
    }

    #[test]
    fn base_api_round_trip() {
        let (_temp, store) = make_store();
        let doc_id = store.add_document(make_doc("T", "hello world")).expect("add");
        let chunks = store.chunk_document(&doc_id, 20).expect("chunk");
        assert!(!chunks.is_empty());
        store.index_chunks(chunks).expect("index");
        let results = store.search_by_text("hello", 5).expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk.doc_id, doc_id);
    }

    #[test]
    fn dedup_by_hash() {
        let (_temp, store) = make_store();
        let id1 = store.add_document(make_doc("A", "same content")).expect("add first");
        let id2 = store.add_document(make_doc("B", "same content")).expect("dedup");
        assert_eq!(id1, id2);
    }

    #[test]
    fn all_chunks_returns_indexed() {
        let (_temp, store) = make_store();
        let doc_id = store.add_document(make_doc("X", "alpha beta gamma delta")).expect("add");
        let chunks = store.chunk_document(&doc_id, 10).expect("chunk");
        let n = chunks.len();
        store.index_chunks(chunks).expect("index");
        let all = store.all_chunks().expect("all_chunks");
        assert_eq!(all.len(), n);
    }

    #[test]
    fn get_chunks_by_ids_ordered() {
        let (_temp, store) = make_store();
        let doc_id = store.add_document(make_doc("Y", "one two three four five six")).expect("add");
        let chunks = store.chunk_document(&doc_id, 5).expect("chunk");
        store.index_chunks(chunks.clone()).expect("index");
        let ids: Vec<String> = chunks.iter().rev().map(|c| c.id.clone()).collect();
        let fetched = store.get_chunks_by_ids(&ids).expect("fetch");
        assert_eq!(fetched.len(), ids.len());
        // Order should match request order
        for (fetched_chunk, req_id) in fetched.iter().zip(ids.iter()) {
            assert_eq!(&fetched_chunk.id, req_id);
        }
    }

    #[test]
    fn rebuild_kg_produces_entities_and_relations() {
        let (_temp, store) = make_store();
        // Repeat CamelCase terms multiple times to survive min_mentions filter
        let content = "AuthService handles TokenValidator. \
                       AuthService calls TokenValidator to validate. \
                       AuthService uses TokenValidator extensively.";
        let doc_id = store.add_document(make_doc("Auth", content)).expect("add");
        let chunks = store.chunk_document(&doc_id, 500).expect("chunk");
        store.index_chunks(chunks).expect("index");

        store.rebuild_kg().expect("rebuild_kg");

        let graph = store.load_graph().expect("load_graph");
        assert!(!graph.entities.is_empty(), "expected entities after rebuild");
    }

    #[test]
    fn search_with_kg_falls_back_when_no_graph() {
        let (_temp, store) = make_store();
        let doc_id = store.add_document(make_doc("D", "the quick brown fox")).expect("add");
        let chunks = store.chunk_document(&doc_id, 50).expect("chunk");
        store.index_chunks(chunks).expect("index");
        // No rebuild_kg call — graph.json absent
        let result = store.search_with_kg("fox", &KgConfig::default()).expect("search");
        assert!(!result.used_kg, "should fall back to FTS");
        assert!(!result.context.is_empty());
    }

    #[test]
    fn search_with_kg_uses_kg_when_graph_present() {
        let (_temp, store) = make_store();
        let content = "AuthService handles TokenValidator. \
                       AuthService calls TokenValidator to validate. \
                       AuthService uses TokenValidator extensively.";
        let doc_id = store.add_document(make_doc("Auth", content)).expect("add");
        let chunks = store.chunk_document(&doc_id, 500).expect("chunk");
        store.index_chunks(chunks).expect("index");
        store.rebuild_kg().expect("rebuild_kg");

        let cfg = KgConfig { min_entity_mentions: 1, ..KgConfig::default() };
        let result = store.search_with_kg("authservice tokenvalidator", &cfg).expect("search");
        // If seeds were found the context should exist
        assert!(!result.context.is_empty());
    }

    #[test]
    fn empty_seed_falls_back_to_fts() {
        let (_temp, store) = make_store();
        let doc_id = store.add_document(make_doc("F", "rust memory bm25 search")).expect("add");
        let chunks = store.chunk_document(&doc_id, 50).expect("chunk");
        store.index_chunks(chunks).expect("index");

        // Build KG but query with terms that won't match any entity
        store.rebuild_kg().expect("rebuild_kg");
        let result = store
            .search_with_kg("zzz unknown query xyz", &KgConfig::default())
            .expect("search");
        assert!(!result.used_kg, "no matching seeds → FTS fallback");
    }

    #[test]
    fn is_camel_case_detection() {
        assert!(is_camel_case("AuthService"));
        assert!(is_camel_case("TokenValidator"));
        assert!(!is_camel_case("hello"));
        assert!(!is_camel_case("UPPERCASE"));
        assert!(!is_camel_case("ab"));
    }
}
