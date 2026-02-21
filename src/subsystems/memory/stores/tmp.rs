//! `tmp` store — ephemeral, typed in-memory store.
//!
//! Each [`TmpStore`] instance owns a single [`Store`] (the `RwLock`-backed
//! collection map from [`store`](super::super::store)).  Two lifecycles:
//!
//! **Standalone** — obtained via [`MemorySystem::create_tmp_store`].  Use
//! [`doc`](TmpStore::doc) / [`block`](TmpStore::block) to read/write
//! the pre-populated collections directly.
//!
//! **Session-backed** — the shared instance registered in
//! [`MemorySystem`](super::super::MemorySystem) under store type `"tmp"`.
//! [`SessionStore::init`] creates per-session namespaced collections
//! (`"{dir}:doc"`, `"{dir}:block"`), keeping sessions isolated while sharing
//! one backing [`Store`].  K-V operations delegate to the `"doc"` collection.

use std::path::Path;

use crate::error::AppError;
use super::super::collections::{Block, Collection, Doc};
use super::super::store::{SessionStore, Store};
use super::super::types::PrimaryValue;

/// Ephemeral, typed in-memory store backed by a [`Store`].
///
/// See the module-level documentation for usage in standalone vs.
/// session-backed modes.
pub struct TmpStore {
    store: Store,
}

impl TmpStore {
    /// Create a new `TmpStore` with empty `"doc"` and `"block"` collections.
    ///
    /// The pre-populated collections are used in standalone mode.  When
    /// registered in `MemorySystem`, per-session collections are created by
    /// [`SessionStore::init`] and namespaced by session directory.
    pub fn new() -> Self {
        let store = Store::new();
        store
            .insert_collection("doc".into(), Collection::Doc(Doc::default()))
            .expect("fresh store cannot be poisoned");
        store
            .insert_collection("block".into(), Collection::Block(Block::default()))
            .expect("fresh store cannot be poisoned");
        Self { store }
    }

    // ── Standalone convenience methods ────────────────────────────────

    /// Return a snapshot clone of the `"doc"` collection.
    ///
    /// Mutations to the returned value are not automatically persisted.
    /// Call [`set_doc`](Self::set_doc) to write changes back.
    pub fn doc(&self) -> Result<Doc, AppError> {
        self.store
            .get_collection("doc")?
            .and_then(|c| c.into_doc())
            .ok_or_else(|| AppError::Memory("`doc` collection missing or wrong type".into()))
    }

    /// Return a snapshot clone of the `"block"` collection.
    ///
    /// Call [`set_block`](Self::set_block) to write changes back.
    pub fn block(&self) -> Result<Block, AppError> {
        self.store
            .get_collection("block")?
            .and_then(|c| c.into_block())
            .ok_or_else(|| AppError::Memory("`block` collection missing or wrong type".into()))
    }

    /// Replace the `"doc"` collection with a modified snapshot.
    pub fn set_doc(&self, doc: Doc) -> Result<(), AppError> {
        self.store.insert_collection("doc".into(), Collection::Doc(doc))
    }

    /// Replace the `"block"` collection with a modified snapshot.
    pub fn set_block(&self, block: Block) -> Result<(), AppError> {
        self.store.insert_collection("block".into(), Collection::Block(block))
    }

    /// Direct access to the underlying [`Store`] for label inspection or
    /// collection manipulation beyond the convenience API.
    pub fn inner(&self) -> &Store {
        &self.store
    }

    // ── Session-namespace helpers ─────────────────────────────────────

    fn doc_label(session_dir: &Path) -> String {
        format!("{}:doc", session_dir.display())
    }

    fn block_label(session_dir: &Path) -> String {
        format!("{}:block", session_dir.display())
    }
}

impl Default for TmpStore {
    fn default() -> Self { Self::new() }
}

impl SessionStore for TmpStore {
    fn store_type(&self) -> &str {
        "tmp"
    }

    /// Initialise isolated `"doc"` and `"block"` collections for this session.
    ///
    /// Purely in-memory — no files are created regardless of `session_dir`.
    fn init(&self, session_dir: &Path) -> Result<(), AppError> {
        self.store.insert_collection(
            Self::doc_label(session_dir),
            Collection::Doc(Doc::default()),
        )?;
        self.store.insert_collection(
            Self::block_label(session_dir),
            Collection::Block(Block::default()),
        )?;
        Ok(())
    }

