//! [`SessionHandle`] — async-safe handle for reading and writing session data.
//!
//! Agents receive a `SessionHandle` when they create or load a session.
//! All I/O is dispatched to a blocking thread pool so callers remain
//! non-blocking.
//!
//! ### Typed memory (TmpStore sessions)
//!
//! When a session was created with the `"tmp"` store type, the handle also
//! holds a direct `Arc<TmpStore>` reference.  Use [`tmp_doc`] / [`tmp_block`]
//! to read a snapshot of the session-scoped [`Doc`] or [`Block`] collection,
//! then call [`set_tmp_doc`] / [`set_tmp_block`] to write modifications back.
//!
//! [`tmp_doc`]: SessionHandle::tmp_doc
//! [`tmp_block`]: SessionHandle::tmp_block
//! [`set_tmp_doc`]: SessionHandle::set_tmp_doc
//! [`set_tmp_block`]: SessionHandle::set_tmp_block

use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use crate::error::AppError;
use super::collections::{Block, Collection, Doc};
use super::store::{SessionStore, TranscriptEntry};
use super::stores::tmp::TmpStore;

#[derive(Debug, Clone)]
pub struct SessionFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
}

/// Async-safe handle to a single session's stores.
///
/// Cheaply cloneable (`Arc`-backed).  All file I/O runs on
/// `tokio::task::spawn_blocking` so callers can hold this in async code.
#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    session_dir: PathBuf,
    stores: Vec<Arc<dyn SessionStore>>,
    /// Present when the session was created / loaded with the `"tmp"` store
    /// type.  Used by [`tmp_doc`](Self::tmp_doc) and friends.
    tmp_store: Option<Arc<TmpStore>>,
}

impl SessionHandle {
    pub(crate) fn new(
        session_id: String,
        session_dir: PathBuf,
        stores: Vec<Arc<dyn SessionStore>>,
        tmp_store: Option<Arc<TmpStore>>,
    ) -> Self {
        Self { session_id, session_dir, stores, tmp_store }
    }

    /// Get the first store that matches `store_type`, or the first store if
    /// only one is registered.
    fn find_store(&self, store_type: &str) -> Result<Arc<dyn SessionStore>, AppError> {
        self.stores
            .iter()
            .find(|s| s.store_type() == store_type)
            .or_else(|| self.stores.first())
            .cloned()
            .ok_or_else(|| AppError::Memory("no stores registered for session".into()))
    }

    /// Convenience: get the first store (for single-store sessions like basic_session).
    fn default_store(&self) -> Result<Arc<dyn SessionStore>, AppError> {
        self.stores
            .first()
            .cloned()
            .ok_or_else(|| AppError::Memory("no stores registered for session".into()))
    }

    // ── K-V operations ────────────────────────────────────────────────

