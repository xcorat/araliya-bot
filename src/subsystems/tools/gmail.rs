// TODO: refactor. Move the setup related code to a seperate file, move gmail to a folder.
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const DEFAULT_CALLBACK_PATH: &str = "/oauth2/callback";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailSummary {
    pub id: String,
    pub thread_id: String,
    pub internal_date_unix: Option<u64>,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

fn token_file() -> PathBuf {
    PathBuf::from("config/gmail_token.json")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn random_verifier() -> String {
    format!("{}{}{}", Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
}

fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(digest)
}

fn build_auth_url(client_id: &str, redirect_uri: &str, state: &str, challenge: &str) -> Result<String, String> {
    let mut url = Url::parse(AUTH_URL).map_err(|e| format!("invalid auth URL: {e}"))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.into())
}

fn open_browser(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

fn write_http_ok(stream: &mut TcpStream, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("failed writing callback response: {e}"))
}

fn receive_auth_code(port: u16, expected_state: &str, expected_path: &str) -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("failed to bind callback server on 127.0.0.1:{port}: {e}"))?;

    let (mut stream, _) = listener
        .accept()
        .map_err(|e| format!("failed accepting callback connection: {e}"))?;

    let mut buf = [0_u8; 8192];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("read callback request failed: {e}"))?;
    if n == 0 {
        return Err("empty callback request".to_string());
    }

    let raw = String::from_utf8(buf[..n].to_vec()).map_err(|e| format!("callback request not utf8: {e}"))?;
    let first = raw
        .lines()
        .next()
        .ok_or_else(|| "callback request missing request line".to_string())?;
    let parts: Vec<&str> = first.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("invalid callback request line".to_string());
    }

    let parsed = Url::parse(&format!("http://127.0.0.1:{port}{}", parts[1]))
        .map_err(|e| format!("failed to parse callback URI: {e}"))?;

    if parsed.path() != expected_path {
        let _ = write_http_ok(
            &mut stream,
            "<html><body><h1>Wrong callback path</h1><p>Return to terminal.</p></body></html>",
        );
        return Err(format!("unexpected callback path: {}", parsed.path()));
    }

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut err: Option<String> = None;

    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.to_string()),
            "state" => state = Some(v.to_string()),
            "error" => err = Some(v.to_string()),
            _ => {}
        }
    }

    if let Some(error) = err {
        let _ = write_http_ok(
            &mut stream,
            "<html><body><h1>Authorization failed</h1><p>Return to terminal.</p></body></html>",
        );
        return Err(format!("oauth error from callback: {error}"));
    }

    if state.as_deref() != Some(expected_state) {
        let _ = write_http_ok(
            &mut stream,
            "<html><body><h1>State mismatch</h1><p>Return to terminal.</p></body></html>",
        );
        return Err("callback state mismatch".to_string());
    }

    let auth_code = code.ok_or_else(|| "callback did not include code".to_string())?;
    let _ = write_http_ok(
        &mut stream,
        "<html><body><h1>OAuth complete</h1><p>You can close this tab and return to terminal.</p></body></html>",
    );
    Ok(auth_code)
}

fn parse_loopback_redirect_uri(uri: &str) -> Result<(u16, String), String> {
    let parsed = Url::parse(uri).map_err(|e| format!("invalid redirect URI: {e}"))?;
    if parsed.scheme() != "http" {
        return Err("redirect URI must use http loopback".to_string());
    }
    let host = parsed.host_str().unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" {
        return Err("redirect URI host must be 127.0.0.1 or localhost".to_string());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "redirect URI must include port".to_string())?;
    let path = if parsed.path().is_empty() {
        "/".to_string()
    } else {
        parsed.path().to_string()
    };
    Ok((port, path))
}

fn load_cache() -> Option<TokenCache> {
    let bytes = fs::read(token_file()).ok()?;
    serde_json::from_slice::<TokenCache>(&bytes).ok()
}

fn save_cache(cache: &TokenCache) -> Result<(), String> {
    let path = token_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed creating token dir: {e}"))?;
    }
    let data = serde_json::to_vec_pretty(cache).map_err(|e| format!("token serialize failed: {e}"))?;
    fs::write(path, data).map_err(|e| format!("token write failed: {e}"))
}

