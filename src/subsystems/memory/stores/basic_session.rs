#![cfg_attr(test, allow(private_interfaces))]
//! `basic_session` store — capped key-value (Doc-backed) + capped Markdown transcript.
//!
//! ## On-disk format
//!
//! Two files per session directory:
//!
//! - `kv.json` — `{ "cap": N, "order": ["k1","k2",...], "values": {"k1":"v1",...} }`
//!   The flat `values` map is the canonical serialisation of a [`Doc`] collection.
//!   `order` preserves insertion order for FIFO eviction.
//!
//! - `transcript.md` — Markdown with `### {role} — {timestamp}` delimiters.
//!   Human-readable; also exposed as a [`Block`] collection via
//!   [`read_transcript_block`](BasicSessionStore::read_transcript_block).
//!   Each entry becomes a `Value::Text(TextFile)` keyed by zero-padded index.
//!
//! Both stores are capped by entry count (FIFO — oldest entries dropped first).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::AppError;
use super::super::collections::{Block, Doc};
use super::super::store::{SessionStore, TranscriptEntry};
use super::super::types::{PrimaryValue, TextFile, Value};

//  TODO: these  should be const  definied top or config.
/// Default maximum number of k-v entries before FIFO eviction.
const DEFAULT_KV_CAP: usize = 200;
/// Default maximum number of transcript entries before FIFO eviction.
const DEFAULT_TRANSCRIPT_CAP: usize = 500;

const KV_FILENAME: &str = "kv.json";
const TRANSCRIPT_FILENAME: &str = "transcript.md";

// ── On-disk structures ────────────────────────────────────────────────────────

/// On-disk shape of `kv.json`.
///
/// `order` records insertion order so FIFO eviction is deterministic.
/// `values` is a plain string map — structurally identical to a serialised [`Doc`].
#[derive(serde::Serialize, serde::Deserialize)]
struct KvFile {
    cap: usize,
    /// Insertion-ordered list of active keys.
    order: Vec<String>,
    /// Flat string map — direct serialisation of a [`Doc`].
    values: HashMap<String, String>,
}

impl KvFile {
    fn empty(cap: usize) -> Self {
        Self { cap, order: Vec::new(), values: HashMap::new() }
    }

    /// Build a [`Doc`] from the current k-v values.
    fn to_doc(&self) -> Doc {
        let mut doc = Doc::default();
        for (k, v) in &self.values {
            doc.set(k.clone(), PrimaryValue::Str(v.clone()));
        }
        doc
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    /// Upsert, maintaining insertion order and FIFO cap.
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
        if removed { self.order.retain(|k| k != key); }
        removed
    }
}

// ── BasicSessionStore ─────────────────────────────────────────────────────────

pub struct BasicSessionStore {
    kv_cap: usize,
    transcript_cap: usize,
}

impl BasicSessionStore {
    pub fn new(kv_cap: Option<usize>, transcript_cap: Option<usize>) -> Self {
        Self {
            kv_cap: kv_cap.unwrap_or(DEFAULT_KV_CAP),
            transcript_cap: transcript_cap.unwrap_or(DEFAULT_TRANSCRIPT_CAP),
        }
    }

    // ── K-V helpers ───────────────────────────────────────────────────

    fn kv_path(session_dir: &Path) -> std::path::PathBuf {
        session_dir.join(KV_FILENAME)
    }

    pub(crate) fn read_kv(session_dir: &Path) -> Result<KvFile, AppError> {
        let path = Self::kv_path(session_dir);
        let data = fs::read_to_string(&path)
            .map_err(|e| AppError::Memory(format!("cannot read {}: {e}", path.display())))?;
        serde_json::from_str(&data)
            .map_err(|e| AppError::Memory(format!("malformed {}: {e}", path.display())))
    }

