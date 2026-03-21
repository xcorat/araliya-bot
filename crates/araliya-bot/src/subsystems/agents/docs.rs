//! Docs agent plugin — answers questions about project documentation using RAG.
//!
//! ## Workflow (agentic multi-pass)
//!
//! 1. **Instruction pass** — the LLM formulates the best search query from the
//!    user's question.
//! 2. **`docs_search` local tool** — runs BM25 full-text search (or KG-RAG when
//!    `use_kg = true`) against the agent's [`IDocStore`] / [`IKGDocStore`].
//! 3. **Response pass** — the main LLM answers the question using the retrieved
//!    documentation as context, with conversation history for multi-turn support.
//!
//! The [`AgenticLoop`] in `core::agentic` manages the session lifecycle,
//! transcript persistence, and spend accounting; this file provides the
//! docs-specific [`LocalTool`] implementation and the thin plugin wrapper.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::warn;

use super::core::agentic::{AgenticLoop, LocalTool};
use super::{Agent, AgentsState};
use crate::config::DocsKgConfig;
use crate::subsystems::memory::stores::docstore::IDocStore;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

const ERR_INTERNAL: i32 = -32000;

/// Number of BM25 chunks to surface per query.
const TOP_K: usize = 5;

// ── DocsRagTool ───────────────────────────────────────────────────────────────

/// In-process RAG tool — performs BM25 or KG search against the docs docstore.
///
/// Runs inside `tokio::task::spawn_blocking` (via [`AgenticLoop`]).
// TODO: RAG tool should be a tool within the memory subsystem?
pub(crate) struct DocsRagTool {
    pub(crate) identity_dir: PathBuf,
    pub(crate) index_name: String,
    #[cfg_attr(not(feature = "ikgdocstore"), allow(dead_code))]
    pub(crate) use_kg: bool,
    #[cfg_attr(not(feature = "ikgdocstore"), allow(dead_code))]
    pub(crate) kg_cfg: DocsKgConfig,
}

impl LocalTool for DocsRagTool {
    fn name(&self) -> &str {
        "docs_search"
    }

    fn description(&self) -> &str {
        "action: \"search\", params: {\"query\": \"<search terms>\"}\n  \
         Description: Searches the documentation and returns the most relevant passages."
    }

