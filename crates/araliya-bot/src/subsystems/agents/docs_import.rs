//! `docs_import` — populate an agent's [IDocStore] from a configured docs directory tree.
//!
//! Called once during agent subsystem startup.  If the target docstore already
//! contains documents, the import is skipped so existing data is never overwritten.
//!
//! Allowed file extensions: `.md`, `.txt`.  
//! Skipped sub-directories: `images`.  
//! Files larger than [`MAX_FILE_BYTES`] are ignored with a warning.

use std::path::Path;
use std::{fs, io};

use crate::error::AppError;
use crate::subsystems::memory::stores::docstore::{Document, IDocStore};

/// Maximum size of a single source file that will be imported.
pub const MAX_FILE_BYTES: u64 = 2_000_000; // 2 MB

/// Chunk size (in bytes) used when indexing imported documents for BM25 search.
const CHUNK_SIZE: usize = 16384; // 16 KB

/// Extensions that are considered text content and will be imported.
const ALLOWED_EXTENSIONS: &[&str] = &["md", "txt"];

/// Directory names that are never descended into.
const SKIP_DIRS: &[&str] = &["images"];

/// Placeholder content written to `index_name` when it is missing from `source_dir`.
const INDEX_PLACEHOLDER: &str = "_TODO_";

/// Populate `agent_identity_dir`'s docstore from `source_dir`.
///
/// # Behaviour
/// 1. Opens (or creates) the [IDocStore] at `{agent_identity_dir}/docstore/`.
/// 2. If the docstore already contains at least one document, returns immediately.
/// 3. Recursively walks `source_dir`.  For each `.md` / `.txt` file:
///    - Skips files larger than [`MAX_FILE_BYTES`].
///    - Uses the relative path from `source_dir` as the document ID.
///    - Adds the document to the store, then chunks and indexes it.
/// 4. If no file matching `index_name` was found (or added), creates a placeholder
///    document with content `"_TODO_"`.
///
/// All per-file errors are logged as warnings; only fatal errors (cannot open
/// the docstore, cannot read the source directory) are propagated.
pub fn populate_docstore_from_source(
    agent_identity_dir: &Path,
    source_dir: &Path,
    index_name: &str,
) -> Result<(), AppError> {
    let docstore = IDocStore::open(agent_identity_dir)?;

    // Idempotency guard: skip if already populated AND the index document's
    // content file is actually readable.  If the DB has rows but the content
    // file is missing (e.g. stale data from a previous partial/broken import),
    // treat the store as corrupt, wipe all rows, and re-import from scratch.
    let existing = docstore.list_documents()?;
    if !existing.is_empty() {
        let index_healthy = docstore.get_document(index_name).is_ok();
        if index_healthy {
            tracing::debug!(
                "docstore already populated ({} docs) at {:?}, skipping import",
                existing.len(),
                agent_identity_dir
            );
            return Ok(());
        }

        tracing::warn!(
            "docstore has {} metadata row(s) but index '{}' content is missing — \
             stale data detected; clearing for fresh import",
            existing.len(),
            index_name
        );
        for meta in &existing {
            if let Err(e) = docstore.delete_document(&meta.doc_id) {
                tracing::warn!(
                    "docs_import: failed to remove stale doc '{}': {}",
                    meta.doc_id,
                    e
                );
            }
        }
    }

    tracing::info!(
        "importing docs from {:?} into agent docstore at {:?}",
        source_dir,
        agent_identity_dir
    );

    // Collect (relative_path_string, content) pairs.
    let mut entries: Vec<(String, String)> = Vec::new();
    collect_text_files(source_dir, source_dir, &mut entries)?;

    // Ensure an index document is present.
    let index_present = entries.iter().any(|(rel, _)| rel == index_name);
    if !index_present {
        tracing::debug!(
            "index document '{}' not found in {:?}; creating placeholder",
            index_name,
            source_dir
        );
        entries.push((index_name.to_string(), INDEX_PLACEHOLDER.to_string()));
    }

    let mut added = 0usize;
    for (rel_path, content) in entries {
        let doc = Document {
            id: rel_path.clone(),
            title: rel_path.clone(),
            source: source_dir.to_string_lossy().to_string(),
            content,
            content_hash: String::new(),
            created_at: String::new(),
            metadata: Default::default(),
        };

        let doc_id = match docstore.add_document(doc) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("docs_import: skipping '{}': add_document failed: {}", rel_path, e);
                continue;
            }
        };

        match docstore.chunk_document(&doc_id, CHUNK_SIZE) {
            Ok(chunks) => {
                if let Err(e) = docstore.index_chunks(chunks) {
                    tracing::warn!(
                        "docs_import: failed to index chunks for '{}': {}",
                        rel_path,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!("docs_import: failed to chunk '{}': {}", rel_path, e);
            }
        }

        added += 1;
    }

    tracing::info!(
        "docs import complete: {} document(s) added for agent at {:?}",
        added,
        agent_identity_dir
    );
    Ok(())
}

