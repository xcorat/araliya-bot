//! News aggregator sub-agent plugin.
//!
//! Reads source URLs from the newsroom agent's SQLite events database,
//! fetches each article, summarises it via the instruction LLM, and stores
//! the summary as a document in an [`IKGDocStore`] rooted at the **newsroom
//! agent's own identity directory**.  The knowledge graph is rebuilt after
//! each successful aggregation run.
//!
//! The IKGDocStore is shared with — and lives inside — the newsroom agent's
//! identity folder (`{newsroom_identity_dir}/kgdocstore/`), so there is no
//! separate subagent identity required.
//!
//! ## Actions
//!
//! | Action | Effect |
//! |--------|--------|
//! | `aggregate` | Fetch new articles, summarise, add to KG |
//! | `status` | Return doc/entity/relation counts as JSON |
//! | `search <query>` | KG-RAG search over aggregated articles |

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tracing::{error, info, warn};

use crate::subsystems::memory::stores::kg_docstore::{IKGDocStore, KgConfig};
use crate::subsystems::memory::stores::sqlite_core::Document;
use crate::subsystems::memory::stores::sqlite_store::SqlValue;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

use super::{Agent, AgentsState};

const MAX_ARTICLE_CHARS: usize = 4_000;
const BATCH_LIMIT: i64 = 50; // Process up to 50 URLs per cycle — matches GDELT fetch limit
const FETCH_TIMEOUT_S: u64 = 15;
const CHUNK_SIZE: usize = 512;
const FETCH_DELAY_MS: u64 = 1_500;

const ARTICLE_SYSTEM: &str =
    "You are a concise news summarizer. \
     Summarize the given article in 2-3 short paragraphs covering: \
     who is involved, what happened, where, when, and why it matters. \
     Be factual and neutral. Do not include URLs or source attribution.";

// ── Agent ─────────────────────────────────────────────────────────────────────

pub(crate) struct NewsAggregatorAgent;

impl Agent for NewsAggregatorAgent {
    fn id(&self) -> &str {
        "news_aggregator"
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
        // Web UI always delivers action="handle"; use content as the action word.
        let effective = if action == "handle" && !content.trim().is_empty() {
            let lower = content.trim().to_lowercase();
            if lower.starts_with("search") {
                "search".to_string()
            } else {
                lower
            }
        } else {
            action
        };

        tokio::spawn(async move {
            match effective.as_str() {
                "aggregate" => handle_aggregate(channel_id, session_id, state, reply_tx).await,
                "status" => handle_status(channel_id, session_id, state, reply_tx).await,
                "search" => handle_search(content, channel_id, session_id, state, reply_tx).await,
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_METHOD_NOT_FOUND,
                        format!("unknown news_aggregator action: {effective}"),
                    )));
                }
            }
        });
    }
}

// ── aggregate ─────────────────────────────────────────────────────────────────

