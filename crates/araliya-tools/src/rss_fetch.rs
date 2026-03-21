//! RSS / Atom feed fetch tool.
//!
//! Fetches one or more feed URLs, parses them with `feed-rs`, applies an
//! optional lookback window, and returns a flat list of [`RssItem`]s sorted
//! newest-first.
//!
//! ## Tool call
//!
//! ```json
//! { "urls": ["https://example.com/feed.rss"],
//!   "lookback_secs": 86400,
//!   "max_items": 50 }
//! ```

use std::io::Cursor;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const FETCH_TIMEOUT_S: u64 = 15;
const DEFAULT_MAX_ITEMS: usize = 100;
const DESCRIPTION_MAX_CHARS: usize = 500;

// ── Request args ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct RssFetchArgs {
    /// Feed URLs to fetch (RSS or Atom).
    #[serde(default)]
    pub urls: Vec<String>,
    /// Only return items published within this many seconds of now.
    /// `None` (or omitted) means no time filter.
    pub lookback_secs: Option<u64>,
    /// Maximum number of items to return across all feeds.
    /// Defaults to 100.
    pub max_items: Option<usize>,
}

// ── Response item ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RssItem {
    pub title: String,
    pub link: Option<String>,
    pub description: Option<String>,
    /// ISO-8601 UTC string, e.g. `"2024-06-01T12:00:00Z"`.
    pub pub_date: Option<String>,
    /// The feed URL this item came from.
    pub source_url: String,
}

// ── Implementation ────────────────────────────────────────────────────────────

/// Fetch and parse all feeds; apply time filter and item cap.
pub async fn fetch(args: RssFetchArgs) -> Result<Vec<RssItem>, String> {
    if args.urls.is_empty() {
        return Ok(vec![]);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_S))
        .user_agent("Mozilla/5.0 (compatible; AraliyaBot/1.0)")
        .build()
        .map_err(|e| format!("rss_fetch: build http client: {e}"))?;

    let cutoff: Option<DateTime<Utc>> = args.lookback_secs.map(|secs| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff_ts = now.saturating_sub(secs) as i64;
        DateTime::from_timestamp(cutoff_ts, 0).unwrap_or(DateTime::<Utc>::MIN_UTC)
    });

    let max_items = args.max_items.unwrap_or(DEFAULT_MAX_ITEMS);
    let mut all: Vec<(Option<DateTime<Utc>>, RssItem)> = Vec::new();

    for url in &args.urls {
        debug!(url = %url, "rss_fetch: fetching");
        let bytes = match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    warn!(url = %url, error = %e, "rss_fetch: read body");
                    continue;
                }
            },
            Ok(resp) => {
                warn!(url = %url, status = %resp.status(), "rss_fetch: non-2xx");
                continue;
            }
            Err(e) => {
                warn!(url = %url, error = %e, "rss_fetch: request");
                continue;
            }
        };

        let feed = match feed_rs::parser::parse(Cursor::new(bytes.as_ref())) {
            Ok(f) => f,
            Err(e) => {
                warn!(url = %url, error = %e, "rss_fetch: parse");
                continue;
            }
        };

        for entry in feed.entries {
            let pub_dt: Option<DateTime<Utc>> = entry.published.or(entry.updated);

            // Apply lookback filter
            if let (Some(cutoff_dt), Some(dt)) = (cutoff, pub_dt) {
                if dt < cutoff_dt {
                    continue;
                }
            }

            let title = entry
                .title
                .map(|t| t.content.trim().to_string())
                .unwrap_or_else(|| "(no title)".to_string());

            let link = entry.links.into_iter().next().map(|l| l.href);

            let description = entry
                .summary
                .map(|t| t.content)
                .or_else(|| entry.content.and_then(|c| c.body))
                .map(|s| truncate_chars(strip_basic_html(&s), DESCRIPTION_MAX_CHARS));

            let pub_date = pub_dt.map(|dt| dt.to_rfc3339());

            all.push((
                pub_dt,
                RssItem {
                    title,
                    link,
                    description,
                    pub_date,
                    source_url: url.clone(),
                },
            ));
        }
    }

    // Sort newest first, then cap
    all.sort_by(|a, b| b.0.cmp(&a.0));
    let items: Vec<RssItem> = all.into_iter().take(max_items).map(|(_, item)| item).collect();

    debug!(count = items.len(), "rss_fetch: done");
    Ok(items)
}

/// Return item count from a single well-known feed, or an error string.
pub async fn healthcheck() -> Result<String, String> {
    let args = RssFetchArgs {
        urls: vec!["https://feeds.bbci.co.uk/news/rss.xml".to_string()],
        lookback_secs: None,
        max_items: Some(5),
    };
    match fetch(args).await {
        Ok(items) => Ok(format!("rss_fetch ok — BBC feed returned {} items", items.len())),
        Err(e) => Err(e),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Very lightweight HTML tag stripper — removes `<...>` sequences.
fn strip_basic_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    // Collapse whitespace
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}