    fn write_kv(session_dir: &Path, kv: &KvFile) -> Result<(), AppError> {
        let path = Self::kv_path(session_dir);
        let data = serde_json::to_string_pretty(kv)
            .map_err(|e| AppError::Memory(format!("serialise kv: {e}")))?;
        fs::write(&path, data)
            .map_err(|e| AppError::Memory(format!("cannot write {}: {e}", path.display())))
    }

    pub(crate) fn now_iso8601() -> String {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = d.as_secs();
        let (s, m, h, day, mon, yr) = secs_to_utc(secs);
        format!("{yr:04}-{mon:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
    }

    // ── Transcript helpers ────────────────────────────────────────────

    fn transcript_path(session_dir: &Path) -> std::path::PathBuf {
        session_dir.join(TRANSCRIPT_FILENAME)
    }

    /// Parse `transcript.md` into typed entries.
    fn parse_transcript(text: &str) -> Vec<TranscriptEntry> {
        let mut entries = Vec::new();
        let mut current: Option<(String, String, Vec<String>)> = None;

        for line in text.lines() {
            if let Some(header) = line.strip_prefix("### ") {
                if let Some((role, ts, lines)) = current.take() {
                    entries.push(TranscriptEntry {
                        role,
                        timestamp: ts,
                        content: lines.join("\n").trim().to_string(),
                    });
                }
                let (role, ts) = if let Some((r, t)) = header.split_once(" — ") {
                    (r.trim().to_string(), t.trim().to_string())
                } else {
                    (header.to_string(), String::new())
                };
                current = Some((role, ts, Vec::new()));
            } else if let Some((_, _, ref mut lines)) = current {
                lines.push(line.to_string());
            }
        }
        if let Some((role, ts, lines)) = current {
            entries.push(TranscriptEntry {
                role,
                timestamp: ts,
                content: lines.join("\n").trim().to_string(),
            });
        }
        entries
    }

    fn serialise_transcript(entries: &[TranscriptEntry]) -> String {
        let mut out = String::new();
        for e in entries {
            out.push_str(&format!("### {} — {}\n\n{}\n\n", e.role, e.timestamp, e.content));
        }
        out
    }

    // ── Typed Collection views ────────────────────────────────────────

    /// Return the current k-v store as a [`Doc`] collection.
    ///
    /// All values are `PrimaryValue::Str`.  Changes to the returned [`Doc`] are
    /// not automatically persisted — call [`kv_set`](SessionStore::kv_set) as
    /// normal to write mutations back.
    pub fn read_kv_doc(&self, session_dir: &Path) -> Result<Doc, AppError> {
        Ok(Self::read_kv(session_dir)?.to_doc())
    }

    /// Return all transcript entries as a [`Block`] collection.
    ///
    /// Each entry is keyed by its zero-padded position (`"000000"`, `"000001"`,…).
    /// The value is a [`Value::Text`] where:
    /// - `content` = the entry text,
    /// - `metadata` = `{ "role": ..., "ts": ..., "mime": "text/plain" }`.
    pub fn read_transcript_block(&self, session_dir: &Path) -> Result<Block, AppError> {
        let path = Self::transcript_path(session_dir);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let entries = Self::parse_transcript(&text);

        let mut block = Block::default();
        for (i, entry) in entries.iter().enumerate() {
            let key = format!("{i:06}");
            let mut tf = TextFile::new(entry.content.clone());
            tf.metadata.insert("role".to_string(), entry.role.clone());
            tf.metadata.insert("ts".to_string(), entry.timestamp.clone());
            tf.metadata.insert("mime".to_string(), "text/plain".to_string());
            block.set(key, Value::Text(tf));
        }
        Ok(block)
    }
}

impl SessionStore for BasicSessionStore {
    fn store_type(&self) -> &str {
        "basic_session"
    }

    fn init(&self, session_dir: &Path) -> Result<(), AppError> {
        Self::write_kv(session_dir, &KvFile::empty(self.kv_cap))?;
        let path = Self::transcript_path(session_dir);
        fs::write(&path, "")
            .map_err(|e| AppError::Memory(format!("cannot create {}: {e}", path.display())))?;
        Ok(())
    }

