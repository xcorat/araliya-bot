//! Memory subsystem — bot-scoped session management with pluggable stores.
//!
//! The memory system owns a root directory under the bot's identity dir
//! (`{identity_dir}/memory/`) and manages sessions within it.
//!
//! Sessions are bot-scoped: any agent can access any session if it has the
//! session ID.  A lightweight index (`sessions.json`) tracks metadata.
//!
//! ```text
//! {identity_dir}/
//! └── memory/
//!     ├── sessions.json
//!     └── sessions/
//!         └── {session_id}/
//!             ├── kv.json
//!             └── transcript.md
//! ```

pub mod collections;
pub mod handle;
pub mod rw;
pub mod store;
pub mod stores;
pub mod types;

// Re-export the core type vocabulary so callers can write
// `memory::PrimaryValue` etc. without spelling out the sub-module.
// Suppressed until later phases start consuming these types.
#[allow(unused_imports)]
pub use collections::{Block, Collection, Doc};
#[allow(unused_imports)]
pub use store::Store;
#[allow(unused_imports)]
pub use types::{Obj, PrimaryValue, TextFile, Value};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::error::AppError;
use handle::SessionHandle;
use store::SessionStore;

/// Metadata for a single session, persisted in `sessions.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub created_at: String,
    pub store_types: Vec<String>,
    /// Last agent that accessed this session (informational).
    #[serde(default)]
    pub last_agent: Option<String>,
    /// Aggregate token spend for this session, mirrored from `spend.json`.
    #[serde(default)]
    pub spend: Option<SessionSpend>,
}

/// Aggregate token and cost totals for a session.
/// Persisted as `sessions/{session_id}/spend.json` and mirrored into `sessions.json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionSpend {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cached_tokens: u64,
    /// Cumulative cost in USD. Recomputed on every accumulation using current rates.
    pub total_cost_usd: f64,
    /// ISO-8601 timestamp of the last accumulation.
    pub last_updated: String,
}

/// On-disk shape of `sessions.json`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SessionIndex {
    sessions: HashMap<String, SessionInfo>,
}

impl Default for SessionIndex {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }
}

/// Configuration for the memory subsystem.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Cap for k-v entries in `basic_session` store.
    pub kv_cap: Option<usize>,
    /// Cap for transcript entries in `basic_session` store.
    pub transcript_cap: Option<usize>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            kv_cap: None,
            transcript_cap: None,
        }
    }
}

/// Central memory system.  Constructed once at startup, shared via `Arc`.
pub struct MemorySystem {
    memory_root: PathBuf,
    sessions_dir: PathBuf,
    stores: HashMap<String, Arc<dyn SessionStore>>,
    /// Typed reference to the shared `TmpStore` instance, used to populate
    /// [`SessionHandle::tmp_store`] for sessions created with store type `"tmp"`.
    tmp_store: Arc<stores::tmp::TmpStore>,
}

impl MemorySystem {
    /// Create or open the memory root at `{identity_dir}/memory/`.
    pub fn new(identity_dir: &Path, config: MemoryConfig) -> Result<Self, AppError> {
        let memory_root = identity_dir.join("memory");
        let sessions_dir = memory_root.join("sessions");

        fs::create_dir_all(&sessions_dir)
            .map_err(|e| AppError::Memory(format!("cannot create {}: {e}", sessions_dir.display())))?;

        // Ensure index file exists.
        let index_path = memory_root.join("sessions.json");
        if !index_path.exists() {
            let idx = SessionIndex::default();
            let data = serde_json::to_string_pretty(&idx)
                .map_err(|e| AppError::Memory(format!("serialise index: {e}")))?;
            fs::write(&index_path, data)
                .map_err(|e| AppError::Memory(format!("cannot write {}: {e}", index_path.display())))?;
        }

        // Register built-in stores.
        let mut stores: HashMap<String, Arc<dyn SessionStore>> = HashMap::new();
        let basic = Arc::new(stores::basic_session::BasicSessionStore::new(
            config.kv_cap,
            config.transcript_cap,
        ));
        stores.insert(basic.store_type().to_string(), basic);
        let tmp = Arc::new(stores::tmp::TmpStore::new());
        stores.insert(tmp.store_type().to_string(), tmp.clone() as Arc<dyn SessionStore>);

        info!(
            memory_root = %memory_root.display(),
            registered_stores = ?stores.keys().collect::<Vec<_>>(),
            "memory system initialised"
        );

        Ok(Self {
            memory_root,
            sessions_dir,
            stores,
            tmp_store: tmp,
        })
    }

    pub fn memory_root(&self) -> &Path {
        &self.memory_root
    }

