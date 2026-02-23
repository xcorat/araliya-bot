//! `agent` store — persistent, agent-scoped key-value + text-list storage.
//!
//! Each [`AgentStore`] is rooted at the agent's identity directory (e.g.
//! `memory/agent/news-{pkhash}/store/`) and holds two files:
//!
//! - `kv.json`    — string-keyed scalar map (same format as `basic_session`).
//! - `texts.json` — ordered `Vec<TextItem>` for longer text payloads.
//!
//! Unlike session stores, an `AgentStore` is agent-scoped and persistent
//! across restarts; it is not namespaced by session ID.
//!
//! ## Future session hook
//! Agent-scoped sessions (rooted under the agent dir rather than the global
//! sessions dir) can be wired in later without changing this API.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use super::super::collections::Doc;
use super::super::types::PrimaryValue;
use super::super::{MemorySystem, SessionInfo};
use super::super::handle::SessionHandle;

const KV_FILENAME: &str = "kv.json";
const TEXTS_FILENAME: &str = "texts.json";
const DEFAULT_KV_CAP: usize = 500;

// ── On-disk: KV ──────────────────────────────────────────────────────────────

/// On-disk shape of `kv.json`.  Insertion-ordered for deterministic FIFO eviction.
#[derive(serde::Serialize, serde::Deserialize)]
struct KvFile {
    cap: usize,
    order: Vec<String>,
    values: HashMap<String, String>,
}

impl KvFile {
    fn empty(cap: usize) -> Self {
        Self { cap, order: Vec::new(), values: HashMap::new() }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    fn set(&mut self, key: &str, value: &str) {
        self.order.retain(|k| k != key);
        self.order.push(key.to_string());
        self.values.insert(key.to_string(), value.to_string());
        while self.order.len() > self.cap {
            let oldest = self.order.remove(0);
            self.values.remove(&oldest);
        }
    }

    fn delete(&mut self, key: &str) -> bool {
        let removed = self.values.remove(key).is_some();
        if removed {
            self.order.retain(|k| k != key);
        }
        removed
    }

    fn to_doc(&self) -> Doc {
        let mut doc = Doc::default();
        for (k, v) in &self.values {
            doc.set(k.clone(), PrimaryValue::Str(v.clone()));
        }
        doc
    }
}

// ── On-disk: Texts ────────────────────────────────────────────────────────────

/// A single text entry in the agent's text list.
///
/// `id` is a stable, unique identifier (UUIDv7) assigned on push.
/// `metadata` carries free-form string fields such as `"source"`, `"ts"`,
/// `"subject"`, or `"mime"`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextItem {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl TextItem {
    /// Create a new `TextItem` with a fresh UUIDv7 `id`.
    pub fn new(content: String, metadata: HashMap<String, String>) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            content,
            metadata,
        }
    }
}

// ── AgentStore ────────────────────────────────────────────────────────────────

/// Persistent, agent-scoped store rooted at `{agent_identity_dir}/store/`.
///
/// All I/O is synchronous (blocking).  Call from a `spawn_blocking` context
/// when inside an async task.
pub struct AgentStore {
    dir: PathBuf,
    /// The agent's identity directory (parent of `store/` and `sessions/`).
    pub identity_dir: PathBuf,
}

impl AgentStore {
    /// Open (or create) the store directory and initialise missing files.
    pub fn open(agent_identity_dir: &Path) -> Result<Self, AppError> {
        let dir = agent_identity_dir.join("store");
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::Memory(format!("agent store: cannot create {}: {e}", dir.display())))?;

        let kv_path = dir.join(KV_FILENAME);
        if !kv_path.exists() {
            let empty = KvFile::empty(DEFAULT_KV_CAP);
            Self::write_kv_file(&kv_path, &empty)?;
        }

        let texts_path = dir.join(TEXTS_FILENAME);
        if !texts_path.exists() {
            fs::write(&texts_path, "[]")
                .map_err(|e| AppError::Memory(format!("agent store: cannot create {}: {e}", texts_path.display())))?;
        }

        // Ensure a sessions index exists so agent sessions can be created later.
        let sessions_index = agent_identity_dir.join("sessions.json");
        if !sessions_index.exists() {
            fs::write(&sessions_index, "{\"sessions\":{}}")
                .map_err(|e| AppError::Memory(format!("agent store: cannot create sessions.json: {e}")))?;
        }

