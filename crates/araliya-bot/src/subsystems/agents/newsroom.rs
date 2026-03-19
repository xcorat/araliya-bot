//! Newsroom agent — persistent GDELT event store.
//!
//! Fetches global events from BigQuery, inserts new events into a per-agent
//! SQLite database (deduplicating on `source_url`), caps the store at
//! [`EVENT_CAP`] rows, and summarises **only the newly detected events** with
//! the LLM.  If a fetch produces no new rows the agent replies immediately
//! without calling the LLM.
//!
//! ## Actions
//!
//! | Action | Effect |
//! |--------|--------|
//! | `handle` / `latest` | Return the most recent stored summary (no BQ query) |
//! | `read` | Fetch → store → rank sources → summarise new events |
//! | `sources` | Return top-50 sources ranked by score as JSON |
//! | `status` | Return event count + oldest/newest timestamps as JSON |
//! | `health` | Delegate to `gdelt_bigquery:health` |

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::{error, warn};

use crate::subsystems::memory::stores::sqlite_core::now_iso8601;
use crate::subsystems::memory::stores::sqlite_store::{SqlValue, SqliteStore};
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

use super::core::prompt::PromptBuilder;
use super::{Agent, AgentsState};

/// Maximum number of events to retain in the SQLite store.
const EVENT_CAP: i64 = 2500;
/// Maximum number of summaries to retain.
const SUMMARY_CAP: i64 = 10;

const EVENTS_DDL: &str = "
    CREATE TABLE IF NOT EXISTS events (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        date         TEXT    NOT NULL,
        actor1       TEXT    NOT NULL,
        actor2       TEXT    NOT NULL,
        event_code   TEXT    NOT NULL,
        goldstein    REAL    NOT NULL,
        num_articles INTEGER NOT NULL,
        avg_tone     REAL    NOT NULL,
        source_url   TEXT    NOT NULL UNIQUE,
        fetched_at   TEXT    NOT NULL
    );
";

const SUMMARIES_DDL: &str = "
    CREATE TABLE IF NOT EXISTS summaries (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        content    TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
";

const SOURCES_DDL: &str = "
    CREATE TABLE IF NOT EXISTS sources (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        domain       TEXT    NOT NULL UNIQUE,
        root_domain  TEXT    NOT NULL,
        display_name TEXT    NOT NULL DEFAULT '',
        rank         REAL    NOT NULL DEFAULT 0.5,
        fetch_count  INTEGER NOT NULL DEFAULT 0,
        event_count  INTEGER NOT NULL DEFAULT 0,
        avg_tone     REAL    NOT NULL DEFAULT 0.0,
        last_seen    TEXT    NOT NULL,
        created_at   TEXT    NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_sources_rank ON sources (rank DESC);
    CREATE INDEX IF NOT EXISTS idx_sources_root ON sources (root_domain);
";

fn ensure_schema(store: &SqliteStore) -> Result<(), String> {
    store.migrate(1, EVENTS_DDL).map_err(|e| format!("newsroom: migrate v1: {e}"))?;
    store.migrate(2, SUMMARIES_DDL).map_err(|e| format!("newsroom: migrate v2: {e}"))?;
    store.migrate(3, SOURCES_DDL).map_err(|e| format!("newsroom: migrate v3: {e}"))?;
    Ok(())
}

pub(crate) struct NewsroomAgent;

impl Agent for NewsroomAgent {
    fn id(&self) -> &str {
        "newsroom"
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
        // When called via /api/message (web UI), the bus method is always
        // `agents/newsroom` → action="handle". Treat message content as the
        // action so the frontend can dispatch to any handler by content.
        let effective = if action == "handle" && !content.trim().is_empty() {
            content.trim().to_lowercase()
        } else {
            action
        };
        tokio::spawn(async move {
            match effective.as_str() {
                "health" => handle_health(channel_id, session_id, state, reply_tx).await,
                "status" => handle_status(channel_id, session_id, state, reply_tx).await,
                "read" => handle_read(channel_id, session_id, state, reply_tx).await,
                "sources" => handle_sources(channel_id, session_id, state, reply_tx).await,
                "events" => handle_events(channel_id, session_id, state, reply_tx).await,
                "latest" | "handle" => handle_latest(channel_id, session_id, state, reply_tx).await,
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_METHOD_NOT_FOUND,
                        format!("unknown newsroom action: {effective}"),
                    )));
                }
            }
        });
    }
}