    /// Read a string value from this session's `"doc"` collection.
    fn kv_get(&self, session_dir: &Path, key: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .store
            .get_collection(&Self::doc_label(session_dir))?
            .and_then(|c| c.into_doc())
            .and_then(|doc| doc.get(key).map(|v| v.to_string())))
    }

    /// Write a string value into this session's `"doc"` collection.
    fn kv_set(&self, session_dir: &Path, key: &str, value: &str) -> Result<(), AppError> {
        let label = Self::doc_label(session_dir);
        let mut doc = self
            .store
            .get_collection(&label)?
            .and_then(|c| c.into_doc())
            .unwrap_or_default();
        doc.set(key.to_string(), PrimaryValue::Str(value.to_string()));
        self.store.insert_collection(label, Collection::Doc(doc))
    }

    /// Remove a key from this session's `"doc"` collection.
    fn kv_delete(&self, session_dir: &Path, key: &str) -> Result<bool, AppError> {
        let label = Self::doc_label(session_dir);
        let Some(mut doc) = self
            .store
            .get_collection(&label)?
            .and_then(|c| c.into_doc())
        else {
            return Ok(false);
        };
        let removed = doc.delete(key);
        self.store.insert_collection(label, Collection::Doc(doc))?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Standalone API ────────────────────────────────────────────────

    #[test]
    fn new_has_doc_and_block() {
        let ts = TmpStore::new();
        assert!(ts.doc().unwrap().is_empty());
        assert!(ts.block().unwrap().is_empty());
    }

    #[test]
    fn set_and_get_doc() {
        let ts = TmpStore::new();
        let mut doc = ts.doc().unwrap();
        doc.set("greeting".into(), PrimaryValue::from("hello"));
        ts.set_doc(doc).unwrap();
        assert_eq!(
            ts.doc().unwrap().get("greeting"),
            Some(&PrimaryValue::from("hello"))
        );
    }

    #[test]
    fn inner_store_is_accessible() {
        let ts = TmpStore::new();
        // "doc" and "block" labels present for standalone use
        let mut labels = ts.inner().labels().unwrap();
        labels.sort();
        assert_eq!(labels, vec!["block", "doc"]);
    }

    // ── SessionStore API ──────────────────────────────────────────────

    #[test]
    fn kv_set_get_delete() {
        let store = TmpStore::new();
        let dir = PathBuf::from("/tmp/session1");
        store.init(&dir).unwrap();

        assert_eq!(store.kv_get(&dir, "foo").unwrap(), None);
        store.kv_set(&dir, "foo", "bar").unwrap();
        assert_eq!(store.kv_get(&dir, "foo").unwrap(), Some("bar".into()));
        store.kv_set(&dir, "foo", "baz").unwrap();
        assert_eq!(store.kv_get(&dir, "foo").unwrap(), Some("baz".into()));
        assert!(store.kv_delete(&dir, "foo").unwrap());
        assert_eq!(store.kv_get(&dir, "foo").unwrap(), None);
        assert!(!store.kv_delete(&dir, "foo").unwrap());
    }

    #[test]
    fn sessions_are_independent() {
        let store = TmpStore::new();
        let dir1 = PathBuf::from("/tmp/session_a");
        let dir2 = PathBuf::from("/tmp/session_b");
        store.init(&dir1).unwrap();
        store.init(&dir2).unwrap();

        store.kv_set(&dir1, "key", "val1").unwrap();
        store.kv_set(&dir2, "key", "val2").unwrap();

        assert_eq!(store.kv_get(&dir1, "key").unwrap(), Some("val1".into()));
        assert_eq!(store.kv_get(&dir2, "key").unwrap(), Some("val2".into()));
    }

    #[test]
    fn init_is_in_memory_only() {
        let store = TmpStore::new();
        // Must not error even for a non-existent path — no files are created.
        store.init(Path::new("/nonexistent/path")).unwrap();
    }

    #[test]
    fn store_type_is_tmp() {
        assert_eq!(TmpStore::new().store_type(), "tmp");
    }

    #[test]
    fn inner_labels_after_two_sessions() {
        let store = TmpStore::new();
        let a = PathBuf::from("/s/a");
        let b = PathBuf::from("/s/b");
        store.init(&a).unwrap();
        store.init(&b).unwrap();
        // 2 standalone + 2×2 session-namespaced = 6 labels
        let labels = store.inner().labels().unwrap();
        assert_eq!(labels.len(), 6);
        assert!(labels.iter().any(|l| l.ends_with(":doc")));
        assert!(labels.iter().any(|l| l.ends_with(":block")));
    }
}