/// Core aggregation logic — shared between the explicit `aggregate` action and
/// the background trigger invoked automatically by the newsroom agent after each
/// successful summary update.
///
/// Returns a human-readable result string (for logging or direct reply).
async fn do_aggregate(channel_id: String, state: Arc<AgentsState>) -> String {
    // ── 1. Resolve newsroom identity dir ────────────────────────────────────
    let newsroom_dir = match state.agent_identities.get("newsroom") {
        Some(id) => id.identity_dir.clone(),
        None => {
            return "news_aggregator: newsroom agent not found — enable plugin-newsroom-agent"
                .to_string();
        }
    };

    // ── 2. Load source URLs from newsroom events DB using a forward cursor ──
    //
    // We persist `last_processed_id` in an `agg_state` table inside the
    // newsroom events DB.  Each cycle reads events with `id > cursor` in
    // ascending order, then advances the cursor to the highest id seen.
    // This ensures every event is eventually processed and dead/transient
    // URLs never block the queue indefinitely.
    let state_db = state.clone();
    let urls_result = tokio::task::spawn_blocking(move || {
        let store = state_db
            .open_sqlite_store("newsroom", "events")
            .map_err(|e| format!("news_aggregator: open events db: {e}"))?;

        // Ensure the cursor state table exists.
        store
            .execute_ddl(
                "CREATE TABLE IF NOT EXISTS agg_state \
                 (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            )
            .map_err(|e| format!("news_aggregator: create agg_state table: {e}"))?;

        // Read the cursor (defaults to 0 so all existing events are eligible).
        let cursor_rows = store
            .query_rows(
                "SELECT value FROM agg_state WHERE key = 'last_processed_id'",
                &[],
            )
            .map_err(|e| format!("news_aggregator: read cursor: {e}"))?;
        let cursor: i64 = cursor_rows
            .first()
            .and_then(|r| r.get("value"))
            .and_then(|v| {
                if let SqlValue::Text(s) = v {
                    s.parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        // Fetch the next batch of events after the cursor, oldest-first.
        let rows = store
            .query_rows(
                "SELECT id, source_url FROM events \
                 WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
                &[SqlValue::Integer(cursor), SqlValue::Integer(BATCH_LIMIT)],
            )
            .map_err(|e| format!("news_aggregator: query events: {e}"))?;

        let pairs: Vec<(i64, String)> = rows
            .into_iter()
            .filter_map(|r| {
                let id = match r.get("id") {
                    Some(SqlValue::Integer(i)) => *i,
                    _ => return None,
                };
                let url = match r.get("source_url") {
                    Some(SqlValue::Text(s)) => s.clone(),
                    _ => return None,
                };
                Some((id, url))
            })
            .collect();

        Ok::<(i64, Vec<(i64, String)>), String>((cursor, pairs))
    })
    .await
    .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking urls: {e}")));

    let (cursor, event_pairs) = match urls_result {
        Ok(v) => v,
        Err(e) => return e,
    };

    // ── 3. Find already-aggregated URLs from KGDocStore ─────────────────────
    let dir2 = newsroom_dir.clone();
    let known_result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&dir2)
            .map_err(|e| format!("news_aggregator: open kgdocstore: {e}"))?;
        let known: HashSet<String> = store
            .list_documents()
            .map_err(|e| format!("news_aggregator: list documents: {e}"))?
            .into_iter()
            .map(|d| d.source)
            .collect();
        Ok::<HashSet<String>, String>(known)
    })
    .await
    .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking known: {e}")));

    let known_urls = match known_result {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Capture the highest event id in the full batch BEFORE filtering — this
    // is used to advance the cursor even when all URLs are already known.
    let max_id_in_batch: Option<i64> = event_pairs.iter().map(|(id, _)| *id).max();

    // Filter to genuinely new URLs.
    let new_urls: Vec<String> = event_pairs
        .into_iter()
        .filter(|(_, u)| !u.is_empty() && !known_urls.contains(u))
        .map(|(_, u)| u)
        .collect();

    if new_urls.is_empty() {
        info!(
            cursor,
            known = known_urls.len(),
            "news_aggregator: no new articles to aggregate"
        );
        // Still advance the cursor so future cycles see events beyond this batch.
        if let Some(new_cursor) = max_id_in_batch {
            let state_adv = state.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(store) = state_adv.open_sqlite_store("newsroom", "events") {
                    let _ = store.execute(
                        "INSERT INTO agg_state (key, value) VALUES ('last_processed_id', ?1) \
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        &[SqlValue::Text(new_cursor.to_string())],
                    );
                }
            })
            .await
            .ok();
        }
        return "No new articles to aggregate.".to_string();
    }

    // ── 4. Fetch → summarise → store each article ────────────────────────────
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_S))
        .user_agent("Mozilla/5.0 (compatible; AraliyaBot/1.0; +https://github.com/)")
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("news_aggregator: build http client: {e}"),
    };

    let total_new = new_urls.len();
    info!(
        total_new,
        "news_aggregator: starting aggregation for new articles"
    );
    let mut processed = 0usize;
    let mut skipped = 0usize;

    for url in &new_urls {
        // a) Fetch HTML
        let html = match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    warn!(url = %url, error = %e, "news_aggregator: read response body");
                    skipped += 1;
                    continue;
                }
            },
            Ok(resp) => {
                warn!(url = %url, status = %resp.status(), "news_aggregator: non-2xx");
                skipped += 1;
                continue;
            }
            Err(e) => {
                warn!(url = %url, error = %e, "news_aggregator: fetch");
                skipped += 1;
                continue;
            }
        };

        // b) Strip tags and truncate
        let text = strip_html(&html);
        let truncated = truncate_chars(&text, MAX_ARTICLE_CHARS);
        if truncated.trim().is_empty() {
            warn!(url = %url, "news_aggregator: stripped article body is empty — skipping");
            skipped += 1;
            continue;
        }

        // c) Summarise via instruction LLM
        let prompt = format!("Article URL: {url}\n\nArticle text:\n{truncated}");
        let summary = match state
            .complete_via_instruct_llm(&channel_id, &prompt, Some(ARTICLE_SYSTEM))
            .await
        {
            Ok(BusPayload::CommsMessage { content, .. }) => content,
            Ok(_) => {
                warn!(url = %url, "news_aggregator: unexpected LLM reply type");
                skipped += 1;
                continue;
            }
            Err(e) => {
                warn!(url = %url, error = ?e, "news_aggregator: LLM summarize");
                skipped += 1;
                continue;
            }
        };

        // d) Store in KGDocStore (blocking)
        let dir = newsroom_dir.clone();
        let url_c = url.clone();
        let summary_c = summary.clone();
        let store_result = tokio::task::spawn_blocking(move || {
            let store = IKGDocStore::open(&dir)
                .map_err(|e| format!("news_aggregator: open kgdocstore for insert: {e}"))?;
            let doc = Document {
                id: String::new(),
                title: url_c.clone(),
                source: url_c,
                content: summary_c,
                content_hash: String::new(),
                created_at: String::new(),
                metadata: Default::default(),
            };
            let doc_id = store
                .add_document(doc)
                .map_err(|e| format!("news_aggregator: add_document: {e}"))?;
            let chunks = store
                .chunk_document(&doc_id, CHUNK_SIZE)
                .map_err(|e| format!("news_aggregator: chunk_document: {e}"))?;
            store
                .index_chunks(chunks)
                .map_err(|e| format!("news_aggregator: index_chunks: {e}"))?;
            Ok::<(), String>(())
        })
        .await
        .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking store: {e}")));

        match store_result {
            Ok(()) => processed += 1,
            Err(e) => {
                error!(error = %e, url = %url, "news_aggregator: failed to store article in KG");
                skipped += 1;
            }
        }

        // Polite delay between requests
        if processed + skipped < total_new {
            tokio::time::sleep(Duration::from_millis(FETCH_DELAY_MS)).await;
        }
    }

    // ── 5. Rebuild KG ────────────────────────────────────────────────────────
    // Use min_entity_mentions=1 so small corpora (few articles) still produce
    // a non-empty graph.  The default of 2 filters out almost everything when
    // the corpus has fewer than ~10 documents.
    if processed > 0 {
        let dir = newsroom_dir.clone();
        let doc_count = processed + known_urls.len();
        let cfg = KgConfig {
            min_entity_mentions: if doc_count < 10 { 1 } else { 2 },
            ..KgConfig::default()
        };
        match tokio::task::spawn_blocking(move || {
            IKGDocStore::open(&dir)
                .and_then(|s| s.rebuild_kg_with_config(&cfg, &[]))
                .map_err(|e| format!("news_aggregator: rebuild_kg: {e}"))
        })
        .await
        .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking rebuild: {e}")))
        {
            Ok(()) => info!(doc_count, "news_aggregator: KG rebuilt successfully"),
            Err(e) => error!(error = %e, "news_aggregator: KG rebuild FAILED — graph will be stale"),
        }
    }

    // ── 6. Advance the cursor ────────────────────────────────────────────────
    // Advance even when some URLs were skipped/failed, so transient errors
    // don't block the queue indefinitely.  A retry mechanism can be layered
    // on top later by tracking failed URLs separately.
    if let Some(new_cursor) = max_id_in_batch {
        let state_adv = state.clone();
        let adv_result = tokio::task::spawn_blocking(move || {
            let store = state_adv
                .open_sqlite_store("newsroom", "events")
                .map_err(|e| format!("news_aggregator: open events db (cursor advance): {e}"))?;
            store
                .execute(
                    "INSERT INTO agg_state (key, value) VALUES ('last_processed_id', ?1) \
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    &[SqlValue::Text(new_cursor.to_string())],
                )
                .map_err(|e| format!("news_aggregator: advance cursor: {e}"))?;
            Ok::<(), String>(())
        })
        .await
        .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking cursor: {e}")));

        match adv_result {
            Ok(()) => info!(
                old_cursor = cursor,
                new_cursor,
                "news_aggregator: cursor advanced"
            ),
            Err(e) => error!(error = %e, "news_aggregator: failed to advance cursor"),
        }
    }

    let total_in_kg = processed + known_urls.len();
    info!(
        processed,
        skipped,
        total_in_kg,
        "news_aggregator: aggregation cycle complete"
    );
    format!(
        "Aggregated {processed} new article(s) into the knowledge graph \
         ({skipped} skipped). KG now covers {total_in_kg} article(s)."
    )
}

