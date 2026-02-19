//! [`SessionHandle`] — async-safe handle for reading and writing session data.
//!
//! Agents receive a `SessionHandle` when they create or load a session.
//! All I/O is dispatched to a blocking thread pool so callers remain
//! non-blocking.

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::AppError;
use super::store::{Store, TranscriptEntry};

/// Async-safe handle to a single session's stores.
///
/// Cheaply cloneable (`Arc`-backed).  All file I/O runs on
/// `tokio::task::spawn_blocking` so callers can hold this in async code.
#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    session_dir: PathBuf,
    stores: Vec<Arc<dyn Store>>,
}

impl SessionHandle {
    pub(crate) fn new(
        session_id: String,
        session_dir: PathBuf,
        stores: Vec<Arc<dyn Store>>,
    ) -> Self {
        Self {
            session_id,
            session_dir,
            stores,
        }
    }

    /// Get the first store that matches `store_type`, or the first store if
    /// only one is registered.
    fn find_store(&self, store_type: &str) -> Result<Arc<dyn Store>, AppError> {
        self.stores
            .iter()
            .find(|s| s.store_type() == store_type)
            .or_else(|| self.stores.first())
            .cloned()
            .ok_or_else(|| AppError::Memory("no stores registered for session".into()))
    }

    /// Convenience: get the first store (for single-store sessions like basic_session).
    fn default_store(&self) -> Result<Arc<dyn Store>, AppError> {
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
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("session_id", &self.session_id)
            .field("session_dir", &self.session_dir)
            .field("stores", &self.stores.iter().map(|s| s.store_type()).collect::<Vec<_>>())
            .finish()
    }
}
