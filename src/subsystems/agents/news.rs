use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::warn;

use crate::subsystems::memory::stores::agent::TextItem;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

use super::{Agent, AgentsState};

const NO_NEWS_MSG: &str = "No new news emails.";

pub(crate) struct NewsAgentPlugin;

impl Agent for NewsAgentPlugin {
    fn id(&self) -> &str { "news" }

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
                    content: "news component: active".to_string(),
                    session_id,
                    usage: None,
                }));
                return;
            }

            let tool_action = match action.as_str() {
                "handle" | "read" => "get",
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_METHOD_NOT_FOUND,
                        format!("unknown news action: {action}"),
                    )));
                    return;
                }
            };

            // ── 1. Fetch from tool ──────────────────────────────────────
            let result = state
                .execute_tool(
                    "newsmail_aggregator",
                    tool_action,
                    state.news_query_args_json.clone(),
                    &channel_id,
                    session_id.clone(),
                )
                .await;

            let raw_json = match result {
                Ok(BusPayload::ToolResponse { ok: true, data_json: Some(data), .. }) => data,
                Ok(BusPayload::ToolResponse { ok: true, data_json: None, .. }) => "[]".to_string(),
                Ok(BusPayload::ToolResponse { ok: false, error, .. }) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!(
                            "newsmail_aggregator tool error: {}",
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
                match state_store.open_agent_store("news") {
                    Err(e) => {
                        warn!(error = %e, "news: failed to open agent store");
                        (None, None)
                    }
                    Ok(store) => {
                        if let Err(e) = store.write_raw(&raw_filename, &raw_json_clone) {
                            warn!(error = %e, "news: failed to write raw fetch");
                        }
                        if let Err(e) = store.texts_replace_all(items_clone) {
                            warn!(error = %e, "news: failed to persist texts");
                        }
                        let cached = store.kv_get(&cache_key_clone).unwrap_or(None);
                        let session = store.get_or_create_session(&memory, "news").map_err(|e| {
                            warn!(error = %e, "news: failed to open agent session");
                            e
                        }).ok();
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
                }));
                return;
            }

            // ── 5. Empty inbox — no LLM needed ─────────────────────────
            if items.is_empty() {
                persist_summary(&state, &cache_key, NO_NEWS_MSG).await;
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: NO_NEWS_MSG.to_string(),
                    session_id,
                    usage: None,
                }));
                return;
            }

            // ── 6. Ask LLM to summarise ─────────────────────────────────
            let prompt = build_summary_prompt(&items);
            let llm_result = state.complete_via_llm(&channel_id, &prompt).await;

            let (summary, usage) = match llm_result {
                Ok(BusPayload::CommsMessage { content, usage, .. }) => (content, usage),
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

            // ── 7. Record comm transcript in agent session ──────────────
            if let Some(ref session) = agent_session {
                if let Err(e) = session.transcript_append("agent", &prompt).await {
                    warn!(error = %e, "news: failed to append prompt to transcript");
                }
                if let Err(e) = session.transcript_append("llm", &summary).await {
                    warn!(error = %e, "news: failed to append response to transcript");
                }
            }

            // ── 8. Cache the summary + record fetch time ────────────────
            persist_summary(&state, &cache_key, &summary).await;

            let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                channel_id,
                content: summary,
                session_id,
                usage,
            }));
        });
    }
}

/// Compute a 16-char hex content hash using [`DefaultHasher`].
///
/// Not cryptographic — used only as a stable cache/filename key.
fn content_hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Write summary + `last_fetched` timestamp to the agent store.
/// Failures are logged but do not propagate.
async fn persist_summary(state: &Arc<AgentsState>, cache_key: &str, summary: &str) {
    let state = state.clone();
    let cache_key = cache_key.to_string();
    let summary = summary.to_string();
    let now = chrono::Utc::now().to_rfc3339();
    tokio::task::spawn_blocking(move || {
        match state.open_agent_store("news") {
            Err(e) => warn!(error = %e, "news: failed to open agent store for summary cache"),
            Ok(store) => {
                if let Err(e) = store.kv_set(&cache_key, &summary) {
                    warn!(error = %e, "news: failed to cache summary");
                }
                if let Err(e) = store.kv_set("last_fetched", &now) {
                    warn!(error = %e, "news: failed to update last_fetched");
                }
            }
        }
    })
    .await
    .ok();
}

/// Format [`TextItem`]s into an LLM summarisation prompt.
fn build_summary_prompt(items: &[TextItem]) -> String {
    let prompt_path = Path::new("config/prompts/news_summary.txt");
    let template = fs::read_to_string(prompt_path).unwrap_or_else(|_| {
        // fallback to a minimal prompt if file missing
        "Summarize the following news items:\n\n{{items}}".to_string()
    });
    let mut items_str = String::new();
    for (i, item) in items.iter().enumerate() {
        let subject = item.metadata.get("subject").map(|s| s.as_str()).unwrap_or("(no subject)");
        let from    = item.metadata.get("from").map(|s| s.as_str()).unwrap_or("");
        let date    = item.metadata.get("date").map(|s| s.as_str()).unwrap_or("");
        items_str.push_str(&format!("[{}] {}\n    From: {}  Date: {}\n", i + 1, subject, from, date));
    }
    template.replace("{{items}}", &items_str)
}

/// Parse a JSON array from the tool response into [`TextItem`]s.
///
/// Each array element becomes one `TextItem` whose `content` is the
/// element serialised back to a compact JSON string.  String-valued
/// fields (`"subject"`, `"from"`, `"date"`, `"id"`) are also extracted
/// into `metadata` for later filtering or display.
fn parse_tool_items(json: &str) -> Vec<TextItem> {
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(json) else {
        return Vec::new();
    };

    arr.into_iter()
        .map(|v| {
            let mut meta = HashMap::new();
            if let serde_json::Value::Object(ref obj) = v {
                for key in ["subject", "from", "date", "id", "mime"] {
                    if let Some(serde_json::Value::String(s)) = obj.get(key) {
                        meta.insert(key.to_string(), s.clone());
                    }
                }
            }
            TextItem::new(v.to_string(), meta)
        })
        .collect()
}