async fn refresh_token(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<TokenResponse, String> {
    let mut form: Vec<(&str, String)> = vec![
        ("client_id", client_id.to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("grant_type", "refresh_token".to_string()),
    ];
    if let Some(secret) = client_secret {
        form.push(("client_secret", secret.to_string()));
    }

    let res = client
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("refresh request failed: {e}"))?;

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("refresh failed: {body}"));
    }

    res.json::<TokenResponse>()
        .await
        .map_err(|e| format!("refresh parse failed: {e}"))
}

async fn exchange_code(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, String> {
    let mut form: Vec<(&str, String)> = vec![
        ("client_id", client_id.to_string()),
        ("code", code.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("grant_type", "authorization_code".to_string()),
    ];
    if let Some(secret) = client_secret {
        form.push(("client_secret", secret.to_string()));
    }

    let res = client
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("token exchange failed: {e}"))?;

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("token exchange failed: {body}"));
    }

    res.json::<TokenResponse>()
        .await
        .map_err(|e| format!("token parse failed: {e}"))
}

async fn ensure_access_token(client: &reqwest::Client) -> Result<String, String> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").map_err(|_| "missing GOOGLE_CLIENT_ID".to_string())?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok();

    if let Some(cache) = load_cache() {
        if cache.expires_at > now_unix() + 60 {
            return Ok(cache.access_token);
        }

        if let Some(refresh) = cache.refresh_token.clone() {
            let refreshed = refresh_token(client, &client_id, client_secret.as_deref(), &refresh).await?;
            let merged = TokenCache {
                access_token: refreshed.access_token,
                refresh_token: Some(refresh),
                expires_at: now_unix() + refreshed.expires_in.unwrap_or(3600),
            };
            save_cache(&merged)?;
            return Ok(merged.access_token);
        }
    }

    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/oauth2/callback".to_string());
    let (port, path) = parse_loopback_redirect_uri(&redirect_uri)?;

    let state = Uuid::new_v4().to_string();
    let verifier = random_verifier();
    let challenge = code_challenge_s256(&verifier);
    let auth_url = build_auth_url(&client_id, &redirect_uri, &state, &challenge)?;

    println!("gmail tool auth required; opening browser");
    println!("authorize URL: {auth_url}");
    open_browser(&auth_url);

    let code = tokio::task::spawn_blocking(move || receive_auth_code(port, &state, &path))
        .await
        .map_err(|e| format!("callback task join error: {e}"))??;

    let token = exchange_code(
        client,
        &client_id,
        client_secret.as_deref(),
        &code,
        &verifier,
        &redirect_uri,
    )
    .await?;

    let cache = TokenCache {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_unix() + token.expires_in.unwrap_or(3600),
    };
    save_cache(&cache)?;
    Ok(cache.access_token)
}

fn header_value(headers: &[Value], key: &str) -> String {
    for item in headers {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("");
        if name.eq_ignore_ascii_case(key) {
            return item
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
    }
    String::new()
}

fn parse_internal_date_unix(msg: &Value) -> Option<u64> {
    let millis = msg
        .get("internalDate")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<u64>().ok())?;
    Some(millis / 1000)
}

