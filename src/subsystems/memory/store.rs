//! Store trait — defines the data operations a memory store supports.
//!
//! Stores are pluggable backends that live inside a session directory.
//! Each store type creates its own files and implements whichever
//! operations it supports (k-v, transcript, etc.).  Default trait methods
//! return [`MemoryError::Unsupported`] so stores only implement what they need.

use std::path::Path;

use crate::error::AppError;

/// A single key-value entry with a timestamp.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KvEntry {
    pub key: String,
    pub value: String,
    /// ISO-8601 timestamp of when this entry was written.
    pub ts: String,
}

/// A parsed transcript entry.
#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    pub role: String,
    pub timestamp: String,
    pub content: String,
}

/// Pluggable memory store.
///
/// Stores are `Send + Sync` and operate on a session directory via
/// blocking file I/O.  The [`SessionHandle`](super::handle::SessionHandle)
/// wraps these calls in `spawn_blocking`.
pub trait Store: Send + Sync {
    /// Unique type name for this store (e.g. `"basic_session"`).
    fn store_type(&self) -> &str;

    /// Initialise store files inside `session_dir`.
    fn init(&self, session_dir: &Path) -> Result<(), AppError>;

    // ── Key-value operations ──────────────────────────────────────────

    fn kv_get(&self, _session_dir: &Path, _key: &str) -> Result<Option<String>, AppError> {
        Err(AppError::Memory(format!(
            "store '{}' does not support kv_get",
            self.store_type()
        )))
    }

    fn kv_set(&self, _session_dir: &Path, _key: &str, _value: &str) -> Result<(), AppError> {
        Err(AppError::Memory(format!(
            "store '{}' does not support kv_set",
            self.store_type()
        )))
    }

    fn kv_delete(&self, _session_dir: &Path, _key: &str) -> Result<bool, AppError> {
        Err(AppError::Memory(format!(
            "store '{}' does not support kv_delete",
            self.store_type()
        )))
    }

    // ── Transcript operations ─────────────────────────────────────────

    fn transcript_append(
        &self,
        _session_dir: &Path,
        _role: &str,
        _content: &str,
    ) -> Result<(), AppError> {
        Err(AppError::Memory(format!(
            "store '{}' does not support transcript_append",
            self.store_type()
        )))
    }

    fn transcript_read_last(
        &self,
        _session_dir: &Path,
        _n: usize,
    ) -> Result<Vec<TranscriptEntry>, AppError> {
        Err(AppError::Memory(format!(
            "store '{}' does not support transcript_read_last",
            self.store_type()
        )))
    }
}
