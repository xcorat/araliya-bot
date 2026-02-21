//! Store trait and collection store abstraction.
//!
//! * [`SessionStore`] — trait implemented by pluggable session backends
//!   (disk-backed `BasicSessionStore`, in-memory `TmpStore`, …).
//!   Contains K-V and transcript I/O routed through session directories.
//!
//! * [`Store`] — new in-process collection map; a labelled set of
//!   [`Collection`] values protected by an `RwLock`.  This is the
//!   primary abstraction for the new typed memory model.

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

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

/// Pluggable session-backed memory store.
///
/// Implementations are `Send + Sync` and operate on a session directory via
/// blocking file I/O.  The [`SessionHandle`](super::handle::SessionHandle)
/// wraps these calls in `spawn_blocking`.
///
/// This trait is intentionally named `SessionStore` to distinguish it from
/// the in-process [`Store`] struct added alongside it.
pub trait SessionStore: Send + Sync {
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

// ── Store struct ──────────────────────────────────────────────────────────────

/// An in-process collection map: a labelled set of [`Collection`] values
/// protected by an `RwLock`.
///
/// This is the new core memory abstraction for agents that want typed,
/// in-memory storage.  It is distinct from the [`SessionStore`] trait used
/// by disk-backed session backends.
///
/// All operations acquire the lock internally, so the struct itself requires
/// only a shared reference (`&self`) for both reads and writes.
///
/// # Example
/// ```rust,no_run
/// use araliya_bot::subsystems::memory::store::Store;
/// use araliya_bot::subsystems::memory::collections::{Collection, Doc};
///
/// let store = Store::new();
/// store.insert_collection("meta".into(), Collection::Doc(Doc::default())).unwrap();
/// assert!(store.labels().unwrap().contains(&"meta".to_string()));
/// ```
pub struct Store {
    collections: RwLock<HashMap<String, super::collections::Collection>>,
}

impl Store {
    /// Create an empty store with no collections.
    pub fn new() -> Self {
        Self { collections: RwLock::new(HashMap::new()) }
    }

    /// Retrieve a *clone* of the collection stored under `label`, or `None`.
    ///
    /// Clones are returned rather than references because the `RwLock` guard
    /// cannot be held across `await` points and leaking it would couple the
    /// caller to the lock lifetime.
    pub fn get_collection(
        &self,
        label: &str,
    ) -> Result<Option<super::collections::Collection>, AppError> {
        let guard = self.collections
            .read()
            .map_err(|_| AppError::Memory("Store RwLock poisoned (read)".into()))?;
        Ok(guard.get(label).cloned())
    }

    /// Insert or overwrite the collection stored under `label`.
    pub fn insert_collection(
        &self,
        label: String,
        collection: super::collections::Collection,
    ) -> Result<(), AppError> {
        let mut guard = self.collections
            .write()
            .map_err(|_| AppError::Memory("Store RwLock poisoned (write)".into()))?;
        guard.insert(label, collection);
        Ok(())
    }

    /// Remove the collection stored under `label`.  Returns `true` if it
    /// was present.
    pub fn remove_collection(&self, label: &str) -> Result<bool, AppError> {
        let mut guard = self.collections
            .write()
            .map_err(|_| AppError::Memory("Store RwLock poisoned (write)".into()))?;
        Ok(guard.remove(label).is_some())
    }

    /// All labels present in the store, in arbitrary order.
    pub fn labels(&self) -> Result<Vec<String>, AppError> {
        let guard = self.collections
            .read()
            .map_err(|_| AppError::Memory("Store RwLock poisoned (read)".into()))?;
        Ok(guard.keys().cloned().collect())
    }

    /// Number of collections currently in the store.
    pub fn len(&self) -> Result<usize, AppError> {
        let guard = self.collections
            .read()
            .map_err(|_| AppError::Memory("Store RwLock poisoned (read)".into()))?;
        Ok(guard.len())
    }

    /// `true` when no collections are present.
    pub fn is_empty(&self) -> Result<bool, AppError> {
        Ok(self.len()? == 0)
    }
}

impl Default for Store {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::collections::{Collection, Doc, Block};
    use super::super::types::PrimaryValue;

    #[test]
    fn insert_and_get() {
        let store = Store::new();
        let mut doc = Doc::default();
        doc.set("k".into(), PrimaryValue::Int(1));
        store.insert_collection("meta".into(), Collection::Doc(doc)).unwrap();

        let c = store.get_collection("meta").unwrap().unwrap();
        assert_eq!(c.as_doc().unwrap().get("k"), Some(&PrimaryValue::Int(1)));
    }

    #[test]
    fn missing_label_returns_none() {
        let store = Store::new();
        assert!(store.get_collection("nope").unwrap().is_none());
    }

    #[test]
    fn remove_returns_presence() {
        let store = Store::new();
        store.insert_collection("x".into(), Collection::Block(Block::default())).unwrap();
        assert!(store.remove_collection("x").unwrap());
        assert!(!store.remove_collection("x").unwrap());
    }

    #[test]
    fn labels_and_len() {
        let store = Store::new();
        assert!(store.is_empty().unwrap());
        store.insert_collection("a".into(), Collection::Doc(Doc::default())).unwrap();
        store.insert_collection("b".into(), Collection::Block(Block::default())).unwrap();
        assert_eq!(store.len().unwrap(), 2);
        let mut lbls = store.labels().unwrap();
        lbls.sort();
        assert_eq!(lbls, vec!["a", "b"]);
    }

    #[test]
    fn overwrite_collection() {
        let store = Store::new();
        store.insert_collection("c".into(), Collection::Doc(Doc::default())).unwrap();
        store.insert_collection("c".into(), Collection::Block(Block::default())).unwrap();
        let c = store.get_collection("c").unwrap().unwrap();
        assert!(c.as_block().is_some(), "should have been overwritten with Block");
    }

    #[test]
    fn concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(Store::new());
        store.insert_collection("shared".into(), Collection::Doc(Doc::default())).unwrap();

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let s = store.clone();
                thread::spawn(move || {
                    let c = s.get_collection("shared").unwrap().unwrap();
                    assert!(c.as_doc().is_some());
                })
            })
            .collect();

        for h in handles { h.join().unwrap(); }
    }
}
