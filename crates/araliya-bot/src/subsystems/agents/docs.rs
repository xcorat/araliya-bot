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
struct DocsRagTool {
    identity_dir: PathBuf,
    index_name: String,
    #[cfg_attr(not(feature = "ikgdocstore"), allow(dead_code))]
    use_kg: bool,
    #[cfg_attr(not(feature = "ikgdocstore"), allow(dead_code))]
    kg_cfg: DocsKgConfig,
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
                }));
                return;
            }

            // Action routing.
            let query = if action == "ask" || action.is_empty() || action == "handle" {
                content
            } else {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unknown docs action: {action}"),
                )));
                return;
            };

            if query.trim().is_empty() {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_INTERNAL,
                    "empty query: please provide a question about the docs".to_string(),
                )));
                return;
            }

            // Resolve agent identity directory for the docstore.
            let identity_dir = match state.agent_identities.get("docs") {
                Some(id) => id.identity_dir.clone(),
                None => {
                    warn!("docs agent: identity not found");
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_INTERNAL,
                        "docs agent identity not found".to_string(),
                    )));
                    return;
                }
            };

            let rag_tool: Arc<dyn LocalTool + Send + Sync> = Arc::new(DocsRagTool {
                identity_dir,
                index_name: state
                    .docs_index_name
                    .clone()
                    .unwrap_or_else(|| "index.md".to_string()),
                use_kg: state.docs_use_kg,
                kg_cfg: state.docs_kg_config.clone(),
            });

            let allowed_tools = state
                .agent_skills
                .get("docs")
                .cloned()
                .unwrap_or_default();

            let loop_ = AgenticLoop::new(
                "docs",
                false,
                "docs_instruct.txt",
                "docs_context.txt",
                vec![rag_tool],
                allowed_tools,
                "config/prompts",
                state.debug_logging,
            );

            let result = loop_.run(channel_id, query, session_id, state).await;
            let _ = reply_tx.send(result);
        });
    }
}
