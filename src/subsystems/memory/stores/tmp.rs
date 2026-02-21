//! `tmp` store — ephemeral in-memory key-value store.
//!
//! All data lives in process memory and is discarded when the process exits.
//! The `session_dir` path is used as a session-namespace key so that multiple
//! sessions sharing the same `TmpStore` instance remain independent.
//! `init` is a no-op — no files are written to disk.
//!
//! Registered by default in every [`MemorySystem`](super::super::MemorySystem)
//! under the store type name `"tmp"`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::error::AppError;
use super::super::store::Store;

/// Ephemeral in-memory key-value store.
///
/// Data is scoped per session using the session directory path as a namespace
/// key, so all sessions that reference this store instance remain isolated.
pub struct TmpStore {
    /// session_dir_string -> key -> value
    data: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl TmpStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    fn session_key(session_dir: &Path) -> String {
        session_dir.to_string_lossy().into_owned()
    }
}

impl Store for TmpStore {
    fn store_type(&self) -> &str {
        "tmp"
    }

    /// No-op: the tmp store holds no files.
    fn init(&self, _session_dir: &Path) -> Result<(), AppError> {
        Ok(())
    }

    fn kv_get(&self, session_dir: &Path, key: &str) -> Result<Option<String>, AppError> {
        let data = self
            .data
            .lock()
            .map_err(|_| AppError::Memory("tmp store lock poisoned".into()))?;
        Ok(data
            .get(&Self::session_key(session_dir))
            .and_then(|m| m.get(key))
            .cloned())
    }

    fn kv_set(&self, session_dir: &Path, key: &str, value: &str) -> Result<(), AppError> {
        let mut data = self
            .data
            .lock()
            .map_err(|_| AppError::Memory("tmp store lock poisoned".into()))?;
        data.entry(Self::session_key(session_dir))
            .or_default()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn kv_delete(&self, session_dir: &Path, key: &str) -> Result<bool, AppError> {
        let mut data = self
            .data
            .lock()
            .map_err(|_| AppError::Memory("tmp store lock poisoned".into()))?;
        Ok(data
            .get_mut(&Self::session_key(session_dir))
            .map(|m| m.remove(key).is_some())
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn kv_set_get_delete() {
        let store = TmpStore::new();
        let dir = PathBuf::from("/tmp/session1");

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

        store.kv_set(&dir1, "key", "val1").unwrap();
        store.kv_set(&dir2, "key", "val2").unwrap();

        assert_eq!(store.kv_get(&dir1, "key").unwrap(), Some("val1".into()));
        assert_eq!(store.kv_get(&dir2, "key").unwrap(), Some("val2".into()));
    }

    #[test]
    fn init_is_noop() {
        let store = TmpStore::new();
        let dir = PathBuf::from("/nonexistent/path");
        // Must not error even if the directory does not exist.
        store.init(&dir).unwrap();
    }

    #[test]
    fn store_type_is_tmp() {
        let store = TmpStore::new();
        assert_eq!(store.store_type(), "tmp");
    }
}
