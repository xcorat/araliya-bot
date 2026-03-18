//! Integration test — replay the news-aggregator article→LLM pipeline step-by-step
//! and print every in/out to stderr for debugging.
//!
//! # Run
//! ```
//! LLM_API_KEY=sk-... cargo test --features plugin-news-aggregator \
//!     --test test_news_agg_pipeline -- --ignored --nocapture
//! ```
//!
//! # Env vars
//! | Var              | Default                                              |
//! |------------------|------------------------------------------------------|
//! | `LLM_API_KEY`    | *(required for LLM step)*                            |
//! | `LLM_BASE_URL`   | `https://api.openai.com/v1/chat/completions`         |
//! | `LLM_MODEL`      | `gpt-5-nano`                                         |
//! | `EVENTS_DB`      | auto-detected from `~/.araliya`                      |
//! | `PIPELINE_LIMIT` | `3` (number of URLs to process)                      |
//! | `SKIP_CURSOR`    | `1` = ignore cursor, replay already-processed URLs   |
//!
//! The test is marked `#[ignore]` so it never runs in CI — always pass `--ignored` explicitly.

use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};

// ── constants mirrored from news_aggregator.rs ────────────────────────────────

const MAX_ARTICLE_CHARS: usize = 4_000;
const FETCH_TIMEOUT_S:   u64   = 15;
const ARTICLE_SYSTEM:    &str  =
    "You are a concise news summarizer. \
     Summarize the given article in 2-3 short paragraphs covering: \
     who is involved, what happened, where, when, and why it matters. \
     Be factual and neutral. Do not include URLs or source attribution.";

// ── helpers (mirrors of private fns in news_aggregator.rs) ───────────────────

fn strip_html(html: &str) -> String {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "head", "nav", "footer", "iframe", "img"])
        .build();
    let text = converter.convert(html).unwrap_or_default();
    let text = regex_strip_data_uris(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn regex_strip_data_uris(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("data:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | ')' | ']'))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

fn find_events_db() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let pattern = format!("{home}/.araliya/bot-pkey*/memory/agents/newsroom-*/sqlite/events.db");
    // Manual glob: expand up to two wildcard levels we care about.
    let araliya = std::path::Path::new(&home).join(".araliya");
    if !araliya.exists() {
        return None;
    }
    for bot_entry in std::fs::read_dir(&araliya).ok()?.flatten() {
        let bot_path = bot_entry.path();
        if !bot_path.file_name()?.to_str()?.starts_with("bot-pkey") {
            continue;
        }
        let mem = bot_path.join("memory").join("agents");
        if !mem.exists() {
            continue;
        }
        for agent_entry in std::fs::read_dir(&mem).ok()?.flatten() {
            let ap = agent_entry.path();
            if !ap.file_name()?.to_str()?.starts_with("newsroom") {
                continue;
            }
            let db = ap.join("sqlite").join("events.db");
            if db.exists() {
                eprintln!("[db] found: {}", db.display());
                return Some(db);
            }
        }
    }
    eprintln!("[db] WARNING: no events.db found under {pattern}");
    None
}

fn load_urls(db_path: &std::path::Path, limit: usize, skip_cursor: bool) -> Vec<(i64, String)> {
    use std::collections::HashMap;
    let conn = rusqlite::Connection::open(db_path).expect("open events.db");

    // Read cursor.
    let cursor: i64 = if skip_cursor {
        0
    } else {
        conn.query_row(
            "SELECT value FROM agg_state WHERE key = 'last_processed_id'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
    };

    let sql = if skip_cursor {
        format!(
            "SELECT id, source_url FROM events ORDER BY id DESC LIMIT {limit}"
        )
    } else {
        format!(
            "SELECT id, source_url FROM events WHERE id > {cursor} ORDER BY id ASC LIMIT {limit}"
        )
    };

    let mut stmt = conn.prepare(&sql).expect("prepare");
    stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })
    .expect("query")
    .filter_map(|r| r.ok())
    .filter(|(_, u)| !u.is_empty())
    .collect()
}

// ── LLM call ─────────────────────────────────────────────────────────────────

async fn call_llm(
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String, String> {
    // gpt-5 family rejects any temperature value other than the default (1).
    let mut body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ],
    });
    if !model.starts_with("gpt-5") {
        body["temperature"] = json!(0.2);
    }

    let resp = client
        .post(base_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let raw = resp.text().await.map_err(|e| format!("read body: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {raw}"));
    }
    Ok(raw)
}