async fn handle_aggregate(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    // Respond immediately so the bus doesn't time out.
    // The actual aggregation happens in the background.
    if reply_tx
        .send(Ok(BusPayload::CommsMessage {
            channel_id: channel_id.clone(),
            content: "Aggregation started in background.".to_string(),
            session_id,
            usage: None,
            timing: None,
            thinking: None,
        }))
        .is_err()
    {
        warn!("news_aggregator: caller dropped reply receiver before aggregate ack — proceeding anyway");
    }

    // Spawn the long-running aggregation as a background task.
    tokio::spawn(async move {
        let result = do_aggregate(channel_id, state).await;
        if result.starts_with("news_aggregator:") || result.starts_with("No new") {
            tracing::info!(result = %result, "news_aggregator: background aggregation complete");
        } else {
            tracing::info!(result = %result, "news_aggregator: background aggregation complete");
        }
    });
}

// ── status ────────────────────────────────────────────────────────────────────

async fn handle_status(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let newsroom_dir = match state.agent_identities.get("newsroom") {
        Some(id) => id.identity_dir.clone(),
        None => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                "news_aggregator: newsroom agent not found".to_string(),
            )));
            return;
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&newsroom_dir)
            .map_err(|e| format!("news_aggregator: open kgdocstore: {e}"))?;
        let doc_count = store
            .list_documents()
            .map_err(|e| format!("news_aggregator: list documents: {e}"))?
            .len();

        // Count entities and relations from the KG graph file.
        let graph_path = newsroom_dir.join("kgdocstore").join("kg").join("graph.json");
        let (entity_count, relation_count) = if graph_path.exists() {
            match std::fs::read_to_string(&graph_path) {
                Ok(s) => {
                    let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
                    let e = v["entities"].as_object().map_or(0, |o| o.len());
                    let r = v["relations"].as_array().map_or(0, |a| a.len());
                    (e, r)
                }
                Err(_) => (0, 0),
            }
        } else {
            (0, 0)
        };

        Ok::<String, String>(format!(
            r#"{{"doc_count":{doc_count},"entity_count":{entity_count},"relation_count":{relation_count}}}"#
        ))
    })
    .await
    .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking status: {e}")));

    match result {
        Ok(json) => {
            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: json,
                session_id,
                usage: None,
                timing: None,
                thinking: None,
            }));
        }
        Err(e) => {
            let _ = reply_tx.send(Err(BusError::new(-32000, e)));
        }
    }
}