    // ── K-V ───────────────────────────────────────────────────────────

    fn kv_get(&self, session_dir: &Path, key: &str) -> Result<Option<String>, AppError> {
        Ok(Self::read_kv(session_dir)?.get(key).map(|s| s.to_string()))
    }

    fn kv_set(&self, session_dir: &Path, key: &str, value: &str) -> Result<(), AppError> {
        let mut kv = Self::read_kv(session_dir)?;
        kv.set(key, value);
        Self::write_kv(session_dir, &kv)
    }

    fn kv_delete(&self, session_dir: &Path, key: &str) -> Result<bool, AppError> {
        let mut kv = Self::read_kv(session_dir)?;
        let removed = kv.delete(key);
        Self::write_kv(session_dir, &kv)?;
        Ok(removed)
    }

    // ── Transcript ────────────────────────────────────────────────────

    fn transcript_append(
        &self,
        session_dir: &Path,
        role: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let path = Self::transcript_path(session_dir);
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let mut entries = Self::parse_transcript(&existing);

        entries.push(TranscriptEntry {
            role: role.to_string(),
            timestamp: Self::now_iso8601(),
            content: content.to_string(),
        });

        while entries.len() > self.transcript_cap {
            entries.remove(0);
        }

        let out = Self::serialise_transcript(&entries);
        let mut f = fs::File::create(&path)
            .map_err(|e| AppError::Memory(format!("cannot write {}: {e}", path.display())))?;
        f.write_all(out.as_bytes())
            .map_err(|e| AppError::Memory(format!("write {}: {e}", path.display())))?;
        Ok(())
    }

    fn transcript_read_last(
        &self,
        session_dir: &Path,
        n: usize,
    ) -> Result<Vec<TranscriptEntry>, AppError> {
        let path = Self::transcript_path(session_dir);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let entries = Self::parse_transcript(&text);
        let start = entries.len().saturating_sub(n);
        Ok(entries[start..].to_vec())
    }

    fn read_kv_doc(
        &self,
        session_dir: &Path,
    ) -> Result<super::super::collections::Doc, AppError> {
        self.read_kv_doc(session_dir)
    }

