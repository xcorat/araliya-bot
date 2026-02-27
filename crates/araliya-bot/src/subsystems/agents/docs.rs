//! Docs agent plugin — answers questions about the project documentation using RAG.
//!
//! On each query:
//! 1. Opens the agent's session store (when `memory = ["basic_session"]`) and gets or creates
//!    a session under the docs agent identity directory for transcript persistence.
//! 2. Opens the agent's [`IDocStore`] and performs BM25 full-text search.
//! 3. If results are found, uses the top-ranked chunk text as context.
//! 4. If nothing is found (empty docstore or no match), falls back to reading
//!    the configured index document directly from the docstore.
//! 5. Builds a prompt from `config/prompts/docs_qa.txt` and forwards it to the LLM.
//! 6. Appends user and assistant messages to the session transcript and accumulates spend.

use std::fs;
use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::warn;

use crate::error::AppError;
use crate::subsystems::memory::stores::docstore::IDocStore;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use super::core::prompt::PromptBuilder;
use super::{Agent, AgentsState};

const ERR_INTERNAL: i32 = -32000;

/// Number of BM25 chunks to surface per query.
const TOP_K: usize = 5;

/// Number of recent transcript entries to include as conversation context (multi-turn).
const CONTEXT_WINDOW: usize = 20;

pub(crate) struct DocsAgentPlugin;

impl Agent for DocsAgentPlugin {
    fn id(&self) -> &str { "docs" }

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
            if action == "health" {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: "docs component: active".to_string(),
                    session_id,
                    usage: None,
                }));
                return;
            }

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

            // Get or create a session under the docs agent identity directory for transcript persistence.
            let state_for_session = state.clone();
            let requested_session_id = session_id.clone();
            let session_handle = tokio::task::spawn_blocking(move || {
                let store = state_for_session.open_agent_store("docs")?;
                let memory = state_for_session.memory.as_ref();
                if let Some(ref sid) = requested_session_id {
                    memory.load_session_in(
                        &store.agent_sessions_dir(),
                        &store.agent_sessions_index(),
                        sid,
                        Some("docs"),
                    )
                } else {
                    store.get_or_create_session(memory, "docs")
                }
            })
            .await
            .map_err(|e| AppError::Memory(format!("docs agent: spawn_blocking panic: {e}")))
            .and_then(|r| r);

            let handle = match session_handle {
                Ok(h) => h,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(ERR_INTERNAL, e.to_string())));
                    return;
                }
            };

            // Append user message to session transcript.
            if let Err(e) = handle.transcript_append("user", &query).await {
                warn!(error = %e, "docs agent: transcript_append(user) failed");
            }

            // Optional: recent conversation context for multi-turn (template may use {{history}}).
            let history = match handle.transcript_read_last(CONTEXT_WINDOW).await {
                Ok(entries) => {
                    let mut ctx = String::new();
                    for entry in entries.iter().rev().skip(1).rev() {
                        ctx.push_str(&format!("{}: {}\n", entry.role, entry.content));
                    }
                    ctx
                }
                Err(e) => {
                    warn!(error = %e, "docs agent: transcript_read_last failed");
                    String::new()
                }
            };

            // Retrieve agent identity dir and run RAG (blocking).
            let identity_dir = match state.agent_identities.get("docs") {
                Some(id) => id.identity_dir.clone(),
                None => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_INTERNAL,
                        "docs agent identity not found".to_string(),
                    )));
                    return;
                }
            };

            let index_name = state
                .docs_index_name
                .clone()
                .unwrap_or_else(|| "index.md".to_string());
            let query_for_store = query.clone();
            let use_kg = state.docs_use_kg;
            let kg_cfg = state.docs_kg_config.clone();

            let context_result: Result<String, AppError> =
                tokio::task::spawn_blocking(move || {
                    // ── KG-RAG path (feature-gated) ──────────────────────────────
                    #[cfg(feature = "ikgdocstore")]
                    if use_kg {
                        use crate::subsystems::memory::stores::kg_docstore::{
                            IKGDocStore, KgConfig,
                        };
                        let kg_store = IKGDocStore::open(&identity_dir)?;
                        let cfg = KgConfig {
                            min_entity_mentions: kg_cfg.min_entity_mentions,
                            bfs_max_depth: kg_cfg.bfs_max_depth,
                            edge_weight_threshold: kg_cfg.edge_weight_threshold,
                            max_chunks: kg_cfg.max_chunks,
                            fts_share: kg_cfg.fts_share,
                            max_seeds: kg_cfg.max_seeds,
                        };
                        let result = kg_store.search_with_kg(&query_for_store, &cfg)?;
                        return Ok(result.context);
                    }

                    // ── Standard FTS path (IDocStore) ────────────────────────────
                    let docstore = IDocStore::open(&identity_dir)?;

                    let results = docstore.search_by_text(&query_for_store, TOP_K)?;

                    if !results.is_empty() {
                        let context = results
                            .iter()
                            .map(|r| r.chunk.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n\n---\n\n");
                        return Ok(context);
                    }

                    tracing::debug!(
                        "docs agent: no BM25 results for query; falling back to index document '{}'",
                        index_name
                    );
                    let doc = docstore.get_document(&index_name).map_err(|e| {
                        AppError::Memory(format!(
                            "no docs available (docstore empty or not imported): {e}"
                        ))
                    })?;

                    let content = if doc.content.len() > 200_000 {
                        tracing::warn!(
                            "docs agent: index document is large ({} bytes); truncating to 200 KB",
                            doc.content.len()
                        );
                        doc.content[..200_000].to_string()
                    } else {
                        doc.content
                    };

                    Ok(content)
                })
                .await
                .unwrap_or_else(|e| {
                    Err(AppError::Memory(format!("docs agent: spawn_blocking panic: {e}")))
                });

            let context = match context_result {
                Ok(c) => c,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(ERR_INTERNAL, e.to_string())));
                    return;
                }
            };

            // Build the LLM prompt (optionally include conversation history).
            let question_section = if history.trim().is_empty() {
                query.clone()
            } else {
                format!("Conversation history:\n{}\n\nQuestion:\n{}", history.trim(), query)
            };
            let system = super::core::prompt::preamble("config/prompts", &state.enabled_tools).build();

            let body = fs::read_to_string("config/prompts/docs_qa.txt")
                .unwrap_or_else(|_| "Documentation:\n{{docs}}\n\nQuestion:\n{{question}}\n".to_string());
            let prompt = PromptBuilder::new("config/prompts")
                .append(body)
                .var("docs", &context)
                .var("question", &question_section)
                .build();

            let llm_result = state.complete_via_llm_with_system(&channel_id, &prompt, Some(&system)).await;

            // Persist assistant reply and spend to the session.
            if let Ok(BusPayload::CommsMessage { content: ref reply, ref usage, .. }) = llm_result {
                if let Err(e) = handle.transcript_append("assistant", reply).await {
                    warn!(error = %e, "docs agent: transcript_append(assistant) failed");
                }
                if let Some(u) = usage {
                    if let Err(e) = handle.accumulate_spend(u, &state.llm_rates).await {
                        warn!(error = %e, "docs agent: accumulate_spend failed");
                    }
                }
            }

            match llm_result {
                Ok(BusPayload::CommsMessage { content, usage, .. }) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content,
                        session_id: Some(handle.session_id.clone()),
                        usage,
                    }));
                }
                Ok(other) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_INTERNAL,
                        format!("unexpected LLM reply: {other:?}"),
                    )));
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                }
            }
        });
    }
}
