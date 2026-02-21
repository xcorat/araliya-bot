//! [`SessionHandle`] — async-safe handle for reading and writing session data.
//!
//! `SessionHandle` is intentionally lightweight: it carries identity metadata
//! (`session_id`) and delegates all data read/write behavior to [`SessionRw`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::AppError;

use super::collections::{Block, Doc};
pub use super::rw::SessionFileInfo;
use super::rw::SessionRw;
use super::store::{SessionStore, TranscriptEntry};
use super::stores::tmp::TmpStore;

#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    rw: Arc<SessionRw>,
}

impl SessionHandle {
    pub(crate) fn new(
        session_id: String,
        session_dir: PathBuf,
        stores: Vec<Arc<dyn SessionStore>>,
        tmp_store: Option<Arc<TmpStore>>,
    ) -> Self {
        Self {
            session_id,
            rw: Arc::new(SessionRw::new(session_dir, stores, tmp_store)),
        }
    }

    pub async fn kv_get(&self, key: &str) -> Result<Option<String>, AppError> {
        self.rw.kv_get(key).await
    }

    pub async fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.rw.kv_set(key, value).await
    }

    pub async fn kv_delete(&self, key: &str) -> Result<bool, AppError> {
        self.rw.kv_delete(key).await
    }

    pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError> {
        self.rw.transcript_append(role, content).await
    }

    pub async fn transcript_read_last(&self, n: usize) -> Result<Vec<TranscriptEntry>, AppError> {
        self.rw.transcript_read_last(n).await
    }

    pub async fn working_memory_read(&self) -> Result<String, AppError> {
        Ok(self.kv_get("working_memory").await?.unwrap_or_default())
    }

    pub async fn kv_doc(&self) -> Result<Doc, AppError> {
        self.rw.kv_doc().await
    }

    pub async fn transcript_block(&self) -> Result<Block, AppError> {
        self.rw.transcript_block().await
    }

    pub fn tmp_doc(&self) -> Result<Doc, AppError> {
        self.rw.tmp_doc()
    }

    pub fn tmp_block(&self) -> Result<Block, AppError> {
        self.rw.tmp_block()
    }

    pub fn set_tmp_doc(&self, doc: Doc) -> Result<(), AppError> {
        self.rw.set_tmp_doc(doc)
    }

    pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError> {
        self.rw.set_tmp_block(block)
    }

    pub async fn list_files(&self) -> Result<Vec<SessionFileInfo>, AppError> {
        self.rw.list_files().await
    }
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("session_id", &self.session_id)
            .field("session_dir", &self.rw.session_dir())
            .field("stores", &self.rw.store_types())
            .field("has_tmp_store", &self.rw.has_tmp_store())
            .finish()
    }
}