// ── health ────────────────────────────────────────────────────────────────────

async fn handle_health(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let result = state
        .execute_tool(
            "gdelt_bigquery",
            "health",
            "{}".to_string(),
            &channel_id,
            session_id.clone(),
        )
        .await;

    let content = match result {
        Ok(BusPayload::ToolResponse { ok: true, .. }) => "newsroom: gdelt_bigquery reachable".to_string(),
        Ok(BusPayload::ToolResponse { ok: false, error, .. }) => {
            format!("newsroom: gdelt_bigquery error: {}", error.unwrap_or_default())
        }
        Ok(other) => format!("newsroom: unexpected health reply: {other:?}"),
        Err(e) => format!("newsroom: health error: {e:?}"),
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

// ── status ────────────────────────────────────────────────────────────────────

async fn handle_status(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let content = tokio::task::spawn_blocking(move || {
        let store = state.open_sqlite_store("newsroom", "events").map_err(|e| {
            format!("newsroom: open store: {e}")
        })?;
        ensure_schema(&store)?;

        let row = store
            .query_one(
                "SELECT COUNT(*) AS cnt, MIN(fetched_at) AS oldest, MAX(fetched_at) AS newest FROM events",
                &[],
            )
            .map_err(|e| format!("newsroom: status query: {e}"))?;

        let (cnt, oldest, newest) = row
            .map(|r| {
                let cnt = match r.get("cnt") {
                    Some(SqlValue::Integer(n)) => *n,
                    _ => 0,
                };
                let oldest = match r.get("oldest") {
                    Some(SqlValue::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                let newest = match r.get("newest") {
                    Some(SqlValue::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                (cnt, oldest, newest)
            })
            .unwrap_or((0, String::new(), String::new()));

        Ok::<String, String>(format!(
            r#"{{"event_count":{cnt},"oldest":{:?},"newest":{:?}}}"#,
            oldest, newest
        ))
    })
    .await
    .unwrap_or_else(|e| Err(format!("newsroom: spawn_blocking: {e}")));

    match content {
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

// ── read ──────────────────────────────────────────────────────────────────────

async fn handle_read(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    // ── 1. Fetch from BigQuery tool ──────────────────────────────────────────
    let result = state
        .execute_tool(
            "gdelt_bigquery",
            "fetch",
            state.newsroom_query_args_json.clone(),
            &channel_id,
            session_id.clone(),
        )
        .await;

    let raw_json = match result {
        Ok(BusPayload::ToolResponse {
            ok: true,
            data_json: Some(data),
            ..
        }) => data,
        Ok(BusPayload::ToolResponse {
            ok: true,
            data_json: None,
            ..
        }) => "[]".to_string(),
        Ok(BusPayload::ToolResponse {
            ok: false, error, ..
        }) => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!(
                    "newsroom: gdelt_bigquery error: {}",
                    error.unwrap_or_else(|| "unknown".to_string())
                ),
            )));
            return;
        }
        Ok(other) => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!("newsroom: unexpected tool reply: {other:?}"),
            )));
            return;
        }
        Err(e) => {
            let _ = reply_tx.send(Err(e));
            return;
        }
    };

    // ── 2. Parse events ──────────────────────────────────────────────────────
    let fetched_events = parse_gdelt_events(&raw_json);

    // ── 3. Insert new events; update source stats; sort by source rank ────────
    let state_store = state.clone();
    let insert_result = tokio::task::spawn_blocking(move || {
        let store = state_store
            .open_sqlite_store("newsroom", "events")
            .map_err(|e| format!("newsroom: open store: {e}"))?;
        ensure_schema(&store)?;

        let now = now_iso8601();
        let mut new_events: Vec<GdeltRow> = Vec::new();

        for event in &fetched_events {
            let rows_affected = store
                .execute(
                    "INSERT OR IGNORE INTO events \
                     (date, actor1, actor2, event_code, goldstein, num_articles, avg_tone, source_url, fetched_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    &[
                        SqlValue::Text(event.date.clone()),
                        SqlValue::Text(event.actor1.clone()),
                        SqlValue::Text(event.actor2.clone()),
                        SqlValue::Text(event.event_code.clone()),
                        SqlValue::Real(event.goldstein),
                        SqlValue::Integer(event.num_articles as i64),
                        SqlValue::Real(event.avg_tone),
                        SqlValue::Text(event.source_url.clone()),
                        SqlValue::Text(now.clone()),
                    ],
                )
                .map_err(|e| format!("newsroom: insert: {e}"))?;
            if rows_affected > 0 {
                new_events.push(event.clone());
            }
        }

        // ── 4. Cap at EVENT_CAP rows ─────────────────────────────────────────
        store
            .execute(
                "DELETE FROM events WHERE id NOT IN \
                 (SELECT id FROM events ORDER BY id DESC LIMIT ?1)",
                &[SqlValue::Integer(EVENT_CAP)],
            )
            .map_err(|e| format!("newsroom: cap: {e}"))?;

        // ── 5. Update source stats grouped by domain ─────────────────────────
        // Group new events by domain so fetch_count increments once per fetch.
        let mut domain_tones: HashMap<String, Vec<f64>> = HashMap::new();
        for event in &new_events {
            let domain = extract_domain(&event.source_url);
            domain_tones.entry(domain).or_default().push(event.avg_tone);
        }

        for (domain, tones) in &domain_tones {
            let root = extract_root_domain(domain);
            let batch_avg = tones.iter().sum::<f64>() / tones.len() as f64;
            let event_count_delta = tones.len() as i64;

            // Ensure row exists.
            store.execute(
                "INSERT OR IGNORE INTO sources \
                 (domain, root_domain, rank, fetch_count, event_count, avg_tone, last_seen, created_at) \
                 VALUES (?1, ?2, 0.5, 0, 0, 0.0, ?3, ?3)",
                &[
                    SqlValue::Text(domain.clone()),
                    SqlValue::Text(root.clone()),
                    SqlValue::Text(now.clone()),
                ],
            ).map_err(|e| format!("newsroom: source insert: {e}"))?;

            // Read current stats for EMA.
            let existing = store.query_one(
                "SELECT avg_tone, event_count FROM sources WHERE domain = ?1",
                &[SqlValue::Text(domain.clone())],
            ).map_err(|e| format!("newsroom: source read: {e}"))?;

            let (old_avg_tone, old_event_count) = match existing {
                Some(row) => {
                    let t = match row.get("avg_tone") { Some(SqlValue::Real(v)) => *v, _ => 0.0 };
                    let c = match row.get("event_count") { Some(SqlValue::Integer(n)) => *n, _ => 0 };
                    (t, c)
                }
                None => (0.0, 0),
            };

            let new_avg_tone = if old_event_count == 0 {
                batch_avg
            } else {
                0.1 * batch_avg + 0.9 * old_avg_tone
            };

            store.execute(
                "UPDATE sources SET \
                 fetch_count = fetch_count + 1, \
                 event_count = event_count + ?1, \
                 avg_tone    = ?2, \
                 last_seen   = ?3 \
                 WHERE domain = ?4",
                &[
                    SqlValue::Integer(event_count_delta),
                    SqlValue::Real(new_avg_tone),
                    SqlValue::Text(now.clone()),
                    SqlValue::Text(domain.clone()),
                ],
            ).map_err(|e| format!("newsroom: source update: {e}"))?;

            // Sync rank across all domains sharing this root_domain.
            sync_root_rank(&store, &root, &now)
                .map_err(|e| format!("newsroom: sync rank: {e}"))?;
        }

        // ── 6. Sort new_events by source rank DESC ───────────────────────────
        let mut rank_map: HashMap<String, f64> = HashMap::new();
        for domain in domain_tones.keys() {
            if let Ok(Some(row)) = store.query_one(
                "SELECT rank FROM sources WHERE domain = ?1",
                &[SqlValue::Text(domain.clone())],
            ) {
                if let Some(SqlValue::Real(r)) = row.get("rank") {
                    rank_map.insert(domain.clone(), *r);
                }
            }
        }
        new_events.sort_by(|a, b| {
            let ra = rank_map.get(&extract_domain(&a.source_url)).copied().unwrap_or(0.5);
            let rb = rank_map.get(&extract_domain(&b.source_url)).copied().unwrap_or(0.5);
            rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok::<Vec<GdeltRow>, String>(new_events)
    })
    .await
    .unwrap_or_else(|e| Err(format!("newsroom: spawn_blocking: {e}")));

    let new_events = match insert_result {
        Ok(v) => v,
        Err(e) => {
            let _ = reply_tx.send(Err(BusError::new(-32000, e)));
            return;
        }
    };

    // ── 7. Nothing new — check if we have a summary; if not, regenerate from stored events ──
    let new_events = if new_events.is_empty() {
        // If a summary already exists, nothing to do.
        let has_summary = tokio::task::spawn_blocking({
            let state2 = state.clone();
            move || {
                state2
                    .open_sqlite_store("newsroom", "events")
                    .ok()
                    .and_then(|s| {
                        s.query_one("SELECT COUNT(*) AS n FROM summaries", &[])
                            .ok()
                            .flatten()
                    })
                    .and_then(|row| match row.get("n") {
                        Some(SqlValue::Integer(n)) => Some(*n > 0),
                        _ => None,
                    })
                    .unwrap_or(true) // assume summary exists on error → skip LLM
            }
        })
        .await
        .unwrap_or(true);

        if has_summary {
            update_last_fetched(&state).await;
            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: "No new events since last fetch.".to_string(),
                session_id,
                usage: None,
                timing: None,
                thinking: None,
            }));
            return;
        }

        // No summary yet — reload stored events and run the LLM for the first time.
        tokio::task::spawn_blocking({
            let state2 = state.clone();
            move || -> Vec<GdeltRow> {
                state2
                    .open_sqlite_store("newsroom", "events")
                    .ok()
                    .map(|s| load_recent_events(&s, 50))
                    .unwrap_or_default()
            }
        })
        .await
        .unwrap_or_default()
    } else {
        new_events
    };

    if new_events.is_empty() {
        update_last_fetched(&state).await;
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
            channel_id,
            content: "No events stored yet.".to_string(),
            session_id,
            usage: None,
            timing: None,
            thinking: None,
        }));
        return;
    }

    // ── 8. Open agent session for transcript ─────────────────────────────────
    let state_session = state.clone();
    let agent_session = tokio::task::spawn_blocking(move || {
        let memory = state_session.memory.clone();
        match state_session.open_agent_store("newsroom") {
            Err(e) => {
                warn!(error = %e, "newsroom: failed to open agent store");
                None
            }
            Ok(store) => store
                .get_or_create_session(&memory, "newsroom")
                .map_err(|e| {
                    warn!(error = %e, "newsroom: failed to open agent session");
                    e
                })
                .ok(),
        }
    })
    .await
    .unwrap_or(None);

    // ── 9. Build prompt and call LLM ─────────────────────────────────────────
    let skills = state
        .agent_skills
        .get("newsroom")
        .cloned()
        .unwrap_or_default();
    let (system, user_prompt) = build_summary_prompt(&new_events, &skills);

    let llm_result = state
        .complete_via_llm_with_system(&channel_id, &user_prompt, Some(&system))
        .await;

    let (summary, usage, thinking) = match llm_result {
        Ok(BusPayload::CommsMessage {
            content,
            usage,
            thinking,
            ..
        }) => (content, usage, thinking),
        Ok(other) => {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!("newsroom: unexpected LLM reply: {other:?}"),
            )));
            return;
        }
        Err(e) => {
            let _ = reply_tx.send(Err(e));
            return;
        }
    };

    // ── 10. Record transcript + store summary + update last_fetched ──────────
    if let Some(ref session) = agent_session {
        if let Err(e) = session.transcript_append("user", &user_prompt).await {
            warn!(error = %e, "newsroom: failed to append prompt to transcript");
        }
        if let Err(e) = session.transcript_append("assistant", &summary).await {
            warn!(error = %e, "newsroom: failed to append response to transcript");
        }
    }

    let summary_to_store = summary.clone();
    let state_sum = state.clone();
    tokio::task::spawn_blocking(move || {
        let now = now_iso8601();
        match state_sum.open_sqlite_store("newsroom", "events") {
            Err(e) => warn!(error = %e, "newsroom: open store for summary"),
            Ok(store) => {
                if let Err(e) = store.execute(
                    "INSERT INTO summaries (content, created_at) VALUES (?1, ?2)",
                    &[SqlValue::Text(summary_to_store), SqlValue::Text(now)],
                ) {
                    warn!(error = %e, "newsroom: insert summary");
                } else if let Err(e) = store.execute(
                    "DELETE FROM summaries WHERE id NOT IN \
                     (SELECT id FROM summaries ORDER BY id DESC LIMIT ?1)",
                    &[SqlValue::Integer(SUMMARY_CAP)],
                ) {
                    warn!(error = %e, "newsroom: cap summaries");
                }
            }
        }
    })
    .await
    .ok();

    update_last_fetched(&state).await;

    // ── 11. Trigger news aggregator in background ─────────────────────────────
    // Extract URLs from new_events and dispatch to the configured aggregator target.
    // Fire-and-forget: if the aggregator agent is registered it will pick up the
    // URLs and build the knowledge graph.  ERR_METHOD_NOT_FOUND means the plugin
    // is simply not enabled and can be ignored; all other errors indicate a real
    // problem and should be surfaced as warnings.
    {
        let agg_state = state.clone();
        let agg_channel = channel_id.clone();

        // Extract URLs from new_events.
        let new_event_urls: Vec<String> = new_events
            .iter()
            .map(|e| e.source_url.clone())
            .filter(|u| !u.is_empty())
            .collect();

        if !new_event_urls.is_empty() {
            // Resolve the target aggregator from config (default: "news_aggregator").
            let agg_target: String = agg_state
                .agent_aggregation_targets
                .get("newsroom")
                .cloned()
                .unwrap_or_else(|| "news_aggregator".to_string());

            tracing::info!(
                url_count = new_event_urls.len(),
                target = %agg_target,
                "newsroom: triggering aggregator with URLs"
            );

            tokio::spawn(async move {
                let payload = serde_json::json!({
                    "urls": new_event_urls,
                    "source_agent": "newsroom"
                })
                .to_string();

                match agg_state
                    .dispatch_to_agent(&agg_target, "aggregate", &payload, &agg_channel, None)
                    .await
                {
                    Ok(_) => {}
                    Err(e) if e.code == crate::supervisor::bus::ERR_METHOD_NOT_FOUND => {
                        tracing::debug!(target = %agg_target, "newsroom: aggregator plugin not enabled — KG will not be updated");
                    }
                    Err(e) => {
                        error!(
                            error = ?e,
                            target = %agg_target,
                            "newsroom: aggregator dispatch FAILED — knowledge graph will NOT be updated"
                        );
                    }
                }
            });
        }
    }

    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
        channel_id,
        content: summary,
        session_id,
        usage,
        timing: None,
        thinking,
    }));
}

