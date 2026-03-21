//! GDELT News agent — fetches recent global events from BigQuery and
//! summarises them via LLM, following the same caching pattern as the
//! `news` agent.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::warn;

use crate::subsystems::memory::stores::agent::TextItem;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

use super::core::prompt::PromptBuilder;
use super::{Agent, AgentsState};

const NO_EVENTS_MSG: &str = "No GDELT events found.";

pub(crate) struct GdeltNewsAgent;

impl Agent for GdeltNewsAgent {
    fn id(&self) -> &str {
        "gdelt_news"
    }

    fn handle(
        &self,
        action: String,
        channel_id: String,
        _content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            if action == "health" {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: "gdelt_news component: active".to_string(),
                    session_id,
                    usage: None,
                    timing: None,
                    thinking: None,
                }));
                return;
            }

            match action.as_str() {
                "handle" | "read" => {}
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_METHOD_NOT_FOUND,
                        format!("unknown gdelt_news action: {action}"),
                    )));
                    return;
                }
            }

            // ── 1. Fetch from tool ──────────────────────────────────────
            let result = state
                .execute_tool(
                    "gdelt_bigquery",
                    "fetch",
                    state.gdelt_query_args_json.clone(),
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
                            "gdelt_bigquery tool error: {}",
                            error.unwrap_or_else(|| "unknown".to_string())
                        ),
                    )));
                    return;
                }
                Ok(other) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!("unexpected tools reply: {other:?}"),
                    )));
                    return;
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                    return;
                }
            };

            // ── 2. Content hash — stable key for cache + raw file name ──
            let hash = content_hash(&raw_json);
            let raw_filename = format!("{hash}.json");
            let cache_key = format!("summary:{hash}");

            // ── 3. Persist raw fetch + check cache + open agent session ──
            let items = parse_tool_items(&raw_json);
            let state_store = state.clone();
            let raw_json_clone = raw_json.clone();
            let items_clone = items.clone();
            let cache_key_clone = cache_key.clone();
            let (cached, agent_session) = tokio::task::spawn_blocking(move || {
                let memory = state_store.memory.clone();
                match state_store.open_agent_store("gdelt_news") {
                    Err(e) => {
                        warn!(error = %e, "gdelt_news: failed to open agent store");
                        (None, None)
                    }
                    Ok(store) => {
                        if let Err(e) = store.write_raw(&raw_filename, &raw_json_clone) {
                            warn!(error = %e, "gdelt_news: failed to write raw fetch");
                        }
                        if let Err(e) = store.texts_replace_all(items_clone) {
                            warn!(error = %e, "gdelt_news: failed to persist texts");
                        }
                        let cached = store.kv_get(&cache_key_clone).unwrap_or(None);
                        let session = store
                            .get_or_create_session(&memory, "gdelt_news")
                            .map_err(|e| {
                                warn!(error = %e, "gdelt_news: failed to open agent session");
                                e
                            })
                            .ok();
                        (cached, session)
                    }
                }
            })
            .await
            .unwrap_or((None, None));

            // ── 4. Return cached summary if available ───────────────────
            if let Some(summary) = cached {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: summary,
                    session_id,
                    usage: None,
                    timing: None,
                    thinking: None,
                }));
                return;
            }

            // ── 5. Empty result — no LLM needed ────────────────────────
            if items.is_empty() {
                persist_summary(&state, &cache_key, NO_EVENTS_MSG).await;
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: NO_EVENTS_MSG.to_string(),
                    session_id,
                    usage: None,
                    timing: None,
                    thinking: None,
                }));
                return;
            }

            // ── 6. Ask LLM to summarise ─────────────────────────────────
            let skills = state
                .agent_skills
                .get("gdelt_news")
                .cloned()
                .unwrap_or_default();
            let (system, user_prompt) = build_summary_prompt(&items, &skills, &state.agents_dir);
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
                        format!("unexpected LLM reply: {other:?}"),
                    )));
                    return;
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                    return;
                }
            };

            // ── 7. Record transcript in agent session ───────────────────
            if let Some(ref session) = agent_session {
                if let Err(e) = session.transcript_append("user", &user_prompt).await {
                    warn!(error = %e, "gdelt_news: failed to append prompt to transcript");
                }
                if let Err(e) = session.transcript_append("assistant", &summary).await {
                    warn!(error = %e, "gdelt_news: failed to append response to transcript");
                }
            }

            // ── 8. Cache the summary ────────────────────────────────────
            persist_summary(&state, &cache_key, &summary).await;

            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: summary,
                session_id,
                usage,
                timing: None,
                thinking,
            }));
        });
    }
}

