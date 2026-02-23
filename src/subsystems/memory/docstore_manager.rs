//! Background docstore manager — internal to the memory subsystem.
//!
//! Compiled only when the `idocstore` Cargo feature is enabled.
//!
//! Spawned by [`super::MemorySystem::start_docstore_manager`].  Invisibe
//! outside of the memory subsystem.  Periodically scans every agent identity
//! directory for an IDocStore and performs two maintenance tasks:
//!
//! - **Index unindexed docs** — any document row in `doc_metadata` that has
//!   no corresponding rows in the FTS5 `chunks` table is chunked (2 KB
//!   default) and indexed automatically.
//! - **Cleanup orphan files** — any `.txt` file under `docstore/docs/` that
//!   has no matching `doc_metadata` entry is removed.
//!
//! The scan interval is 24 hours. A single-shot `IndexNow` command is also
//! accepted so callers can request immediate maintenance after ingesting a
//! document.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;

use super::stores::docstore::IDocStore;

const SCAN_INTERVAL_SECS: u64 = 86_400; // 24 hours
const DEFAULT_CHUNK_SIZE: usize = 2048;

// ── Command channel ───────────────────────────────────────────────────────────

pub(super) enum ManagerCmd {
    /// Request immediate index+cleanup for a single agent identity dir.
    IndexNow { agent_identity_dir: PathBuf },
}

// ── Public handle ─────────────────────────────────────────────────────────────

/// Handle to the background docstore manager task.
///
/// Only accessible inside the `memory` module hierarchy
/// (`pub(super)` visibility).
pub(super) struct DocstoreManager {
    cmd_tx: mpsc::Sender<ManagerCmd>,
}

impl DocstoreManager {
    /// Spawn the background manager task.
    ///
    /// `agent_dirs_root` should be `{memory_root}/agent/` — the parent of all
    /// per-agent identity directories.
    pub(super) fn spawn(agent_dirs_root: PathBuf, shutdown: CancellationToken) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        tokio::spawn(
            ManagerService {
                agent_dirs_root,
                cmd_rx,
                shutdown,
            }
            .run(),
        );
        Self { cmd_tx }
    }

    /// Schedule immediate index+cleanup for one agent identity directory.
    ///
    /// Non-blocking — the work is queued and processed in the background task.
    pub(super) fn schedule_index(&self, agent_identity_dir: PathBuf) {
        let _ = self.cmd_tx.try_send(ManagerCmd::IndexNow { agent_identity_dir });
    }
}

// ── Background service ────────────────────────────────────────────────────────

struct ManagerService {
    agent_dirs_root: PathBuf,
    cmd_rx: mpsc::Receiver<ManagerCmd>,
    shutdown: CancellationToken,
}

impl ManagerService {
    async fn run(mut self) {
        info!(
            root = %self.agent_dirs_root.display(),
            interval_secs = SCAN_INTERVAL_SECS,
            "docstore manager started (daily scan)"
        );

        let mut interval = tokio::time::interval(Duration::from_secs(SCAN_INTERVAL_SECS));
        interval.tick().await; // skip the first immediate tick

        loop {
            tokio::select! {
                biased;

                _ = self.shutdown.cancelled() => {
                    info!("docstore manager stopping");
                    break;
                }

                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        ManagerCmd::IndexNow { agent_identity_dir } => {
                            process_agent_dir(agent_identity_dir).await;
                        }
                    }
                }

                _ = interval.tick() => {
                    self.scan_all().await;
                }
            }
        }
    }

    async fn scan_all(&self) {
        let Ok(rd) = std::fs::read_dir(&self.agent_dirs_root) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("docstore").join("chunks.db").exists() {
                process_agent_dir(path).await;
            }
        }
    }
}

// ── Per-docstore maintenance (sync, executed in spawn_blocking) ───────────────

async fn process_agent_dir(agent_identity_dir: PathBuf) {
    let result = tokio::task::spawn_blocking(move || {
        let store = IDocStore::open(&agent_identity_dir)?;
        let indexed = index_unindexed(&store)?;
        let cleaned = cleanup_orphans(&store)?;
        Ok::<_, AppError>((indexed, cleaned))
    })
    .await;

    match result {
        Ok(Ok((i, c))) if i > 0 || c > 0 => {
            debug!(indexed = i, cleaned = c, "docstore: maintenance done");
        }
        Ok(Ok(_)) => {}
        Ok(Err(e)) => warn!(error = %e, "docstore manager: maintenance error"),
        Err(e) => warn!(error = %e, "docstore manager: task panicked"),
    }
}