// ── latest ────────────────────────────────────────────────────────────────────

async fn handle_latest(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let content = tokio::task::spawn_blocking(move || {
        let store = state.open_sqlite_store("newsroom", "events").map_err(|e| {
            format!("newsroom: open store: {e}")
        })?;
        ensure_schema(&store)?;

        let row = store
            .query_one(
                "SELECT content FROM summaries ORDER BY id DESC LIMIT 1",
                &[],
            )
            .map_err(|e| format!("newsroom: latest query: {e}"))?;

        Ok::<String, String>(match row {
            Some(r) => match r.get("content") {
                Some(SqlValue::Text(s)) => s.clone(),
                _ => "No summaries stored yet.".to_string(),
            },
            None => "No summaries stored yet.".to_string(),
        })
    })
    .await
    .unwrap_or_else(|e| Err(format!("newsroom: spawn_blocking: {e}")));

    match content {
        Ok(text) => {
            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: text,
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

// ── sources ───────────────────────────────────────────────────────────────────

async fn handle_sources(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let content = tokio::task::spawn_blocking(move || {
        let store = state.open_sqlite_store("newsroom", "events").map_err(|e| {
            format!("newsroom: open store: {e}")
        })?;
        ensure_schema(&store)?;

        let rows = store
            .query_rows(
                "SELECT domain, root_domain, display_name, rank, fetch_count, event_count, avg_tone, last_seen \
                 FROM sources ORDER BY rank DESC LIMIT 50",
                &[],
            )
            .map_err(|e| format!("newsroom: sources query: {e}"))?;

        let mut out = String::from("[");
        for (i, row) in rows.iter().enumerate() {
            if i > 0 { out.push(','); }
            let domain  = match row.get("domain")       { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let root    = match row.get("root_domain")  { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let name    = match row.get("display_name") { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let rank    = match row.get("rank")         { Some(SqlValue::Real(f)) => *f, _ => 0.0 };
            let fc      = match row.get("fetch_count")  { Some(SqlValue::Integer(n)) => *n, _ => 0 };
            let ec      = match row.get("event_count")  { Some(SqlValue::Integer(n)) => *n, _ => 0 };
            let tone    = match row.get("avg_tone")     { Some(SqlValue::Real(f)) => *f, _ => 0.0 };
            let ls      = match row.get("last_seen")    { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            out.push_str(&format!(
                r#"{{"domain":{domain:?},"root_domain":{root:?},"display_name":{name:?},"rank":{rank:.4},"fetch_count":{fc},"event_count":{ec},"avg_tone":{tone:.2},"last_seen":{ls:?}}}"#
            ));
        }
        out.push(']');
        Ok::<String, String>(out)
    })
    .await
    .unwrap_or_else(|e| Err(format!("newsroom: spawn_blocking: {e}")));

    match content {
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

// ── events ────────────────────────────────────────────────────────────────────

async fn handle_events(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
    reply_tx: oneshot::Sender<BusResult>,
) {
    let content = tokio::task::spawn_blocking(move || {
        let store = state.open_sqlite_store("newsroom", "events").map_err(|e| {
            format!("newsroom: open store: {e}")
        })?;
        ensure_schema(&store)?;

        let rows = store
            .query_rows(
                "SELECT date, actor1, actor2, event_code, goldstein, num_articles, avg_tone, source_url \
                 FROM events ORDER BY id DESC LIMIT 50",
                &[],
            )
            .map_err(|e| format!("newsroom: events query: {e}"))?;

        let mut out = String::from("[");
        for (i, row) in rows.iter().enumerate() {
            if i > 0 { out.push(','); }
            let date      = match row.get("date")         { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let actor1    = match row.get("actor1")       { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let actor2    = match row.get("actor2")       { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let code      = match row.get("event_code")   { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let goldstein = match row.get("goldstein")    { Some(SqlValue::Real(f)) => *f, _ => 0.0 };
            let articles  = match row.get("num_articles") { Some(SqlValue::Integer(n)) => *n, _ => 0 };
            let tone      = match row.get("avg_tone")     { Some(SqlValue::Real(f)) => *f, _ => 0.0 };
            let url       = match row.get("source_url")   { Some(SqlValue::Text(s)) => s.as_str(), _ => "" };
            let domain    = extract_domain(url);
            out.push_str(&format!(
                r#"{{"date":{date:?},"actor1":{actor1:?},"actor2":{actor2:?},"event_code":{code:?},"goldstein":{goldstein:.2},"num_articles":{articles},"avg_tone":{tone:.2},"source_url":{url:?},"domain":{domain:?}}}"#
            ));
        }
        out.push(']');
        Ok::<String, String>(out)
    })
    .await
    .unwrap_or_else(|e| Err(format!("newsroom: spawn_blocking: {e}")));

    match content {
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

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn update_last_fetched(state: &Arc<AgentsState>) {
    let state = state.clone();
    let now = now_iso8601();
    tokio::task::spawn_blocking(move || match state.open_agent_store("newsroom") {
        Err(e) => error!(error = %e, "newsroom: failed to open agent store for last_fetched — fetch time will not be recorded"),
        Ok(store) => {
            if let Err(e) = store.kv_set("last_fetched", &now) {
                error!(error = %e, "newsroom: failed to update last_fetched");
            }
        }
    })
    .await
    .ok();
}

/// Aggregate stats across all domains sharing `root_domain` and write the
/// computed rank back to every sibling row.
fn sync_root_rank(
    store: &SqliteStore,
    root: &str,
    now: &str,
) -> Result<(), crate::core::error::AppError> {
    let rows = store.query_rows(
        "SELECT fetch_count, avg_tone, last_seen FROM sources WHERE root_domain = ?1",
        &[SqlValue::Text(root.to_string())],
    )?;

    if rows.is_empty() {
        return Ok(());
    }

    // Aggregate across siblings.
    let total_fetch: i64 = rows.iter().map(|r| match r.get("fetch_count") {
        Some(SqlValue::Integer(n)) => *n,
        _ => 0,
    }).sum();

    let avg_tone: f64 = {
        let sum: f64 = rows.iter().map(|r| match r.get("avg_tone") {
            Some(SqlValue::Real(f)) => *f,
            _ => 0.0,
        }).sum();
        sum / rows.len() as f64
    };

    let most_recent = rows.iter()
        .filter_map(|r| match r.get("last_seen") { Some(SqlValue::Text(s)) => Some(s.as_str()), _ => None })
        .max()
        .unwrap_or(now);

    let rank = compute_rank(total_fetch, avg_tone, most_recent);

    store.execute(
        "UPDATE sources SET rank = ?1 WHERE root_domain = ?2",
        &[SqlValue::Real(rank), SqlValue::Text(root.to_string())],
    )?;

    Ok(())
}

/// Rank score in [0, 1].
/// Load the most recent `limit` events from the store as `GdeltRow`s.
fn load_recent_events(store: &SqliteStore, limit: usize) -> Vec<GdeltRow> {
    store
        .query_rows(
            "SELECT date, actor1, actor2, event_code, goldstein, num_articles, avg_tone, source_url \
             FROM events ORDER BY id DESC LIMIT ?1",
            &[SqlValue::Integer(limit as i64)],
        )
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            Some(GdeltRow {
                date: match row.get("date")? { SqlValue::Text(v) => v.clone(), _ => return None },
                actor1: match row.get("actor1")? { SqlValue::Text(v) => v.clone(), _ => return None },
                actor2: match row.get("actor2")? { SqlValue::Text(v) => v.clone(), _ => return None },
                event_code: match row.get("event_code")? { SqlValue::Text(v) => v.clone(), _ => return None },
                goldstein: match row.get("goldstein")? { SqlValue::Real(v) => *v, _ => return None },
                num_articles: match row.get("num_articles")? { SqlValue::Integer(v) => *v as u64, _ => return None },
                avg_tone: match row.get("avg_tone")? { SqlValue::Real(v) => *v, _ => return None },
                source_url: match row.get("source_url")? { SqlValue::Text(v) => v.clone(), _ => return None },
            })
        })
        .collect()
}

/// - 50% fetch frequency (sigmoid, saturates around fetch_count ≈ 20)
/// - 30% tone score (avg_tone mapped from [-10,+10] to [0,1])
/// - 20% recency (exponential decay, half-life ≈ 21 days)
fn compute_rank(fetch_count: i64, avg_tone: f64, last_seen_iso: &str) -> f64 {
    let fetch_score = 1.0 - 1.0 / (1.0 + fetch_count as f64 * 0.2);
    let tone_score = (avg_tone.clamp(-10.0, 10.0) + 10.0) / 20.0;
    let days = chrono::DateTime::parse_from_rfc3339(last_seen_iso)
        .map(|dt| {
            let diff = chrono::Utc::now().signed_duration_since(dt.with_timezone(&chrono::Utc));
            (diff.num_seconds() as f64 / 86400.0).max(0.0)
        })
        .unwrap_or(0.0);
    let recency_score = (-days / 30.0_f64).exp();
    0.50 * fetch_score + 0.30 * tone_score + 0.20 * recency_score
}

/// Extract the subdomain-aware hostname from a URL, stripping `www.` only.
/// `https://feeds.bbci.co.uk/news` → `feeds.bbci.co.uk`
/// `https://www.reuters.com/article` → `reuters.com`
fn extract_domain(url: &str) -> String {
    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host = host_port.split(':').next().unwrap_or(host_port).to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// Strip one subdomain label to get the registrable root domain, with
/// handling for common second-level TLDs (co.uk, com.au, org.uk, …).
/// `feeds.bbci.co.uk` → `bbci.co.uk`
/// `reuters.com`      → `reuters.com`
fn extract_root_domain(domain: &str) -> String {
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() <= 2 {
        return domain.to_string();
    }
    let tld = parts[parts.len() - 1];
    let sld = parts[parts.len() - 2];
    // Pattern: <name>.co.uk, <name>.com.au, etc. — keep last 3 labels.
    if tld.len() == 2 && matches!(sld, "co" | "com" | "org" | "net" | "gov" | "edu" | "ac") {
        return parts[parts.len().saturating_sub(3)..].join(".");
    }
    parts[parts.len() - 2..].join(".")
}

fn build_summary_prompt(events: &[GdeltRow], tools: &[String]) -> (String, String) {
    let mut items_str = String::new();
    for (i, ev) in events.iter().enumerate() {
        items_str.push_str(&format!(
            "[{}] {} ↔ {}  code={} importance={} articles={} tone={}\n    {}\n",
            i + 1,
            ev.actor1,
            ev.actor2,
            ev.event_code,
            ev.goldstein,
            ev.num_articles,
            ev.avg_tone,
            ev.source_url,
        ));
    }

    let system = super::core::prompt::preamble("config/prompts", tools).build();
    let body = std::fs::read_to_string("config/prompts/newsroom_summary.txt")
        .unwrap_or_else(|_| {
            "Summarize the following newly detected GDELT events:\n\n{{items}}".to_string()
        });
    let user = PromptBuilder::new("config/prompts")
        .append(body)
        .var("items", &items_str)
        .build();

    (system, user)
}

// ── Raw event row ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GdeltRow {
    date: String,
    actor1: String,
    actor2: String,
    event_code: String,
    goldstein: f64,
    num_articles: u64,
    avg_tone: f64,
    source_url: String,
}

fn parse_gdelt_events(json: &str) -> Vec<GdeltRow> {
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(json) else {
        return Vec::new();
    };

    arr.into_iter()
        .filter_map(|v| {
            let obj = v.as_object()?;
            let str_field = |key: &str| -> String {
                match obj.get(key) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                }
            };
            let f64_field = |key: &str| -> f64 {
                obj.get(key)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
            };
            let u64_field = |key: &str| -> u64 {
                obj.get(key)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            };
            Some(GdeltRow {
                date: str_field("date"),
                actor1: str_field("actor1"),
                actor2: str_field("actor2"),
                event_code: str_field("event_code"),
                goldstein: f64_field("goldstein"),
                num_articles: u64_field("num_articles"),
                avg_tone: f64_field("avg_tone"),
                source_url: str_field("source_url"),
            })
        })
        .collect()
}
