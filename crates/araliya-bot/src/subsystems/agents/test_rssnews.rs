//! Test RSS news agent.
//!
//! Fetches a hardcoded set of RSS feeds via the `rss_fetch` tool, formats the
//! items as plain text, and asks the LLM to produce a short news briefing.
//!
//! This agent exists to exercise the `rss_fetch` tool end-to-end.  It is
//! intentionally simple — no caching, no persistence, no KG storage.

use std::sync::Arc;

use tokio::sync::oneshot;
use tracing::warn;

use araliya_core::bus::message::{BusError, BusPayload, BusResult};

use super::{Agent, AgentsState};

const DEFAULT_FEEDS: &[&str] = &[
    "https://feeds.bbci.co.uk/news/rss.xml",
    "https://rss.nytimes.com/services/xml/rss/nyt/HomePage.xml",
    "https://feeds.reuters.com/reuters/topNews",
];

const LOOKBACK_SECS: u64 = 86_400; // 24 hours
const MAX_ITEMS: usize = 30;

const SYSTEM_PROMPT: &str = "\
You are a concise news editor. Below are RSS feed items from multiple sources.\n\
Produce a short briefing (5-10 bullet points) covering the most important stories.\n\
For each bullet: one sentence summary. Group by topic if possible.\n\
Omit duplicates. Be factual and neutral.";

// ── Agent ─────────────────────────────────────────────────────────────────────

pub(crate) struct TestRssNewsAgent;

impl Agent for TestRssNewsAgent {
    fn id(&self) -> &str {
        "test_rssnews"
    }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        _content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            let reply = run(channel_id, session_id, state).await;
            let _ = reply_tx.send(reply);
        });
    }
}

// ── Core logic ────────────────────────────────────────────────────────────────

async fn run(
    channel_id: String,
    session_id: Option<String>,
    state: Arc<AgentsState>,
) -> BusResult {
    // 1. Build tool args
    let urls: Vec<String> = DEFAULT_FEEDS.iter().map(|s| s.to_string()).collect();
    let args_json = serde_json::json!({
        "urls": urls,
        "lookback_secs": LOOKBACK_SECS,
        "max_items": MAX_ITEMS,
    })
    .to_string();

    // 2. Call the rss_fetch tool
    let tool_result = state
        .execute_tool("rss_fetch", "fetch", args_json, &channel_id, session_id)
        .await;

    let data_json = match tool_result {
        Ok(BusPayload::ToolResponse {
            ok: true,
            data_json: Some(data),
            ..
        }) => data,
        Ok(BusPayload::ToolResponse {
            ok: false,
            error,
            ..
        }) => {
            let msg = error.unwrap_or_else(|| "rss_fetch tool error".to_string());
            warn!(error = %msg, "test_rssnews: tool returned error");
            return Err(BusError::new(-32000, msg));
        }
        Ok(_) => {
            warn!("test_rssnews: unexpected tool response type");
            return Err(BusError::new(-32000, "unexpected tool response"));
        }
        Err(e) => {
            warn!(error = ?e, "test_rssnews: execute_tool bus error");
            return Err(e);
        }
    };

    // 3. Deserialise items
    #[derive(serde::Deserialize)]
    struct Item {
        title: String,
        description: Option<String>,
        pub_date: Option<String>,
        source_url: String,
    }

    let items: Vec<Item> = serde_json::from_str(&data_json).unwrap_or_default();

    if items.is_empty() {
        let content = "No RSS items found in the last 24 hours across the configured feeds."
            .to_string();
        return Ok(BusPayload::CommsMessage {
            channel_id,
            content,
            session_id: None,
            usage: None,
            timing: None,
            thinking: None,
        });
    }

    // 4. Format items as plain text for LLM
    let mut user_text = format!("Found {} RSS items:\n\n", items.len());
    for (i, item) in items.iter().enumerate() {
        user_text.push_str(&format!("[{}] {}\n", i + 1, item.title));
        if let Some(desc) = &item.description {
            user_text.push_str(&format!("    {}\n", desc));
        }
        if let Some(date) = &item.pub_date {
            user_text.push_str(&format!("    Published: {}\n", date));
        }
        user_text.push_str(&format!("    Source feed: {}\n\n", item.source_url));
    }

    // 5. Ask the LLM to summarise
    match state
        .complete_via_llm_with_system(&channel_id, &user_text, Some(SYSTEM_PROMPT))
        .await
    {
        Ok(BusPayload::CommsMessage { content, usage, timing, thinking, .. }) => {
            Ok(BusPayload::CommsMessage {
                channel_id,
                content,
                session_id: None,
                usage,
                timing,
                thinking,
            })
        }
        Ok(_) => Err(BusError::new(-32000, "unexpected LLM response type")),
        Err(e) => {
            warn!(error = ?e, "test_rssnews: LLM call failed");
            Err(e)
        }
    }
}