/// Chunk and index every `doc_metadata` row that has no `chunks` entries.
///
/// Returns the number of documents indexed.
fn index_unindexed(store: &IDocStore) -> Result<usize, AppError> {
    let db = store.root_dir().join("chunks.db");
    let conn = Connection::open(&db)
        .map_err(|e| AppError::Memory(format!("docstore manager: open db: {e}")))?;

    let mut stmt = conn
        .prepare(
            "SELECT doc_id FROM doc_metadata \
             WHERE doc_id NOT IN (SELECT DISTINCT doc_id FROM chunks)",
        )
        .map_err(|e| AppError::Memory(format!("docstore manager: prepare unindexed query: {e}")))?;

    let unindexed: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| AppError::Memory(format!("docstore manager: query unindexed: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    let n = unindexed.len();
    for doc_id in unindexed {
        let chunks = match store.chunk_document(&doc_id, DEFAULT_CHUNK_SIZE) {
            Ok(c) => c,
            Err(e) => {
                warn!(%doc_id, error = %e, "docstore manager: chunk failed");
                continue;
            }
        };
        if chunks.is_empty() {
            continue;
        }
        if let Err(e) = store.index_chunks(chunks) {
            warn!(%doc_id, error = %e, "docstore manager: index_chunks failed");
        }
    }

    Ok(n)
}

/// Remove `.txt` files in `docstore/docs/` that have no matching metadata row.
///
/// Returns the number of files removed.
fn cleanup_orphans(store: &IDocStore) -> Result<usize, AppError> {
    let docs_dir = store.root_dir().join("docs");
    if !docs_dir.exists() {
        return Ok(0);
    }

    let known: HashSet<String> = store
        .list_documents()?
        .into_iter()
        .map(|m| m.doc_id)
        .collect();

    let Ok(rd) = std::fs::read_dir(&docs_dir) else {
        return Ok(0);
    };

    let mut removed = 0usize;
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let doc_id = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if doc_id.is_empty() || known.contains(&doc_id) {
            continue;
        }
        match std::fs::remove_file(&p) {
            Ok(()) => {
                removed += 1;
                debug!(%doc_id, "docstore manager: orphan content file removed");
            }
            Err(e) => warn!(path = %p.display(), error = %e, "docstore manager: remove orphan failed"),
        }
    }

    Ok(removed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    use super::super::stores::docstore::{Document, IDocStore};

    fn make_store() -> (TempDir, IDocStore) {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("agent");
        fs::create_dir_all(&dir).unwrap();
        let store = IDocStore::open(&dir).unwrap();
        (temp, store)
    }

    fn doc(content: &str) -> Document {
        Document {
            id: String::new(),
            title: "Test".into(),
            source: "unit".into(),
            content: content.into(),
            content_hash: String::new(),
            created_at: String::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn index_unindexed_chunks_new_doc() {
        let (_t, store) = make_store();
        let _id = store.add_document(doc("hello world from the manager test")).unwrap();

        let indexed = index_unindexed(&store).unwrap();
        assert_eq!(indexed, 1);

        // Idempotent: already indexed, nothing more to do.
        let again = index_unindexed(&store).unwrap();
        assert_eq!(again, 0);
    }

    #[test]
    fn index_unindexed_skips_already_indexed() {
        let (_t, store) = make_store();
        let id = store.add_document(doc("already indexed content here")).unwrap();
        let chunks = store.chunk_document(&id, 2048).unwrap();
        store.index_chunks(chunks).unwrap();

        let indexed = index_unindexed(&store).unwrap();
        assert_eq!(indexed, 0);
    }

    #[test]
    fn cleanup_orphans_removes_files_without_metadata() {
        let (_t, store) = make_store();

        // Simulate a file that has no metadata entry.
        let orphan = store.root_dir().join("docs").join("deadbeef-orphan.txt");
        fs::create_dir_all(store.root_dir().join("docs")).unwrap();
        fs::write(&orphan, "orphaned content").unwrap();

        let removed = cleanup_orphans(&store).unwrap();
        assert_eq!(removed, 1);
        assert!(!orphan.exists());
    }

    #[test]
    fn cleanup_orphans_keeps_valid_docs() {
        let (_t, store) = make_store();
        let id = store.add_document(doc("valid doc content")).unwrap();

        let removed = cleanup_orphans(&store).unwrap();
        assert_eq!(removed, 0);

        let content_file = store.root_dir().join("docs").join(format!("{id}.txt"));
        assert!(content_file.exists());
    }
}