/// Recursively walk `current_dir` (relative to `source_root`) and push
/// `(relative_path, content)` pairs into `out` for every allowed text file.
fn collect_text_files(
    source_root: &Path,
    current_dir: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), AppError> {
    let read = fs::read_dir(current_dir).map_err(|e| {
        AppError::Memory(format!(
            "docs_import: read_dir {:?}: {}",
            current_dir, e
        ))
    })?;

    for entry in read {
        let entry = entry
            .map_err(|e| AppError::Memory(format!("docs_import: dir entry error: {}", e)))?;
        let path = entry.path();

        if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if SKIP_DIRS.contains(&dir_name) {
                tracing::debug!("docs_import: skipping directory '{}'", dir_name);
                continue;
            }
            collect_text_files(source_root, &path, out)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        // Size guard.
        match fs::metadata(&path) {
            Ok(m) if m.len() > MAX_FILE_BYTES => {
                tracing::warn!(
                    "docs_import: skipping {:?}: {} bytes exceeds limit of {}",
                    path,
                    m.len(),
                    MAX_FILE_BYTES
                );
                continue;
            }
            Err(e) => {
                tracing::warn!("docs_import: cannot stat {:?}: {}", path, e);
                continue;
            }
            _ => {}
        }

        // Read as UTF-8; skip on error rather than aborting the whole import.
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                tracing::warn!("docs_import: skipping {:?}: not valid UTF-8", path);
                continue;
            }
            Err(e) => {
                tracing::warn!("docs_import: skipping {:?}: read error: {}", path, e);
                continue;
            }
        };

        // Relative path from source root becomes the document ID.
        let rel = path
            .strip_prefix(source_root)
            .map_err(|_| {
                AppError::Memory(format!(
                    "docs_import: strip_prefix failed for {:?}",
                    path
                ))
            })?
            .to_string_lossy()
            .to_string();

        out.push((rel, content));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::memory::AGENTS_DIRNAME;
    use std::fs;
    use tempfile::TempDir;

    fn make_identity_dir() -> TempDir {
        let tmp = TempDir::new().expect("tempdir");
        fs::create_dir_all(tmp.path().join(AGENTS_DIRNAME)).expect("create identity dir");
        tmp
    }

    fn make_source_dir() -> TempDir {
        let tmp = TempDir::new().expect("source tempdir");
        let src = tmp.path();

        fs::write(src.join("index.md"), "# Docs index").expect("index.md");
        fs::write(src.join("guide.md"), "## Guide content").expect("guide.md");
        fs::write(src.join("notes.txt"), "plain text note").expect("notes.txt");

        // Sub-directory that should be walked.
        let sub = src.join("architecture");
        fs::create_dir_all(&sub).expect("architecture dir");
        fs::write(sub.join("overview.md"), "arch overview").expect("overview.md");

        // Images directory that must be skipped.
        let img_dir = src.join("images");
        fs::create_dir_all(&img_dir).expect("images dir");
        fs::write(img_dir.join("logo.png"), &[0u8; 4]).expect("logo.png");
        fs::write(img_dir.join("caption.md"), "this should be skipped").expect("caption.md in images");

        tmp
    }

    #[test]
    fn import_copies_text_files_and_indexes() {
        let identity_tmp = make_identity_dir();
        let identity_dir = identity_tmp.path().join(AGENTS_DIRNAME);

        let source_tmp = make_source_dir();
        let source_dir = source_tmp.path();

        populate_docstore_from_source(&identity_dir, source_dir, "index.md")
            .expect("import should succeed");

        let store = IDocStore::open(&identity_dir).expect("open after import");
        let docs = store.list_documents().expect("list");

        // index.md, guide.md, notes.txt, architecture/overview.md = 4 docs.
        assert_eq!(docs.len(), 4, "expected 4 imported documents, got: {:?}", docs.iter().map(|d| &d.doc_id).collect::<Vec<_>>());

        // images/caption.md must NOT be in the store.
        assert!(
            !docs.iter().any(|d| d.doc_id.contains("images")),
            "images/ content must not be imported"
        );

        // All should have chunks indexed.
        let search = store.search_by_text("guide", 5).expect("search");
        assert!(!search.is_empty(), "BM25 search should return results after import");
    }

    #[test]
    fn import_creates_placeholder_index_when_missing() {
        let identity_tmp = make_identity_dir();
        let identity_dir = identity_tmp.path().join(AGENTS_DIRNAME);

        let source_tmp = TempDir::new().expect("empty source");
        fs::write(source_tmp.path().join("readme.md"), "some content").expect("write");

        populate_docstore_from_source(&identity_dir, source_tmp.path(), "index.md")
            .expect("import should succeed");

        let store = IDocStore::open(&identity_dir).expect("open");
        let docs = store.list_documents().expect("list");

        let index_doc = docs.iter().find(|d| d.doc_id == "index.md");
        assert!(index_doc.is_some(), "index.md placeholder should have been created");
        let doc = store.get_document("index.md").expect("get index.md");
        assert_eq!(doc.content.trim(), "_TODO_");
    }

    #[test]
    fn import_is_noop_when_docstore_already_populated() {
        let identity_tmp = make_identity_dir();
        let identity_dir = identity_tmp.path().join(AGENTS_DIRNAME);

        let source_tmp = make_source_dir();

        // First import.
        populate_docstore_from_source(&identity_dir, source_tmp.path(), "index.md").expect("first import");
        let store = IDocStore::open(&identity_dir).expect("open");
        let count_before = store.list_documents().expect("list before").len();

        // Second import — must not change count.
        populate_docstore_from_source(&identity_dir, source_tmp.path(), "index.md").expect("second import");
        let count_after = store.list_documents().expect("list after").len();

        assert_eq!(count_before, count_after, "second import must be a no-op");
    }
}