fn extract_content(raw_json: &str) -> String {
    serde_json::from_str::<Value>(raw_json)
        .ok()
        .and_then(|v| {
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "(could not extract content field)".to_string())
}

// ── test ─────────────────────────────────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn debug_news_agg_pipeline() {
    let api_key  = std::env::var("LLM_API_KEY").unwrap_or_default();
    let base_url = std::env::var("LLM_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
    let model = std::env::var("LLM_MODEL")
        .unwrap_or_else(|_| "gpt-5-nano".to_string());
    let limit: usize = std::env::var("PIPELINE_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let skip_cursor = std::env::var("SKIP_CURSOR")
        .map(|v| v == "1")
        .unwrap_or(true); // default: re-process recent URLs regardless of cursor

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("  NEWS AGGREGATOR PIPELINE DEBUG  (limit={limit}, skip_cursor={skip_cursor})");
    eprintln!("{}", "═".repeat(80));
    eprintln!("  LLM base_url : {base_url}");
    eprintln!("  LLM model    : {model}");
    if api_key.is_empty() {
        eprintln!("  LLM_API_KEY  : NOT SET — LLM step will be skipped");
    } else {
        let masked = format!("{}***", &api_key[..api_key.len().min(8)]);
        eprintln!("  LLM_API_KEY  : {masked}");
    }
    eprintln!("{}", "─".repeat(80));

    // ── 1. collect URLs ───────────────────────────────────────────────────────
    let db_path_env = std::env::var("EVENTS_DB").ok();
    let urls: Vec<(i64, String)> = if let Some(p) = db_path_env {
        let p = std::path::Path::new(&p);
        eprintln!("[db] using EVENTS_DB={}", p.display());
        load_urls(p, limit, skip_cursor)
    } else if let Some(p) = find_events_db() {
        load_urls(&p, limit, skip_cursor)
    } else {
        eprintln!("[db] no DB found — using hardcoded fallback URLs");
        vec![
            (0, "https://www.bbc.com/news".to_string()),
            (1, "https://apnews.com".to_string()),
        ]
    };

    if urls.is_empty() {
        eprintln!("\n[!] No URLs to process (cursor may have consumed all events).");
        eprintln!("    Set SKIP_CURSOR=1 to replay already-processed URLs.");
        return;
    }

    eprintln!("[db] {} URL(s) to process\n", urls.len());

    // ── 2. build HTTP client ──────────────────────────────────────────────────
    let client = Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_S))
        .user_agent("Mozilla/5.0 (compatible; AraliyaBot/1.0; +https://github.com/)")
        .build()
        .expect("build reqwest client");

    let mut ok_count    = 0usize;
    let mut skip_count  = 0usize;

    for (event_id, url) in &urls {
        eprintln!("{}", "─".repeat(80));
        eprintln!("EVENT id={event_id}");
        eprintln!("URL  : {url}");

        // ◆ Step 1 — Fetch ────────────────────────────────────────────────────
        eprintln!("\n◆ STEP 1 — FETCH");
        let html = match client.get(url.as_str()).send().await {
            Ok(resp) => {
                let status = resp.status();
                eprintln!("  status : {status}");
                if !status.is_success() {
                    eprintln!("  result : ✗ non-2xx, skipping");
                    skip_count += 1;
                    continue;
                }
                match resp.text().await {
                    Ok(t) => {
                        eprintln!("  body   : {} bytes", t.len());
                        t
                    }
                    Err(e) => {
                        eprintln!("  result : ✗ read body error: {e}");
                        skip_count += 1;
                        continue;
                    }
                }
            }
            Err(e) => {
                eprintln!("  result : ✗ fetch error: {e}");
                skip_count += 1;
                continue;
            }
        };

        // ◆ Step 2 — Strip HTML ───────────────────────────────────────────────
        eprintln!("\n◆ STEP 2 — STRIP HTML");
        let stripped = strip_html(&html);
        let char_count = stripped.chars().count();
        eprintln!("  chars  : {char_count}");
        let preview: String = stripped.chars().take(500).collect();
        eprintln!("  first 500 chars:\n  ┌───\n  │ {}\n  └───", preview.replace('\n', "\n  │ "));

        if stripped.trim().is_empty() {
            eprintln!("  result : ✗ empty after strip, skipping");
            skip_count += 1;
            continue;
        }

        // ◆ Step 3 — Truncate ─────────────────────────────────────────────────
        eprintln!("\n◆ STEP 3 — TRUNCATE (max={MAX_ARTICLE_CHARS})");
        let truncated = truncate_chars(&stripped, MAX_ARTICLE_CHARS);
        let truncated_chars = truncated.chars().count();
        if truncated_chars < char_count {
            eprintln!("  truncated: {char_count} → {truncated_chars} chars");
        } else {
            eprintln!("  no truncation needed ({truncated_chars} chars)");
        }

        // ◆ Step 4 — Prompt ───────────────────────────────────────────────────
        let prompt = format!("Article URL: {url}\n\nArticle text:\n{truncated}");
        eprintln!("\n◆ STEP 4 — LLM PROMPT");
        eprintln!("  system prompt:\n  ┌───\n  │ {ARTICLE_SYSTEM}\n  └───");
        eprintln!(
            "  user prompt ({} chars):\n  ┌───\n  │ {}\n  └───",
            prompt.len(),
            prompt.replace('\n', "\n  │ ")
        );

        // ◆ Step 5 — LLM call ─────────────────────────────────────────────────
        eprintln!("\n◆ STEP 5 — LLM CALL");
        if api_key.is_empty() {
            eprintln!("  skipped (LLM_API_KEY not set)");
        } else {
            match call_llm(&client, &api_key, &base_url, &model, ARTICLE_SYSTEM, &prompt).await {
                Ok(raw) => {
                    eprintln!("  raw response JSON:\n  ┌───");
                    for line in raw.lines() {
                        eprintln!("  │ {line}");
                    }
                    eprintln!("  └───");
                    let content = extract_content(&raw);
                    eprintln!("\n  extracted content:\n  ┌───\n  │ {}\n  └───",
                        content.replace('\n', "\n  │ "));
                    ok_count += 1;
                }
                Err(e) => {
                    eprintln!("  ✗ LLM error: {e}");
                    skip_count += 1;
                    continue;
                }
            }
        }

        eprintln!();
    }

    eprintln!("{}", "═".repeat(80));
    eprintln!("  DONE  ok={ok_count}  skipped={skip_count}  total={}", urls.len());
    eprintln!("{}", "═".repeat(80));
}
