//! Session-scoped read/write operations shared by handle-like frontends.
//!
//! `SessionRw` centralizes store selection, blocking I/O dispatch, and tmp-store
//! typed collection access. [`SessionHandle`](super::handle::SessionHandle)
//! delegates all data operations to this struct.

use std::path::{Path, PathBuf};
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

pub struct SessionRw {
    session_dir: PathBuf,
    stores: Vec<Arc<dyn SessionStore>>,
    tmp_store: Option<Arc<TmpStore>>,
}

impl SessionRw {
    pub fn new(
        session_dir: PathBuf,
        stores: Vec<Arc<dyn SessionStore>>,
        tmp_store: Option<Arc<TmpStore>>,
    ) -> Self {
        Self { session_dir, stores, tmp_store }
    }

    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    pub fn store_types(&self) -> Vec<String> {
        self.stores.iter().map(|s| s.store_type().to_string()).collect()
    }

    pub fn has_tmp_store(&self) -> bool {
        self.tmp_store.is_some()
    }

    fn default_store(&self) -> Result<Arc<dyn SessionStore>, AppError> {
        self.stores
            .first()
            .cloned()
            .ok_or_else(|| AppError::Memory("no stores registered for session".into()))
    }

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

    pub async fn kv_doc(&self) -> Result<Doc, AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        tokio::task::spawn_blocking(move || store.read_kv_doc(&dir))
            .await
            .map_err(|e| AppError::Memory(format!("kv_doc join: {e}")))?
    }

    pub async fn transcript_block(&self) -> Result<Block, AppError> {
        let store = self.default_store()?;
        let dir = self.session_dir.clone();
        tokio::task::spawn_blocking(move || store.read_transcript_block(&dir))
            .await
            .map_err(|e| AppError::Memory(format!("transcript_block join: {e}")))?
    }

    pub fn tmp_doc(&self) -> Result<Doc, AppError> {
        self.tmp_store()?
            .inner()
            .get_collection(&self.tmp_label("doc"))?
            .and_then(|c| c.into_doc())
            .ok_or_else(|| AppError::Memory("tmp session 'doc' collection not found".into()))
    }

    pub fn tmp_block(&self) -> Result<Block, AppError> {
        self.tmp_store()?
            .inner()
            .get_collection(&self.tmp_label("block"))?
            .and_then(|c| c.into_block())
            .ok_or_else(|| AppError::Memory("tmp session 'block' collection not found".into()))
    }

    pub fn set_tmp_doc(&self, doc: Doc) -> Result<(), AppError> {
        self.tmp_store()?.inner().insert_collection(
            self.tmp_label("doc"),
            Collection::Doc(doc),
        )
    }

    pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError> {
        self.tmp_store()?.inner().insert_collection(
            self.tmp_label("block"),
            Collection::Block(block),
        )
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

    fn tmp_store(&self) -> Result<&Arc<TmpStore>, AppError> {
        self.tmp_store
            .as_ref()
            .ok_or_else(|| AppError::Memory("session has no tmp store (not a 'tmp' session)".into()))
    }

    fn tmp_label(&self, kind: &str) -> String {
        format!("{}:{kind}", self.session_dir.display())
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