    fn read_transcript_block(
        &self,
        session_dir: &Path,
    ) -> Result<super::super::collections::Block, AppError> {
        self.read_transcript_block(session_dir)
    }
}

// ── Minimal UTC formatter (avoids chrono/time dependency) ─────────────────────

fn secs_to_utc(epoch_secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = epoch_secs % 60;
    let total_min = epoch_secs / 60;
    let m = total_min % 60;
    let total_hr = total_min / 60;
    let h = total_hr % 24;
    let mut days = total_hr / 24;

    let mut yr = 1970u64;
    loop {
        let ydays = if is_leap(yr) { 366 } else { 365 };
        if days < ydays { break; }
        days -= ydays;
        yr += 1;
    }
    let leap = is_leap(yr);
    let mdays: [u64; 12] = [
        31, if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut mon = 1u64;
    for &md in &mdays {
        if days < md { break; }
        days -= md;
        mon += 1;
    }
    let day = days + 1;
    (s, m, h, day, mon, yr)
}

fn is_leap(y: u64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, BasicSessionStore) {
        let dir = TempDir::new().unwrap();
        let store = BasicSessionStore::new(Some(5), Some(3));
        store.init(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn kv_set_get_delete() {
        let (dir, store) = setup();

        assert_eq!(store.kv_get(dir.path(), "foo").unwrap(), None);

        store.kv_set(dir.path(), "foo", "bar").unwrap();
        assert_eq!(store.kv_get(dir.path(), "foo").unwrap(), Some("bar".into()));

        store.kv_set(dir.path(), "foo", "baz").unwrap();
        assert_eq!(store.kv_get(dir.path(), "foo").unwrap(), Some("baz".into()));

        assert!(store.kv_delete(dir.path(), "foo").unwrap());
        assert_eq!(store.kv_get(dir.path(), "foo").unwrap(), None);
        assert!(!store.kv_delete(dir.path(), "foo").unwrap());
    }

    #[test]
    fn kv_fifo_cap() {
        let (dir, store) = setup(); // cap = 5

        for i in 0..8 {
            store.kv_set(dir.path(), &format!("k{i}"), &format!("v{i}")).unwrap();
        }

        let kv = BasicSessionStore::read_kv(dir.path()).unwrap();
        assert_eq!(kv.order.len(), 5);
        assert!(store.kv_get(dir.path(), "k0").unwrap().is_none());
        assert!(store.kv_get(dir.path(), "k2").unwrap().is_none());
        assert_eq!(store.kv_get(dir.path(), "k3").unwrap(), Some("v3".into()));
        assert_eq!(store.kv_get(dir.path(), "k7").unwrap(), Some("v7".into()));
    }

    #[test]
    fn kv_as_doc() {
        let (dir, store) = setup();
        store.kv_set(dir.path(), "agent", "chat").unwrap();
        store.kv_set(dir.path(), "model", "gpt-4o").unwrap();

        let doc = store.read_kv_doc(dir.path()).unwrap();
        assert_eq!(doc.get("agent"), Some(&PrimaryValue::Str("chat".into())));
        assert_eq!(doc.get("model"), Some(&PrimaryValue::Str("gpt-4o".into())));
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn transcript_as_block() {
        let (dir, store) = setup();
        store.transcript_append(dir.path(), "user", "hello").unwrap();
        store.transcript_append(dir.path(), "assistant", "hi").unwrap();

        let block = store.read_transcript_block(dir.path()).unwrap();
        assert_eq!(block.len(), 2);

        let v0 = block.get("000000").unwrap();
        if let Value::Text(tf) = v0 {
            assert_eq!(tf.content, "hello");
            assert_eq!(tf.metadata.get("role").unwrap(), "user");
            assert_eq!(tf.metadata.get("mime").unwrap(), "text/plain");
        } else {
            panic!("expected Text, got {:?}", v0);
        }

        let v1 = block.get("000001").unwrap();
        if let Value::Text(tf) = v1 {
            assert_eq!(tf.content, "hi");
            assert_eq!(tf.metadata.get("role").unwrap(), "assistant");
        } else {
            panic!("expected Text, got {:?}", v1);
        }
    }

    #[test]
    fn transcript_append_and_read() {
        let (dir, store) = setup();

        store.transcript_append(dir.path(), "user", "hello").unwrap();
        store.transcript_append(dir.path(), "assistant", "hi there").unwrap();

        let entries = store.transcript_read_last(dir.path(), 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[0].content, "hello");
        assert_eq!(entries[1].role, "assistant");
        assert_eq!(entries[1].content, "hi there");
    }

    #[test]
    fn transcript_fifo_cap() {
        let (dir, store) = setup(); // cap = 3

        for i in 0..5 {
            store.transcript_append(dir.path(), "user", &format!("msg{i}")).unwrap();
        }

        let entries = store.transcript_read_last(dir.path(), 10).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].content, "msg2");
        assert_eq!(entries[2].content, "msg4");
    }

    #[test]
    fn transcript_read_last_n() {
        let (dir, store) = setup();

        store.transcript_append(dir.path(), "user", "a").unwrap();
        store.transcript_append(dir.path(), "assistant", "b").unwrap();
        store.transcript_append(dir.path(), "user", "c").unwrap();

        let entries = store.transcript_read_last(dir.path(), 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "b");
        assert_eq!(entries[1].content, "c");
    }

    #[test]
    fn iso8601_format() {
        let ts = BasicSessionStore::now_iso8601();
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }
}