    pub async fn kv_get(&self, key: &str) -> Result<Option<String>, AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.kv_get(&dir, &key))
            .await
            .map_err(|e| AppError::Memory(format!("kv_get join: {e}")))?
    }

    pub async fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        let key = key.to_string();
        let value = value.to_string();
        tokio::task::spawn_blocking(move || store.kv_set(&dir, &key, &value))
            .await
            .map_err(|e| AppError::Memory(format!("kv_set join: {e}")))?
    }

    pub async fn kv_delete(&self, key: &str) -> Result<bool, AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.kv_delete(&dir, &key))
            .await
            .map_err(|e| AppError::Memory(format!("kv_delete join: {e}")))?
    }

    // ── Transcript operations ─────────────────────────────────────────

    pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        let role = role.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || store.transcript_append(&dir, &role, &content))
            .await
            .map_err(|e| AppError::Memory(format!("transcript_append join: {e}")))?
    }

    pub async fn transcript_read_last(&self, n: usize) -> Result<Vec<TranscriptEntry>, AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        tokio::task::spawn_blocking(move || store.transcript_read_last(&dir, n))
            .await
            .map_err(|e| AppError::Memory(format!("transcript_read_last join: {e}")))?
    }

    pub async fn working_memory_read(&self) -> Result<String, AppError> {
        Ok(self.kv_get("working_memory").await?.unwrap_or_default())
    }

    // ── Typed tmp-store access ────────────────────────────────────────

    /// Returns a snapshot clone of the session-scoped [`Doc`] collection.
    ///
    /// Only available when the session was created with store type `"tmp"`.
    /// Mutations are not visible until written back with [`set_tmp_doc`](Self::set_tmp_doc).
    pub fn tmp_doc(&self) -> Result<Doc, AppError> {
        self.tmp_store()?
            .inner()
            .get_collection(&self.tmp_label("doc"))?
            .and_then(|c| c.into_doc())
            .ok_or_else(|| AppError::Memory("tmp session 'doc' collection not found".into()))
    }

    /// Returns a snapshot clone of the session-scoped [`Block`] collection.
    ///
    /// Only available when the session was created with store type `"tmp"`.
    pub fn tmp_block(&self) -> Result<Block, AppError> {
        self.tmp_store()?
            .inner()
            .get_collection(&self.tmp_label("block"))?
            .and_then(|c| c.into_block())
            .ok_or_else(|| AppError::Memory("tmp session 'block' collection not found".into()))
    }

    /// Write a modified [`Doc`] snapshot back to the session's tmp store.
    pub fn set_tmp_doc(&self, doc: Doc) -> Result<(), AppError> {
        self.tmp_store()?.inner().insert_collection(
            self.tmp_label("doc"),
            Collection::Doc(doc),
        )
    }

    /// Write a modified [`Block`] snapshot back to the session's tmp store.
    pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError> {
        self.tmp_store()?.inner().insert_collection(
            self.tmp_label("block"),
            Collection::Block(block),
        )
    }

    fn tmp_store(&self) -> Result<&Arc<TmpStore>, AppError> {
        self.tmp_store
            .as_ref()
            .ok_or_else(|| AppError::Memory("session has no tmp store (not a 'tmp' session)".into()))
    }

    fn tmp_label(&self, kind: &str) -> String {
        format!("{}:{kind}", self.session_dir.display())
    }

    pub async fn list_files(&self) -> Result<Vec<SessionFileInfo>, AppError> {
        let dir = self.session_dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut files = Vec::new();
            for entry in std::fs::read_dir(&dir)
                .map_err(|e| AppError::Memory(format!("cannot read {}: {e}", dir.display())))?
            {
                let entry = entry.map_err(|e| {
                    AppError::Memory(format!("cannot read file entry in {}: {e}", dir.display()))
                })?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let meta = entry.metadata().map_err(|e| {
                    AppError::Memory(format!("cannot stat {}: {e}", path.display()))
                })?;

                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
                    .map(|d| epoch_to_iso8601(d.as_secs()))
                    .unwrap_or_default();

                files.push(SessionFileInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    size_bytes: meta.len(),
                    modified,
                });
            }

            files.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(files)
        })
        .await
        .map_err(|e| AppError::Memory(format!("list_files join: {e}")))?
    }
}

fn epoch_to_iso8601(epoch_secs: u64) -> String {
    let s = epoch_secs % 60;
    let total_min = epoch_secs / 60;
    let m = total_min % 60;
    let total_hr = total_min / 60;
    let h = total_hr % 24;
    let mut days = total_hr / 24;

    let mut yr = 1970u64;
    loop {
        let ydays = if yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0) { 366 } else { 365 };
        if days < ydays {
            break;
        }
        days -= ydays;
        yr += 1;
    }

    let leap = yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0);
    let mdays: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mon = 1u64;
    for &md in &mdays {
        if days < md {
            break;
        }
        days -= md;
        mon += 1;
    }
    let day = days + 1;

    format!("{yr:04}-{mon:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("session_id", &self.session_id)
            .field("session_dir", &self.session_dir)
            .field("stores", &self.stores.iter().map(|s| s.store_type()).collect::<Vec<_>>())
            .field("has_tmp_store", &self.tmp_store.is_some())
            .finish()
    }
}