    /// Create a new session with the given store types.
    ///
    /// Returns a [`SessionHandle`] for reading and writing session data.
    pub fn create_session(
        &self,
        store_types: &[&str],
        agent_id: Option<&str>,
    ) -> Result<SessionHandle, AppError> {
        // Validate that all requested stores are registered.
        let mut session_stores = Vec::new();
        for &st in store_types {
            let store = self.stores.get(st).ok_or_else(|| {
                AppError::Memory(format!("unknown store type: {st}"))
            })?;
            session_stores.push(store.clone());
        }

        // Generate UUIDv7 (time-ordered).
        let session_id = uuid::Uuid::now_v7().to_string();
        let session_dir = self.sessions_dir.join(&session_id);

        // Skip disk I/O for purely in-memory sessions.
        let all_tmp = store_types.iter().all(|&s| s == "tmp");
        if !all_tmp {
            fs::create_dir_all(&session_dir)
                .map_err(|e| AppError::Memory(format!(
                    "cannot create session dir {}: {e}",
                    session_dir.display()
                )))?;
        }

        // Initialise each store's files (no-op for TmpStore).
        for store in &session_stores {
            store.init(&session_dir)?;
        }

        // Update index.
        let now = now_iso8601();
        let info = SessionInfo {
            session_id: session_id.clone(),
            created_at: now,
            store_types: store_types.iter().map(|s| s.to_string()).collect(),
            last_agent: agent_id.map(|s| s.to_string()),
            spend: None,
        };
        self.update_index(|idx| {
            idx.sessions.insert(session_id.clone(), info);
        })?;

        // Attach the typed TmpStore reference when the session uses it.
        let tmp_store = store_types
            .contains(&"tmp")
            .then(|| self.tmp_store.clone());

        info!(
            session_id = %session_id,
            stores = ?store_types,
            agent = ?agent_id,
            "session created"
        );

        Ok(SessionHandle::new(session_id, session_dir, session_stores, tmp_store))
    }

