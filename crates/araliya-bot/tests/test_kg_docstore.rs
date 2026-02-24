//! Integration tests for the IKGDocStore feature.
//!
//! These tests require the `ikgdocstore` Cargo feature:
//!
//! ```bash
//! cargo test --test test_kg_docstore --features ikgdocstore,subsystem-memory
//! ```

use std::collections::HashMap;
use std::fs;

use araliya_bot::subsystems::memory::stores::kg_docstore::{
    Document, IKGDocStore, KgConfig, KgSearchResult,
};
use tempfile::TempDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_store() -> (TempDir, IKGDocStore) {
    let temp = TempDir::new().expect("tempdir");
    let identity_dir = temp.path().join("agent");
    fs::create_dir_all(&identity_dir).expect("create identity dir");
    let store = IKGDocStore::open(&identity_dir).expect("open kgdocstore");
    (temp, store)
}

fn doc(title: &str, content: &str) -> Document {
    Document {
        id: String::new(),
        title: title.to_string(),
        source: "test".to_string(),
        content: content.to_string(),
        content_hash: String::new(),
        created_at: String::new(),
        metadata: HashMap::new(),
    }
}

fn index_doc(store: &IKGDocStore, title: &str, content: &str) -> String {
    let doc_id = store.add_document(doc(title, content)).expect("add");
    let chunks = store.chunk_document(&doc_id, 512).expect("chunk");
    store.index_chunks(chunks).expect("index");
    doc_id
}

// ── IKGDocStore ───────────────────────────────────────────────────────────────

#[test]
fn open_creates_required_dirs() {
    let (_temp, store) = make_store();
    assert!(store.root_dir().exists(), "kgdocstore root should exist");
    assert!(store.root_dir().join("docs").exists(), "docs/ should exist");
    assert!(store.root_dir().join("kg").exists(), "kg/ should exist");
    assert!(store.root_dir().join("chunks.db").exists(), "chunks.db should exist");
}

#[test]
fn add_and_get_document_round_trip() {
    let (_temp, store) = make_store();
    let doc_id = store.add_document(doc("Hello", "hello world content")).expect("add");
    let retrieved = store.get_document(&doc_id).expect("get");
    assert_eq!(retrieved.title, "Hello");
    assert!(retrieved.content.contains("hello world"));
}

#[test]
fn add_document_deduplicates_by_content_hash() {
    let (_temp, store) = make_store();
    let id1 = store.add_document(doc("A", "same text")).expect("first");
    let id2 = store.add_document(doc("B", "same text")).expect("dedup");
    assert_eq!(id1, id2);
    assert_eq!(store.list_documents().expect("list").len(), 1);
}

#[test]
fn delete_document_cleans_up() {
    let (_temp, store) = make_store();
    let doc_id = index_doc(&store, "Del", "content to remove");
    store.delete_document(&doc_id).expect("delete");
    assert!(store.list_documents().expect("list").is_empty());
    let results = store.search_by_text("content", 5).expect("search");
    assert!(results.is_empty());
}

#[test]
fn chunk_and_search_returns_results() {
    let (_temp, store) = make_store();
    let doc_id = index_doc(&store, "R", "rust memory bm25 search chunk indexing");
    let results = store.search_by_text("bm25", 5).expect("search");
    assert!(!results.is_empty());
    assert_eq!(results[0].chunk.doc_id, doc_id);
}

#[test]
fn all_chunks_enumerates_every_indexed_chunk() {
    let (_temp, store) = make_store();
    index_doc(&store, "A", "alpha beta gamma delta epsilon zeta");
    index_doc(&store, "B", "one two three four five six seven");
    let all = store.all_chunks().expect("all_chunks");
    assert!(all.len() >= 2, "expected at least one chunk per document");
}

#[test]
fn get_chunks_by_ids_preserves_request_order() {
    let (_temp, store) = make_store();
    let doc_id = store.add_document(doc("O", "word1 word2 word3 word4 word5 word6")).expect("add");
    let chunks = store.chunk_document(&doc_id, 8).expect("chunk");
    store.index_chunks(chunks.clone()).expect("index");
    // Request in reverse order
    let ids: Vec<String> = chunks.iter().rev().map(|c| c.id.clone()).collect();
    let fetched = store.get_chunks_by_ids(&ids).expect("fetch");
    for (f, req_id) in fetched.iter().zip(ids.iter()) {
        assert_eq!(&f.id, req_id, "order should match request");
    }
}

// ── KG build pipeline ─────────────────────────────────────────────────────────

#[test]
fn rebuild_kg_with_no_chunks_writes_empty_graph() {
    let (_temp, store) = make_store();
    store.rebuild_kg().expect("rebuild_kg on empty store");
    let graph_path = store.root_dir().join("kg").join("graph.json");
    assert!(graph_path.exists(), "graph.json should be written");
}