// ── search ────────────────────────────────────────────────────────────────────

async fn handle_search(
    content: String,
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    // Extract query from "search <query>" or just treat content as query.
    let query = {
        let trimmed = content.trim();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("search ") {
            trimmed[7..].trim().to_string()
        } else if lower == "search" {
            String::new()
        } else {
            trimmed.to_string()
        }
    };

    if query.is_empty() {
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
            channel_id,
            content: "Usage: search <query>".to_string(),
            session_id,
            usage: None,
            timing: None,
            thinking: None,
        }));
        return;
    }

    let newsroom_dir = match state.agent_identities.get("newsroom") {
        Some(id) => id.identity_dir.clone(),
        None => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                "news_aggregator: newsroom agent not found".to_string(),
            )));
            return;
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&newsroom_dir)
            .map_err(|e| format!("news_aggregator: open kgdocstore: {e}"))?;
        let kg_result = store
            .search_with_kg(&query, &KgConfig::default())
            .map_err(|e| format!("news_aggregator: search_with_kg: {e}"))?;
        Ok::<String, String>(kg_result.context)
    })
    .await
    .unwrap_or_else(|e| Err(format!("news_aggregator: spawn_blocking search: {e}")));

    match result {
        Ok(ctx) => {
            let content = if ctx.is_empty() {
                "No results found.".to_string()
            } else {
                ctx
            };
            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                session_id,
                usage: None,
                timing: None,
                thinking: None,
            }));
        }
        Err(e) => {
            let _ = reply_tx.send(Err(BusError::new(-32000, e)));
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert HTML to plain text using htmd, skipping script/style tags.
fn strip_html(html: &str) -> String {
    let converter = htmd::HtmlToMarkdown::builder()
        // Skip non-content and media tags.  Crucially, skipping "img" prevents
        // htmd from emitting inline base64 data-URIs (data:image/...;base64,...)
        // which would eat the entire MAX_ARTICLE_CHARS budget before any text.
        .skip_tags(vec!["script", "style", "head", "nav", "footer", "iframe", "img"])
        .build();
    let text = converter.convert(html).unwrap_or_default();
    // Strip any residual data-URI blobs that slipped through (e.g. inline svg
    // or background-image attributes converted to markdown links).
    let text = regex_strip_data_uris(&text);
    // Normalise whitespace.
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove `data:<mime>;base64,<blob>` substrings left after HTML→Markdown conversion.
fn regex_strip_data_uris(s: &str) -> String {
    // A data URI starts with `data:` and the base64 payload has no whitespace,
    // so we can match greedily until the first whitespace or closing bracket.
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("data:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        // Skip to the next whitespace, `'`, `"`, or `)` — whichever comes first.
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | ')' | ']'))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Truncate `s` to at most `max_chars` Unicode scalar values.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}
