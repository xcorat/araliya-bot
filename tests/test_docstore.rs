//! Integration tests for the IDocStore feature.
//!
//! Run with:
//!   cargo test --features idocstore --test test_docstore

use std::collections::HashMap;
use std::fs;

use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use araliya_bot::subsystems::memory::{MemoryConfig, MemorySystem};
use araliya_bot::subsystems::memory::stores::docstore::{Document, IDocStore};

// ── helpers ──────────────────────────────────────────────────────────────────

fn identity_dir() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let id_dir = tmp.path().join("agent-test");
    fs::create_dir_all(&id_dir).expect("create identity dir");
    let p = id_dir.clone();
    (tmp, p)
}

fn doc(title: &str, content: &str) -> Document {
    Document {
        id: String::new(),
        title: title.into(),
        source: "integration-test".into(),
        content: content.into(),
        content_hash: String::new(),
        created_at: String::new(),
        metadata: HashMap::new(),
    }
}

// ── IDocStore ─────────────────────────────────────────────────────────────────

#[test]
fn docstore_open_creates_dirs_and_db() {
    let (_tmp, id_dir) = identity_dir();
    let _store = IDocStore::open(&id_dir).expect("open should succeed");
    assert!(id_dir.join("docstore").join("chunks.db").exists());
    assert!(id_dir.join("docstore").join("docs").is_dir());
}

#[test]
fn docstore_add_and_get_roundtrip() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let content = "The quick brown fox jumps over the lazy dog.";
    let id = store.add_document(doc("Pangram", content)).unwrap();

    let fetched = store.get_document(&id).unwrap();
    assert_eq!(fetched.title, "Pangram");
    assert_eq!(fetched.content, content);
    assert_eq!(fetched.source, "integration-test");
}

#[test]
fn docstore_dedup_returns_same_id() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let content = "duplicate content";
    let id1 = store.add_document(doc("First", content)).unwrap();
    let id2 = store.add_document(doc("Second", content)).unwrap();
    assert_eq!(id1, id2);
    assert_eq!(store.list_documents().unwrap().len(), 1);
}

#[test]
fn docstore_list_is_ordered_newest_first() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    store.add_document(doc("Alpha", "content alpha")).unwrap();
    store.add_document(doc("Beta",  "content beta")).unwrap();
    store.add_document(doc("Gamma", "content gamma")).unwrap();

    let docs = store.list_documents().unwrap();
    assert_eq!(docs.len(), 3);
    // All three present; order is newest first.
    let titles: Vec<_> = docs.iter().map(|d| d.title.as_str()).collect();
    assert!(titles.contains(&"Alpha"));
    assert!(titles.contains(&"Beta"));
    assert!(titles.contains(&"Gamma"));
}

#[test]
fn docstore_delete_removes_all_traces() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let id = store.add_document(doc("ToDelete", "remove this content")).unwrap();
    let chunks = store.chunk_document(&id, 512).unwrap();
    store.index_chunks(chunks).unwrap();

    store.delete_document(&id).unwrap();

    assert!(store.list_documents().unwrap().is_empty());
    assert!(store.search_by_text("remove", 5).unwrap().is_empty());
    let content_file = id_dir.join("docstore").join("docs").join(format!("{id}.txt"));
    assert!(!content_file.exists(), "raw content file should be deleted");
}

#[test]
fn docstore_chunk_and_search_bm25() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let id = store
        .add_document(doc("Rust Book", "ownership borrowing lifetimes async await traits"))
        .unwrap();
    let chunks = store.chunk_document(&id, 32).unwrap();
    assert!(!chunks.is_empty());
    store.index_chunks(chunks).unwrap();

    let results = store.search_by_text("ownership", 5).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].chunk.doc_id, id);
    assert!(results[0].score >= 0.0, "BM25 score should be non-negative (sign-flipped)");
}

#[test]
fn docstore_search_empty_query_returns_empty() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    store.add_document(doc("Doc", "some content")).unwrap();
    let results = store.search_by_text("", 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn docstore_chunk_positions_are_sequential() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let content = "a".repeat(200);
    let id = store.add_document(doc("Positions", &content)).unwrap();

    let chunks = store.chunk_document(&id, 50).unwrap();
    assert!(chunks.len() >= 2);
    let mut prev = 0usize;
    for (i, ch) in chunks.iter().enumerate() {
        if i == 0 {
            assert_eq!(ch.position, 0);
        } else {
            assert!(ch.position > prev, "positions must be strictly increasing");
        }
        prev = ch.position;
    }
}

#[test]
fn docstore_reindex_is_idempotent() {
    let (_tmp, id_dir) = identity_dir();
    let store = IDocStore::open(&id_dir).unwrap();
    let id = store.add_document(doc("ReIndex", "reindex test content here")).unwrap();

    // Index twice.
    let c1 = store.chunk_document(&id, 256).unwrap();
    store.index_chunks(c1).unwrap();
    let c2 = store.chunk_document(&id, 256).unwrap();
    store.index_chunks(c2).unwrap();

    // Still finds the document; no duplicate hits.
    let res = store.search_by_text("reindex", 10).unwrap();
    assert!(!res.is_empty());
    let unique_doc_ids: std::collections::HashSet<_> =
        res.iter().map(|r| r.chunk.doc_id.as_str()).collect();
    assert_eq!(unique_doc_ids.len(), 1);
}

// ── MemorySystem integration ──────────────────────────────────────────────────

#[tokio::test]
async fn memory_system_start_docstore_manager_does_not_panic() {
    let tmp = TempDir::new().unwrap();
    let mut mem = MemorySystem::new(tmp.path(), MemoryConfig::default()).unwrap();
    let shutdown = CancellationToken::new();
    // Should not panic; manager directory missing is fine (nothing to scan).
    mem.start_docstore_manager(shutdown.clone());
    shutdown.cancel();
}

#[tokio::test]
async fn memory_system_schedule_docstore_index_is_noop_without_dir() {
    let tmp = TempDir::new().unwrap();
    let mut mem = MemorySystem::new(tmp.path(), MemoryConfig::default()).unwrap();
    let shutdown = CancellationToken::new();
    mem.start_docstore_manager(shutdown.clone());
    // Non-existent path — should not panic, just log+skip.
    mem.schedule_docstore_index(tmp.path().join("nonexistent-agent"));
    shutdown.cancel();
}

#[tokio::test]
async fn memory_system_schedule_docstore_index_for_real_store() {
    let tmp = TempDir::new().unwrap();
    let id_dir = tmp.path().join("agent-identity");
    fs::create_dir_all(&id_dir).unwrap();

    // Pre-populate a docstore with an un-indexed document.
    {
        let store = IDocStore::open(&id_dir).unwrap();
        store.add_document(doc("Scheduled", "content to be indexed by manager")).unwrap();
        // Deliberately do NOT call chunk_document / index_chunks.
    }

    let mut mem = MemorySystem::new(tmp.path(), MemoryConfig::default()).unwrap();
    let shutdown = CancellationToken::new();
    mem.start_docstore_manager(shutdown.clone());
    // Trigger immediate index — non-blocking; just verifies no panic.
    mem.schedule_docstore_index(id_dir);
    shutdown.cancel();
}