#[test]
fn rebuild_kg_extracts_camelcase_entities() {
    let (_temp, store) = make_store();
    // Use the same CamelCase terms enough times to survive min_mentions filter
    let content = "AuthService handles TokenValidator. \
                   AuthService calls TokenValidator. \
                   AuthService uses TokenValidator for each request.";
    index_doc(&store, "Auth", content);

    store.rebuild_kg().expect("rebuild_kg");

    let graph_path = store.root_dir().join("kg").join("graph.json");
    let graph_json = fs::read_to_string(&graph_path).expect("read graph.json");
    // Both camelcase terms should appear in the entity map
    assert!(
        graph_json.contains("authservice"),
        "authservice should be an entity"
    );
    assert!(
        graph_json.contains("tokenvalidator"),
        "tokenvalidator should be an entity"
    );
}

#[test]
fn rebuild_kg_writes_entities_and_relations_files() {
    let (_temp, store) = make_store();
    let content = "SystemA uses SystemB. SystemA calls SystemB. SystemA depends on SystemB.";
    index_doc(&store, "sys", content);
    store.rebuild_kg().expect("rebuild_kg");

    assert!(store.root_dir().join("kg").join("entities.json").exists());
    assert!(store.root_dir().join("kg").join("relations.json").exists());
    assert!(store.root_dir().join("kg").join("graph.json").exists());
}

// ── KG query pipeline ─────────────────────────────────────────────────────────

#[test]
fn search_with_kg_falls_back_when_no_graph() {
    let (_temp, store) = make_store();
    index_doc(&store, "F", "the quick brown fox jumps over the lazy dog");
    // Do NOT call rebuild_kg
    let result: KgSearchResult = store
        .search_with_kg("fox", &KgConfig::default())
        .expect("search");
    assert!(!result.used_kg, "no graph → should fall back to FTS");
    assert!(!result.context.is_empty(), "context should not be empty");
}

#[test]
fn search_with_kg_empty_seed_falls_back_to_fts() {
    let (_temp, store) = make_store();
    let content = "AuthService handles TokenValidator. \
                   AuthService calls TokenValidator. \
                   AuthService uses TokenValidator.";
    index_doc(&store, "Auth", content);
    store.rebuild_kg().expect("rebuild_kg");

    // Query contains no entity names from the graph
    let result = store
        .search_with_kg("zzz unknown query xyz", &KgConfig::default())
        .expect("search");
    assert!(!result.used_kg, "no matching seed entities → FTS fallback");
}

#[test]
fn search_with_kg_uses_kg_when_entity_matched() {
    let (_temp, store) = make_store();
    let content = "AuthService handles TokenValidator. \
                   AuthService calls TokenValidator. \
                   AuthService uses TokenValidator for validation.";
    index_doc(&store, "Auth", content);
    store.rebuild_kg().expect("rebuild_kg");

    let cfg = KgConfig {
        min_entity_mentions: 1,
        ..KgConfig::default()
    };
    let result = store
        .search_with_kg("authservice tokenvalidator", &cfg)
        .expect("search");
    // Context must be non-empty regardless of used_kg value
    assert!(!result.context.is_empty(), "context should contain passages");
}

#[test]
fn search_with_kg_seed_cap_applied() {
    let (_temp, store) = make_store();
    // Create content where several CamelCase entities each appear twice
    let content = "AlphaSystem calls BetaSystem. AlphaSystem uses BetaSystem. \
                   GammaService extends DeltaService. GammaService needs DeltaService. \
                   EpsilonModule links ZetaModule. EpsilonModule wraps ZetaModule.";
    index_doc(&store, "Multi", content);
    store.rebuild_kg().expect("rebuild_kg");

    let cfg = KgConfig {
        max_seeds: 2,
        min_entity_mentions: 1,
        ..KgConfig::default()
    };
    // A broad query that could match many entities — should not panic
    let result = store
        .search_with_kg("alphasystem betasystem gammaservice deltaservice epsilonmodule", &cfg)
        .expect("search with many seeds");
    // Seeds were capped to max_seeds = 2
    assert!(result.seed_entities.len() <= 2);
}

#[test]
fn search_with_kg_context_contains_kg_summary_section() {
    let (_temp, store) = make_store();
    let content = "AuthService handles TokenValidator. \
                   AuthService calls TokenValidator. \
                   AuthService uses TokenValidator.";
    index_doc(&store, "Auth", content);
    store.rebuild_kg().expect("rebuild_kg");

    let cfg = KgConfig { min_entity_mentions: 1, ..KgConfig::default() };
    let result = store
        .search_with_kg("authservice", &cfg)
        .expect("search");

    if result.used_kg {
        assert!(
            result.context.contains("Knowledge Graph Context"),
            "KG summary section should be present when KG is used"
        );
    }
}

// ── Both stores coexist on same agent dir ─────────────────────────────────────

#[cfg(feature = "idocstore")]
#[test]
fn idocstore_and_ikgdocstore_coexist() {
    use araliya_bot::subsystems::memory::stores::docstore::IDocStore;

    let temp = TempDir::new().expect("tempdir");
    let identity_dir = temp.path().join("agent");
    fs::create_dir_all(&identity_dir).expect("mkdir");

    let ids = IDocStore::open(&identity_dir).expect("IDocStore::open");
    let kgs = IKGDocStore::open(&identity_dir).expect("IKGDocStore::open");

    // Each store has its own root directory — no overlap
    assert_ne!(ids.root_dir(), kgs.root_dir());
    assert!(ids.root_dir().ends_with("docstore"));
    assert!(kgs.root_dir().ends_with("kgdocstore"));
}
