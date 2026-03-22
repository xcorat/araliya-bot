//! News aggregator sub-agent plugin.
//!
//! Accepts batches of article URLs from source agents (e.g. newsroom, news),
//! fetches each article, summarises it via the instruction LLM, and stores
//! the summary as a document in an [`IKGDocStore`] rooted at the aggregator's
//! own identity directory.  The knowledge graph is rebuilt after each
//! successful aggregation run.
//!
//! The aggregator owns its own identity directory and KGDocStore, independent
//! of source agents.  This makes it reusable across multiple sources and
//! independently inspectable via the KG inspector UI.
//!
//! ## Actions
//!
//! | Action | Effect |
//! |--------|--------|
//! | `aggregate` | Fetch URLs from payload, summarise, add to KG |
//! | `status` | Return doc/entity/relation counts as JSON |
//! | `search <query>` | KG-RAG search over aggregated articles
//!
//! ## Payload Format for `aggregate`
//!
//! Expects JSON: `{"urls": ["...", "..."], "source_agent": "newsroom"}`
//! Empty/legacy calls (empty string) are no-ops. |

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tracing::{error, info, warn};

use araliya_core::bus::message::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use araliya_memory::stores::kg_docstore::{IKGDocStore, KgConfig};
use araliya_memory::stores::sqlite_core::Document;

use super::{Agent, AgentsState};

const MAX_ARTICLE_CHARS: usize = 4_000;
#[allow(dead_code)]
const BATCH_LIMIT: i64 = 50; // Process up to 50 URLs per cycle — matches GDELT fetch limit
const FETCH_TIMEOUT_S: u64 = 15;
const CHUNK_SIZE: usize = 512;
const FETCH_DELAY_MS: u64 = 1_500;

const ARTICLE_SYSTEM: &str = "You are a concise news summarizer. \
     Summarize the given article in 2-3 short paragraphs covering: \
     who is involved, what happened, where, when, and why it matters. \
     Be factual and neutral. Do not include URLs or source attribution.";

// ── Payload ────────────────────────────────────────────────────────────────────

/// Request payload for the `aggregate` action.
///
/// Sent as a JSON string in the `content` field of the bus message.
/// An empty or non-JSON payload is treated as a legacy no-op call.
#[derive(Debug, serde::Deserialize, Default)]
struct AggregateRequest {
    /// URLs to fetch, summarise, and add to the aggregator's KGDocStore.
    #[serde(default)]
    urls: Vec<String>,
    /// Informational tag identifying the source agent (e.g. "newsroom", "news").
    /// Logged but not acted upon; helps with debugging.
    #[serde(default)]
    source_agent: String,
}

impl AggregateRequest {
    /// Parse from raw content string. Returns `None` for empty/legacy calls.
    fn parse(content: &str) -> Option<Self> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }
}

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
                "aggregate" => {
                    handle_aggregate(channel_id, content, session_id, state, reply_tx).await
                }
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

/// Core aggregation logic — accepts URLs directly from the request payload.
///
/// Returns a human-readable result string (for logging or direct reply).
async fn do_aggregate(channel_id: String, urls: Vec<String>, state: Arc<AgentsState>) -> String {
    // ── 1. Resolve aggregator identity dir ──────────────────────────────────
    let agg_dir = match state.agent_identities.get("news_aggregator") {
        Some(id) => id.identity_dir.clone(),
        None => {
            return "news_aggregator: no identity dir — agent not registered".to_string();
        }
    };

    // ── 2. Find already-aggregated URLs from KGDocStore ────────────────────
    let dir_known = agg_dir.clone();
    let known_result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&dir_known)
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

    // Filter to genuinely new URLs.
    let new_urls: Vec<String> = urls
        .into_iter()
        .filter(|u| !u.is_empty() && !known_urls.contains(u))
        .collect();

    if new_urls.is_empty() {
        info!(
            known = known_urls.len(),
            "news_aggregator: no new articles to aggregate"
        );
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
        let dir = agg_dir.clone();
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

    // ── 3. Rebuild KG ───────────────────────────────────────────────────────
    // Use min_entity_mentions=1 so small corpora (few articles) still produce
    // a non-empty graph.  The default of 2 filters out almost everything when
    // the corpus has fewer than ~10 documents.
    if processed > 0 {
        let dir = agg_dir.clone();
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
            Err(e) => {
                error!(error = %e, "news_aggregator: KG rebuild FAILED — graph will be stale")
            }
        }
    }

    let total_in_kg = processed + known_urls.len();
    info!(
        processed,
        skipped, total_in_kg, "news_aggregator: aggregation cycle complete"
    );
    format!(
        "Aggregated {processed} new article(s) into the knowledge graph \
         ({skipped} skipped). KG now covers {total_in_kg} article(s)."
    )
}

async fn handle_aggregate(
    channel_id: String,
    content: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    // Parse payload — empty string / non-JSON = legacy no-op.
    let request = match AggregateRequest::parse(&content) {
        Some(req) => req,
        None => {
            // Legacy call with empty payload — return status immediately.
            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: "news_aggregator: no URLs provided (legacy call).".to_string(),
                session_id,
                usage: None,
                timing: None,
                thinking: None,
            }));
            return;
        }
    };

    if request.urls.is_empty() {
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
            channel_id,
            content: "news_aggregator: empty URL list — nothing to do.".to_string(),
            session_id,
            usage: None,
            timing: None,
            thinking: None,
        }));
        return;
    }

    // Ack immediately with summary of what we'll do.
    if reply_tx
        .send(Ok(BusPayload::CommsMessage {
            channel_id: channel_id.clone(),
            content: format!(
                "Aggregation started for {} URL(s) from {}.",
                request.urls.len(),
                request.source_agent
            ),
            session_id,
            usage: None,
            timing: None,
            thinking: None,
        }))
        .is_err()
    {
        warn!("news_aggregator: caller dropped reply_tx before ack — proceeding anyway");
    }

    // Spawn the long-running aggregation as a background task.
    tokio::spawn(async move {
        let result = do_aggregate(channel_id, request.urls, state).await;
        tracing::info!(result = %result, "news_aggregator: background aggregation complete");
    });
}

// ── status ────────────────────────────────────────────────────────────────────

async fn handle_status(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let agg_dir = match state.agent_identities.get("news_aggregator") {
        Some(id) => id.identity_dir.clone(),
        None => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                "news_aggregator: no identity dir — agent not registered".to_string(),
            )));
            return;
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&agg_dir)
            .map_err(|e| format!("news_aggregator: open kgdocstore: {e}"))?;
        let doc_count = store
            .list_documents()
            .map_err(|e| format!("news_aggregator: list documents: {e}"))?
            .len();

        // Count entities and relations from the KG graph file.
        let graph_path = agg_dir.join("kgdocstore").join("kg").join("graph.json");
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

    let agg_dir = match state.agent_identities.get("news_aggregator") {
        Some(id) => id.identity_dir.clone(),
        None => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                "news_aggregator: no identity dir — agent not registered".to_string(),
            )));
            return;
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        let store = IKGDocStore::open(&agg_dir)
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
        .skip_tags(vec![
            "script", "style", "head", "nav", "footer", "iframe", "img",
        ])
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