fn content_hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

async fn persist_summary(state: &Arc<AgentsState>, cache_key: &str, summary: &str) {
    let state = state.clone();
    let cache_key = cache_key.to_string();
    let summary = summary.to_string();
    let now = chrono::Utc::now().to_rfc3339();
    tokio::task::spawn_blocking(move || match state.open_agent_store("gdelt_news") {
        Err(e) => warn!(error = %e, "gdelt_news: failed to open agent store for summary cache"),
        Ok(store) => {
            if let Err(e) = store.kv_set(&cache_key, &summary) {
                warn!(error = %e, "gdelt_news: failed to cache summary");
            }
            if let Err(e) = store.kv_set("last_fetched", &now) {
                warn!(error = %e, "gdelt_news: failed to update last_fetched");
            }
        }
    })
    .await
    .ok();
}

fn build_summary_prompt(items: &[TextItem], tools: &[String], agents_dir: &str) -> (String, String) {
    let mut items_str = String::new();
    for (i, item) in items.iter().enumerate() {
        let actor1 = item.metadata.get("actor1").map(|s| s.as_str()).unwrap_or("");
        let actor2 = item.metadata.get("actor2").map(|s| s.as_str()).unwrap_or("");
        let event_code = item
            .metadata
            .get("event_code")
            .map(|s| s.as_str())
            .unwrap_or("");
        let goldstein = item
            .metadata
            .get("goldstein")
            .map(|s| s.as_str())
            .unwrap_or("0");
        let num_articles = item
            .metadata
            .get("num_articles")
            .map(|s| s.as_str())
            .unwrap_or("0");
        let avg_tone = item
            .metadata
            .get("avg_tone")
            .map(|s| s.as_str())
            .unwrap_or("0");
        let source_url = item
            .metadata
            .get("source_url")
            .map(|s| s.as_str())
            .unwrap_or("");
        items_str.push_str(&format!(
            "[{}] {} ↔ {}  code={} importance={} articles={} tone={}\n    {}\n",
            i + 1,
            actor1,
            actor2,
            event_code,
            goldstein,
            num_articles,
            avg_tone,
            source_url,
        ));
    }

    let system = super::core::prompt::preamble(agents_dir, tools).build();

    let agents_path = std::path::Path::new(agents_dir);
    let body = std::fs::read_to_string(agents_path.join("gdelt_news").join("summary.md"))
        .unwrap_or_else(|_| "Summarize the following GDELT global news events:\n\n{{items}}".to_string());
    let user = PromptBuilder::new(agents_path.join("_shared"))
        .append(body)
        .var("items", &items_str)
        .build();

    (system, user)
}

/// Parse raw JSON array of GdeltEvent objects into TextItems.
fn parse_tool_items(json: &str) -> Vec<TextItem> {
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(json) else {
        return Vec::new();
    };

    arr.into_iter()
        .map(|v| {
            let mut meta = HashMap::new();
            if let serde_json::Value::Object(ref obj) = v {
                for key in [
                    "date",
                    "actor1",
                    "actor2",
                    "event_code",
                    "goldstein",
                    "num_articles",
                    "avg_tone",
                    "source_url",
                ] {
                    if let Some(val) = obj.get(key) {
                        let s = match val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        meta.insert(key.to_string(), s);
                    }
                }
            }
            TextItem::new(v.to_string(), meta)
        })
        .collect()
}