async fn fetch_message_summary(
    client: &reqwest::Client,
    access_token: &str,
    msg_id: &str,
) -> Result<GmailSummary, String> {
    let get = client
        .get(format!("{GMAIL_BASE}/messages/{msg_id}"))
        .bearer_auth(access_token)
        .query(&[
            ("format", "metadata"),
            ("metadataHeaders", "Subject"),
            ("metadataHeaders", "From"),
            ("metadataHeaders", "Date"),
        ])
        .send()
        .await
        .map_err(|e| format!("gmail get request failed: {e}"))?;

    if !get.status().is_success() {
        let body = get.text().await.unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("gmail get failed: {body}"));
    }

    let msg: Value = get
        .json()
        .await
        .map_err(|e| format!("gmail get parse failed: {e}"))?;

    let payload_headers = msg
        .get("payload")
        .and_then(|p| p.get("headers"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(GmailSummary {
        id: msg.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        thread_id: msg.get("threadId").and_then(Value::as_str).unwrap_or("").to_string(),
        internal_date_unix: parse_internal_date_unix(&msg),
        from: header_value(&payload_headers, "From"),
        subject: header_value(&payload_headers, "Subject"),
        date: header_value(&payload_headers, "Date"),
        snippet: msg.get("snippet").and_then(Value::as_str).unwrap_or("").to_string(),
    })
}

// ── label resolution ──────────────────────────────────────────────────────────

async fn resolve_label_id_with(
    client: &reqwest::Client,
    access_token: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let resp = client
        .get(format!("{GMAIL_BASE}/labels"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("gmail labels list request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("gmail labels list failed: {body}"));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("gmail labels list parse failed: {e}"))?;

    let id = json
        .get("labels")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter().find_map(|entry| {
                let entry_name = entry.get("name").and_then(Value::as_str)?;
                if entry_name.eq_ignore_ascii_case(name) {
                    entry.get("id").and_then(Value::as_str).map(|s| s.to_string())
                } else {
                    None
                }
            })
        });

    Ok(id)
}

// ── list messages (internal) ──────────────────────────────────────────────────

async fn list_messages_with(
    client: &reqwest::Client,
    access_token: &str,
    filter: &GmailFilter,
    max_results: u32,
) -> Result<Vec<GmailSummary>, String> {
    let bounded_max = max_results.clamp(1, 100);

    let mut params: Vec<(&str, String)> = vec![("maxResults", bounded_max.to_string())];
    for id in &filter.label_ids {
        params.push(("labelIds", id.clone()));
    }
    if let Some(q) = &filter.q {
        if !q.is_empty() {
            params.push(("q", q.clone()));
        }
    }

    let list = client
        .get(format!("{GMAIL_BASE}/messages"))
        .bearer_auth(access_token)
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("gmail list request failed: {e}"))?;

    if !list.status().is_success() {
        let body = list.text().await.unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("gmail list failed: {body}"));
    }

    let list_json: Value = list
        .json()
        .await
        .map_err(|e| format!("gmail list parse failed: {e}"))?;

    let ids: Vec<String> = list_json
        .get("messages")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        items.push(fetch_message_summary(client, access_token, &id).await?);
    }

    Ok(items)
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed building HTTP client: {e}"))
}

// ── public API ─────────────────────────────────────────────────────────────────

/// Filter for `users.messages.list`.
/// `label_ids` and `q` are ANDed when both are present.
#[derive(Debug, Clone, Default)]
pub struct GmailFilter {
    /// Gmail system label IDs ("INBOX", "SENT", …) or user label IDs ("Label_xxx").
    pub label_ids: Vec<String>,
    /// Free-form Gmail search string (same syntax as the search box).
    pub q: Option<String>,
}

/// Resolve a label display name (e.g. "News") to its API ID ("Label_6135459501644760604").
/// Returns `None` if no matching label exists.
pub async fn resolve_label_id(name: &str) -> Result<Option<String>, String> {
    let client = build_http_client()?;
    let access_token = ensure_access_token(&client).await?;
    resolve_label_id_with(&client, &access_token, name).await
}

/// Primary entry point for the newsmail aggregator.
/// Resolves `label_name` to its ID if provided — all with a single HTTP client
/// and one token fetch — then fetches messages via `labelIds[]` + optional `q`.
pub async fn fetch_messages(
    label_ids: &[String],
    label_name: Option<&str>,
    q: Option<&str>,
    max_results: u32,
) -> Result<Vec<GmailSummary>, String> {
    let client = build_http_client()?;
    let access_token = ensure_access_token(&client).await?;

    let mut ids: Vec<String> = label_ids.to_vec();
    if let Some(name) = label_name {
        match resolve_label_id_with(&client, &access_token, name).await? {
            Some(id) => {
                tracing::debug!(label_name = %name, label_id = %id, "gmail: resolved label name");
                ids.push(id);
            }
            None => return Err(format!("gmail label not found: {name}")),
        }
    }

    let filter = GmailFilter {
        label_ids: ids,
        q: q.filter(|s| !s.is_empty()).map(|s| s.to_string()),
    };
    tracing::debug!(label_ids = ?filter.label_ids, q = ?filter.q, "gmail: fetching messages");

    list_messages_with(&client, &access_token, &filter, max_results).await
}

/// Lower-level fetch when label IDs are already known.
pub async fn read_many(filter: GmailFilter, max_results: u32) -> Result<Vec<GmailSummary>, String> {
    let client = build_http_client()?;
    let access_token = ensure_access_token(&client).await?;
    list_messages_with(&client, &access_token, &filter, max_results).await
}

/// Fetch the single most recent message matching `q` from INBOX.
pub async fn read_latest(q: Option<&str>) -> Result<GmailSummary, String> {
    let filter = GmailFilter {
        label_ids: vec!["INBOX".to_string()],
        q: q.filter(|s| !s.is_empty()).map(|s| s.to_string()),
    };
    let mut items = read_many(filter, 1).await?;
    items
        .drain(..)
        .next()
        .ok_or_else(|| "no messages found".to_string())
}