        Ok(Self { dir, identity_dir: agent_identity_dir.to_path_buf() })
    }

    // ── KV helpers ────────────────────────────────────────────────────

    fn kv_path(&self) -> PathBuf { self.dir.join(KV_FILENAME) }
    fn texts_path(&self) -> PathBuf { self.dir.join(TEXTS_FILENAME) }

    fn read_kv_file(&self) -> Result<KvFile, AppError> {
        let path = self.kv_path();
        let data = fs::read_to_string(&path)
            .map_err(|e| AppError::Memory(format!("agent store: cannot read {}: {e}", path.display())))?;
        serde_json::from_str(&data)
            .map_err(|e| AppError::Memory(format!("agent store: malformed {}: {e}", path.display())))
    }

    fn write_kv_file(path: &Path, kv: &KvFile) -> Result<(), AppError> {
        let data = serde_json::to_string_pretty(kv)
            .map_err(|e| AppError::Memory(format!("agent store: serialise kv: {e}")))?;
        fs::write(path, data)
            .map_err(|e| AppError::Memory(format!("agent store: cannot write {}: {e}", path.display())))
    }

    // ── KV API ────────────────────────────────────────────────────────

    /// Get a value by key.
    pub fn kv_get(&self, key: &str) -> Result<Option<String>, AppError> {
        let kv = self.read_kv_file()?;
        Ok(kv.get(key).map(|s| s.to_string()))
    }

    /// Set a key-value pair.  Evicts the oldest entry when over cap.
    pub fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let mut kv = self.read_kv_file()?;
        kv.set(key, value);
        Self::write_kv_file(&self.kv_path(), &kv)
    }

    /// Delete a key.  Returns `true` if the key was present.
    pub fn kv_delete(&self, key: &str) -> Result<bool, AppError> {
        let mut kv = self.read_kv_file()?;
        let removed = kv.delete(key);
        if removed {
            Self::write_kv_file(&self.kv_path(), &kv)?;
        }
        Ok(removed)
    }

    /// Return the full KV store as a [`Doc`] collection.
    pub fn kv_all(&self) -> Result<Doc, AppError> {
        Ok(self.read_kv_file()?.to_doc())
    }

    // ── Text-list helpers ─────────────────────────────────────────────

    fn read_texts_file(&self) -> Result<Vec<TextItem>, AppError> {
        let path = self.texts_path();
        let data = fs::read_to_string(&path)
            .map_err(|e| AppError::Memory(format!("agent store: cannot read {}: {e}", path.display())))?;
        serde_json::from_str(&data)
            .map_err(|e| AppError::Memory(format!("agent store: malformed {}: {e}", path.display())))
    }

    fn write_texts_file(&self, items: &[TextItem]) -> Result<(), AppError> {
        let path = self.texts_path();
        let data = serde_json::to_string_pretty(items)
            .map_err(|e| AppError::Memory(format!("agent store: serialise texts: {e}")))?;
        fs::write(&path, data)
            .map_err(|e| AppError::Memory(format!("agent store: cannot write {}: {e}", path.display())))
    }

    // ── Text-list API ─────────────────────────────────────────────────

    /// Return all text items in insertion order.
    pub fn texts_list(&self) -> Result<Vec<TextItem>, AppError> {
        self.read_texts_file()
    }

    /// Append a new item.  Returns the assigned `id`.
    pub fn texts_push(&self, item: TextItem) -> Result<String, AppError> {
        let mut items = self.read_texts_file()?;
        let id = item.id.clone();
        items.push(item);
        self.write_texts_file(&items)?;
        Ok(id)
    }

    /// Replace the entire text list atomically.
    pub fn texts_replace_all(&self, items: Vec<TextItem>) -> Result<(), AppError> {
        self.write_texts_file(&items)
    }

    /// Remove all text items.
    pub fn texts_clear(&self) -> Result<(), AppError> {
        self.write_texts_file(&[])
    }

    // ── Raw file API ──────────────────────────────────────────────────

    fn raw_dir(&self) -> PathBuf { self.dir.join("raw") }

    /// Write raw UTF-8 content to `store/raw/{name}`.
    ///
    /// The `raw/` directory is created on first use.  Overwrites any
    /// existing file with the same name.
    pub fn write_raw(&self, name: &str, content: &str) -> Result<(), AppError> {
        let dir = self.raw_dir();
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::Memory(format!("agent store: cannot create raw dir: {e}")))?;
        let path = dir.join(name);
        fs::write(&path, content)
            .map_err(|e| AppError::Memory(format!("agent store: cannot write raw/{name}: {e}")))
    }

    /// Read raw UTF-8 content from `store/raw/{name}`.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    pub fn read_raw(&self, name: &str) -> Result<Option<String>, AppError> {
        let path = self.raw_dir().join(name);
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Memory(format!("agent store: cannot read raw/{name}: {e}"))),
        }
    }

    // ── Agent session API ─────────────────────────────────────────

    /// Return the root directory for this agent's sessions.
    pub fn agent_sessions_dir(&self) -> PathBuf {
        self.identity_dir.join("sessions")
    }

    /// Return the path to this agent's sessions index file.
    pub fn agent_sessions_index(&self) -> PathBuf {
        self.identity_dir.join("sessions.json")
    }

    /// Get the active session for this agent, creating one on first use.
    ///
    /// The session ID is persisted in the agent KV store under
    /// `active_session_id` so the same rolling transcript is reused across
    /// restarts.  The session lives at `{identity_dir}/sessions/{uuid}/`.
    pub fn get_or_create_session(
        &self,
        memory: &MemorySystem,
    ) -> Result<SessionHandle, AppError> {
        let sessions_root = self.agent_sessions_dir();
        let index_path = self.agent_sessions_index();

        fs::create_dir_all(&sessions_root)
            .map_err(|e| AppError::Memory(format!("agent: cannot create sessions dir: {e}")))?;

        if let Some(sid) = self.kv_get("active_session_id")? {
            match memory.load_session_in(&sessions_root, &index_path, &sid, Some("agent")) {
                Ok(handle) => return Ok(handle),
                Err(_) => {
                    // Session missing from index or dir — create a fresh one.
                    let _ = self.kv_delete("active_session_id");
                }
            }
        }

        let handle = memory.create_session_in(
            &sessions_root,
            &index_path,
            &["basic_session"],
            Some("agent"),
        )?;
        self.kv_set("active_session_id", &handle.session_id)?;
        Ok(handle)
    }

    /// List all sessions stored under this agent's identity directory.
    pub fn list_agent_sessions(&self) -> Result<Vec<SessionInfo>, AppError> {
        MemorySystem::list_sessions_in(&self.agent_sessions_index())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_tmp() -> (TempDir, AgentStore) {
        let dir = TempDir::new().unwrap();
        let store = AgentStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_files() {
        let dir = TempDir::new().unwrap();
        AgentStore::open(dir.path()).unwrap();
        assert!(dir.path().join("store/kv.json").exists());
        assert!(dir.path().join("store/texts.json").exists());
    }

    #[test]
    fn kv_set_get_delete() {
        let (_dir, store) = open_tmp();
        assert_eq!(store.kv_get("x").unwrap(), None);
        store.kv_set("x", "hello").unwrap();
        assert_eq!(store.kv_get("x").unwrap(), Some("hello".into()));
        store.kv_set("x", "world").unwrap();
        assert_eq!(store.kv_get("x").unwrap(), Some("world".into()));
        assert!(store.kv_delete("x").unwrap());
        assert_eq!(store.kv_get("x").unwrap(), None);
        assert!(!store.kv_delete("x").unwrap());
    }

    #[test]
    fn kv_all_returns_doc() {
        let (_dir, store) = open_tmp();
        store.kv_set("a", "1").unwrap();
        store.kv_set("b", "2").unwrap();
        let doc = store.kv_all().unwrap();
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.get("a"), Some(&PrimaryValue::Str("1".into())));
    }

    #[test]
    fn texts_push_list_clear() {
        let (_dir, store) = open_tmp();
        assert!(store.texts_list().unwrap().is_empty());
        let id = store.texts_push(TextItem::new("hello".into(), HashMap::new())).unwrap();
        let items = store.texts_list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(items[0].content, "hello");
        store.texts_clear().unwrap();
        assert!(store.texts_list().unwrap().is_empty());
    }

    #[test]
    fn texts_replace_all() {
        let (_dir, store) = open_tmp();
        store.texts_push(TextItem::new("old".into(), HashMap::new())).unwrap();
        let new_items = vec![
            TextItem::new("a".into(), HashMap::new()),
            TextItem::new("b".into(), HashMap::new()),
        ];
        store.texts_replace_all(new_items).unwrap();
        let items = store.texts_list().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "a");
        assert_eq!(items[1].content, "b");
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let s1 = AgentStore::open(dir.path()).unwrap();
        s1.kv_set("k", "v").unwrap();
        let s2 = AgentStore::open(dir.path()).unwrap();
        assert_eq!(s2.kv_get("k").unwrap(), Some("v".into()));
    }

    #[test]
    fn write_and_read_raw() {
        let (_dir, store) = open_tmp();
        assert_eq!(store.read_raw("a.json").unwrap(), None);
        store.write_raw("a.json", "[{\"x\":1}]").unwrap();
        assert_eq!(store.read_raw("a.json").unwrap(), Some("[{\"x\":1}]".to_string()));
        // Overwrite
        store.write_raw("a.json", "[]").unwrap();
        assert_eq!(store.read_raw("a.json").unwrap(), Some("[]".to_string()));
    }

    #[test]
    fn write_raw_creates_subdirectory() {
        let dir = TempDir::new().unwrap();
        let store = AgentStore::open(dir.path()).unwrap();
        store.write_raw("data.txt", "hello").unwrap();
        assert!(dir.path().join("store/raw/data.txt").exists());
    }
}