    /// Load an existing session by ID.
    ///
    /// Returns `Err` if the session does not exist in the index.
    pub fn load_session(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionHandle, AppError> {
        // Read index first — the session must be registered regardless of type.
        let idx = self.read_index()?;
        let info = idx.sessions.get(session_id).ok_or_else(|| {
            AppError::Memory(format!("session not found: {session_id}"))
        })?;

        let session_dir = self.sessions_dir.join(session_id);

        // Disk-backed sessions require the directory to be present.
        let all_tmp = info.store_types.iter().all(|s| s == "tmp");
        if !all_tmp && !session_dir.exists() {
            return Err(AppError::Memory(format!(
                "session dir missing for {session_id}"
            )));
        }

        let mut session_stores = Vec::new();
        for st in &info.store_types {
            let store = self.stores.get(st.as_str()).ok_or_else(|| {
                AppError::Memory(format!(
                    "session {session_id} requires store '{st}' which is not registered"
                ))
            })?;
            session_stores.push(store.clone());
        }

        // Update last_agent in index.
        if let Some(agent) = agent_id {
            let agent = agent.to_string();
            let sid = session_id.to_string();
            self.update_index(|idx| {
                if let Some(info) = idx.sessions.get_mut(&sid) {
                    info.last_agent = Some(agent);
                }
            })?;
        }

        info!(session_id = %session_id, agent = ?agent_id, "session loaded");

        let tmp_store = info
            .store_types
            .iter()
            .any(|s| s == "tmp")
            .then(|| self.tmp_store.clone());

        Ok(SessionHandle::new(
            session_id.to_string(),
            session_dir,
            session_stores,
            tmp_store,
        ))
    }

    /// Create a standalone ephemeral store not tracked by the session index.
    ///
    /// The returned [`TmpStore`] owns its own isolated [`Store`] pre-populated
    /// with `"doc"` and `"block"` collections.  All data is discarded when the
    /// `Arc` is dropped — nothing is written to disk.
    ///
    /// Use this when an agent needs a scratch pad for the duration of a task
    /// without caring about persistence or session identity.
    pub fn create_tmp_store(&self) -> Arc<stores::tmp::TmpStore> {
        Arc::new(stores::tmp::TmpStore::new())
    }

    /// List all known sessions.
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>, AppError> {
        let idx = self.read_index()?;
        Ok(idx.sessions.into_values().collect())
    }

    // ── Index helpers ─────────────────────────────────────────────────

    fn index_path(&self) -> PathBuf {
        self.memory_root.join("sessions.json")
    }

    fn read_index(&self) -> Result<SessionIndex, AppError> {
        let path = self.index_path();
        let data = fs::read_to_string(&path)
            .map_err(|e| AppError::Memory(format!("cannot read {}: {e}", path.display())))?;
        serde_json::from_str(&data)
            .map_err(|e| AppError::Memory(format!("malformed {}: {e}", path.display())))
    }

    fn update_index<F: FnOnce(&mut SessionIndex)>(&self, f: F) -> Result<(), AppError> {
        let path = self.index_path();
        let mut idx = self.read_index()?;
        f(&mut idx);
        let data = serde_json::to_string_pretty(&idx)
            .map_err(|e| AppError::Memory(format!("serialise index: {e}")))?;
        fs::write(&path, data)
            .map_err(|e| AppError::Memory(format!("cannot write {}: {e}", path.display())))
    }
}

impl std::fmt::Debug for MemorySystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemorySystem")
            .field("memory_root", &self.memory_root)
            .field("stores", &self.stores.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// ISO-8601 UTC timestamp without external crate.
fn now_iso8601() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let s = secs % 60;
    let total_min = secs / 60;
    let m = total_min % 60;
    let total_hr = total_min / 60;
    let h = total_hr % 24;
    let mut days = total_hr / 24;

    let mut yr = 1970u64;
    loop {
        let ydays = if yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0) { 366 } else { 365 };
        if days < ydays { break; }
        days -= ydays;
        yr += 1;
    }
    let leap = yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0);
    let mdays: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mon = 1u64;
    for &md in &mdays {
        if days < md { break; }
        days -= md;
        mon += 1;
    }
    let day = days + 1;
    format!("{yr:04}-{mon:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, MemorySystem) {
        let dir = TempDir::new().unwrap();
        let mem = MemorySystem::new(dir.path(), MemoryConfig::default()).unwrap();
        (dir, mem)
    }

    #[test]
    fn create_session_creates_dir_and_files() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["basic_session"], Some("chat")).unwrap();

        let session_dir = mem.sessions_dir.join(&handle.session_id);
        assert!(session_dir.exists());
        assert!(session_dir.join("kv.json").exists());
        assert!(session_dir.join("transcript.md").exists());
    }

    #[test]
    fn create_session_updates_index() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["basic_session"], Some("chat")).unwrap();

        let sessions = mem.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, handle.session_id);
        assert_eq!(sessions[0].last_agent, Some("chat".into()));
    }

    #[test]
    fn load_session_works() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["basic_session"], Some("chat")).unwrap();
        let sid = handle.session_id.clone();

        let loaded = mem.load_session(&sid, Some("observer")).unwrap();
        assert_eq!(loaded.session_id, sid);
    }

    #[test]
    fn load_nonexistent_session_errors() {
        let (_dir, mem) = setup();
        let result = mem.load_session("nonexistent", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn unknown_store_type_errors() {
        let (_dir, mem) = setup();
        let result = mem.create_session(&["nonexistent_store"], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown store type"));
    }

    #[test]
    fn multiple_sessions() {
        let (_dir, mem) = setup();
        mem.create_session(&["basic_session"], Some("a")).unwrap();
        mem.create_session(&["basic_session"], Some("b")).unwrap();
        mem.create_session(&["basic_session"], None).unwrap();

        let sessions = mem.list_sessions().unwrap();
        assert_eq!(sessions.len(), 3);
    }

    // ── Phase 3: SessionHandle tmp_doc / tmp_block ─────────────────────

    #[test]
    fn tmp_session_has_typed_collections() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["tmp"], Some("agent")).unwrap();

        // Both collections start empty.
        assert!(handle.tmp_doc().unwrap().is_empty());
        assert!(handle.tmp_block().unwrap().is_empty());
    }

    #[test]
    fn tmp_session_set_and_read_doc() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["tmp"], Some("agent")).unwrap();

        let mut doc = handle.tmp_doc().unwrap();
        doc.set("status".into(), PrimaryValue::from("active"));
        handle.set_tmp_doc(doc).unwrap();

        let doc2 = handle.tmp_doc().unwrap();
        assert_eq!(doc2.get("status"), Some(&PrimaryValue::from("active")));
    }

    #[test]
    fn basic_session_has_no_tmp_store() {
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["basic_session"], None).unwrap();
        assert!(handle.tmp_doc().is_err());
        assert!(handle.tmp_block().is_err());
    }

    #[test]
    fn tmp_session_survives_reload() {
        // Data written to a tmp session persists within the same MemorySystem
        // instance (same in-process TmpStore), even when the handle is re-created via load_session.
        let (_dir, mem) = setup();
        let handle = mem.create_session(&["tmp"], Some("agent")).unwrap();
        let sid = handle.session_id.clone();

        let mut doc = handle.tmp_doc().unwrap();
        doc.set("key".into(), PrimaryValue::Int(42));
        handle.set_tmp_doc(doc).unwrap();

        // Load the same session through a new handle.
        let handle2 = mem.load_session(&sid, None).unwrap();
        assert_eq!(
            handle2.tmp_doc().unwrap().get("key"),
            Some(&PrimaryValue::Int(42))
        );
    }

    #[test]
    fn create_tmp_store_is_independent() {
        let (_dir, mem) = setup();
        let ts = mem.create_tmp_store();
        // Standalone store has empty doc/block by default.
        assert!(ts.doc().unwrap().is_empty());
        let mut d = Doc::default();
        d.set("x".into(), PrimaryValue::Bool(true));
        ts.set_doc(d).unwrap();
        // A second call gives another independent store.
        let ts2 = mem.create_tmp_store();
        assert!(ts2.doc().unwrap().is_empty(), "new standalone store should be empty");
    }
}