    fn call(&self, params: &serde_json::Value) -> Result<String, String> {
        let query = params
            .get("query")
            .and_then(|q| q.as_str())
            .unwrap_or("")
            .to_string();

        if query.trim().is_empty() {
            return Err("docs_search: empty query".to_string());
        }

        // ── KG-RAG path (feature-gated) ──────────────────────────────
        #[cfg(feature = "ikgdocstore")]
        if self.use_kg {
            use crate::subsystems::memory::stores::kg_docstore::{IKGDocStore, KgConfig};
            let kg_store = IKGDocStore::open(&self.identity_dir).map_err(|e| e.to_string())?;
            let cfg = KgConfig {
                min_entity_mentions: self.kg_cfg.min_entity_mentions,
                bfs_max_depth: self.kg_cfg.bfs_max_depth,
                edge_weight_threshold: self.kg_cfg.edge_weight_threshold,
                max_chunks: self.kg_cfg.max_chunks,
                fts_share: self.kg_cfg.fts_share,
                max_seeds: self.kg_cfg.max_seeds,
            };
            let result = kg_store
                .search_with_kg(&query, &cfg)
                .map_err(|e| e.to_string())?;
            return Ok(result.context);
        }

        // ── Standard FTS path (IDocStore) ────────────────────────────
        let docstore = IDocStore::open(&self.identity_dir).map_err(|e| e.to_string())?;
        let results = docstore
            .search_by_text(&query, TOP_K)
            .map_err(|e| e.to_string())?;

        if !results.is_empty() {
            let context = results
                .iter()
                .map(|r| r.chunk.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            return Ok(context);
        }

        // Fall back to the index document when no BM25 results are found.
        tracing::debug!(
            "docs_search: no BM25 results for query; falling back to index '{}'",
            self.index_name
        );
        let doc = docstore
            .get_document(&self.index_name)
            .map_err(|e| format!("no docs available (docstore empty or not imported): {e}"))?;

        let content = if doc.content.len() > 200_000 {
            tracing::warn!(
                "docs_search: index document is large ({} bytes); truncating to 200 KB",
                doc.content.len()
            );
            doc.content[..200_000].to_string()
        } else {
            doc.content
        };

        Ok(content)
    }
}

// ── Shared setup ─────────────────────────────────────────────────────────────

/// Validated setup for a docs agent request.  Returned by [`prepare_docs_loop`].
struct DocsSetup {
    query: String,
    loop_: AgenticLoop,
}

/// Validate the request, resolve the docstore, and build the [`AgenticLoop`].
///
/// Returns `Err(BusResult)` on validation failures that should be sent back
/// to the caller immediately.
fn prepare_docs_loop(
    action: &str,
    content: String,
    state: &AgentsState,
) -> Result<DocsSetup, BusResult> {
    // Action routing.
    let query = if action == "ask" || action.is_empty() || action == "handle" {
        content
    } else {
        return Err(Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("unknown docs action: {action}"),
        )));
    };

    if query.trim().is_empty() {
        return Err(Err(BusError::new(
            ERR_INTERNAL,
            "empty query: please provide a question about the docs".to_string(),
        )));
    }

    // Resolve agent identity directory for the docstore.
    let identity_dir = match state.agent_identities.get("docs") {
        Some(id) => id.identity_dir.clone(),
        None => {
            warn!("docs agent: identity not found");
            return Err(Err(BusError::new(
                ERR_INTERNAL,
                "docs agent identity not found".to_string(),
            )));
        }
    };

    // Guard: reject queries when the docstore has no documents yet.
    {
        let store = match IDocStore::open(&identity_dir) {
            Ok(s) => s,
            Err(e) => {
                return Err(Err(BusError::new(
                    ERR_INTERNAL,
                    format!("docs docstore unavailable: {e}"),
                )));
            }
        };
        let empty = store
            .list_documents()
            .map(|docs| docs.is_empty())
            .unwrap_or(true);
        if empty {
            return Err(Err(BusError::new(
                ERR_INTERNAL,
                "docs docstore is empty — import documents before querying".to_string(),
            )));
        }
    }

    let docs_cfg = state.agent_docs.get("docs");
    let index_name = docs_cfg
        .and_then(|d| d.index.clone())
        .unwrap_or_else(|| "index.md".to_string());
    let use_kg = docs_cfg.map(|d| d.use_kg).unwrap_or(false);
    let kg_cfg = docs_cfg.map(|d| d.kg.clone()).unwrap_or_default();

    let rag_tool: Arc<dyn LocalTool + Send + Sync> = Arc::new(DocsRagTool {
        identity_dir,
        index_name,
        use_kg,
        kg_cfg,
    });

    let allowed_tools = state.agent_skills.get("docs").cloned().unwrap_or_default();

    let loop_ = AgenticLoop::new(
        "docs",
        false,
        "instruct.md",
        "context.md",
        vec![],
        vec![rag_tool],
        allowed_tools,
        &state.agents_dir,
        state.debug_logging,
    );

    Ok(DocsSetup { query, loop_ })
}

// ── DocsAgentPlugin ───────────────────────────────────────────────────────────

pub(crate) struct DocsAgentPlugin;

impl Agent for DocsAgentPlugin {
    fn id(&self) -> &str {
        "docs"
    }

    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            // Health ping — no LLM call needed.
            if action == "health" {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: "docs component: active".to_string(),
                    session_id,
                    usage: None,
                    timing: None,
                    thinking: None,
                }));
                return;
            }

            let setup = match prepare_docs_loop(&action, content, &state) {
                Ok(s) => s,
                Err(result) => {
                    let _ = reply_tx.send(result);
                    return;
                }
            };

            let result = setup
                .loop_
                .run(channel_id, setup.query, session_id, state)
                .await;
            let _ = reply_tx.send(result);
        });
    }

    fn handle_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            let setup = match prepare_docs_loop("handle", content, &state) {
                Ok(s) => s,
                Err(result) => {
                    let _ = reply_tx.send(result);
                    return;
                }
            };

            let result = setup
                .loop_
                .run_stream(channel_id, setup.query, session_id, state)
                .await;
            let _ = reply_tx.send(result);
        });
    }
}
