// TODO: move this to a folder for tests/setup section, and decide what to do in Cargo.toml
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use serde_json::Value;
use uuid::Uuid;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const DEFAULT_CALLBACK_PATH: &str = "/oauth2/callback";

#[derive(Debug, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: u64,
    scope: Option<String>,
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    error_description: Option<String>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn token_file() -> PathBuf {
    PathBuf::from("config/gmail_token.json")
}

fn random_verifier() -> String {
    format!("{}{}{}", Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
}

fn open_browser(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

fn read_http_request(stream: &mut TcpStream) -> Result<String, String> {
    let mut buf = [0_u8; 8192];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("read callback request failed: {e}"))?;
    if n == 0 {
        return Err("empty callback request".to_string());
    }
    String::from_utf8(buf[..n].to_vec()).map_err(|e| format!("callback request was not utf8: {e}"))
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

fn parse_request_target(raw: &str) -> Result<String, String> {
    let first = raw
        .lines()
        .next()
        .ok_or_else(|| "callback request missing request line".to_string())?;
    let parts: Vec<&str> = first.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("invalid callback request line".to_string());
    }
    Ok(parts[1].to_string())
}

fn receive_auth_code(port: u16, expected_state: &str, expected_path: &str) -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("failed to bind callback server on 127.0.0.1:{port}: {e}"))?;
    println!("Listening callback at http://127.0.0.1:{port}{expected_path}");

    let (mut stream, _) = listener
        .accept()
        .map_err(|e| format!("failed accepting callback connection: {e}"))?;

    let raw = read_http_request(&mut stream)?;
    let target = parse_request_target(&raw)?;
    let parsed = Url::parse(&format!("http://127.0.0.1:{port}{target}"))
        .map_err(|e| format!("failed to parse callback URI: {e}"))?;

    if parsed.path() != expected_path {
        let _ = write_http_ok(
            &mut stream,
            "<html><body><h1>Wrong callback path</h1><p>Return to terminal.</p></body></html>",
        );
        return Err(format!("received callback on unexpected path: {}", parsed.path()));
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
        .map_err(|e| format!("token exchange request failed: {e}"))?;

    if !res.status().is_success() {
        let body = res
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        return Err(format!("token exchange failed: {body}"));
    }

    res.json::<TokenResponse>()
        .await
        .map_err(|e| format!("token exchange response parse failed: {e}"))
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
        let body = res
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
            return Err(format!(
                "refresh failed: {} ({})",
                err.error,
                err.error_description.unwrap_or_else(|| "no description".to_string())
            ));
        }
        return Err(format!("refresh failed: {body}"));
    }

    res.json::<TokenResponse>()
        .await
        .map_err(|e| format!("refresh response parse failed: {e}"))
}

fn load_token_cache() -> Option<TokenCache> {
    let path = token_file();
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<TokenCache>(&bytes).ok()
}

fn save_token_cache(cache: &TokenCache) -> Result<(), String> {
    let path = token_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed creating token dir: {e}"))?;
    }
    let text = serde_json::to_string_pretty(cache)
        .map_err(|e| format!("failed serializing token cache: {e}"))?;
    fs::write(path, text).map_err(|e| format!("failed writing token cache: {e}"))
}

fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(digest)
}

fn parse_loopback_redirect_uri(uri: &str) -> Result<(u16, String), String> {
    let parsed = Url::parse(uri).map_err(|e| format!("invalid --redirect-uri: {e}"))?;
    if parsed.scheme() != "http" {
        return Err("redirect URI must use http for loopback desktop flow".to_string());
    }
    let host = parsed.host_str().unwrap_or_default();
    if host != "127.0.0.1" && host != "localhost" {
        return Err("redirect URI host must be 127.0.0.1 or localhost".to_string());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "redirect URI must include a port".to_string())?;
    let path = if parsed.path().is_empty() {
        "/".to_string()
    } else {
        parsed.path().to_string()
    };
    Ok((port, path))
}

fn build_auth_url(client_id: &str, redirect_uri: &str, state: &str, code_challenge: &str) -> Result<String, String> {
    let mut url = Url::parse(AUTH_URL).map_err(|e| format!("invalid auth URL: {e}"))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.into())
}

async fn ensure_access_token(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    force_auth: bool,
    fixed_port: Option<u16>,
    explicit_redirect_uri: Option<&str>,
) -> Result<String, String> {
    if !force_auth {
        if let Some(cache) = load_token_cache() {
            let now = now_unix();
            if cache.expires_at > now + 60 {
                return Ok(cache.access_token);
            }

            if let Some(refresh) = cache.refresh_token.clone() {
                let refreshed = refresh_token(client, client_id, client_secret, &refresh).await?;
                let merged = TokenCache {
                    access_token: refreshed.access_token,
                    refresh_token: Some(refresh),
                    expires_at: now_unix() + refreshed.expires_in.unwrap_or(3600),
                    scope: refreshed.scope,
                    token_type: refreshed.token_type,
                };
                save_token_cache(&merged)?;
                return Ok(merged.access_token);
            }
        }
    }

    let (actual_port, callback_path, redirect_uri) = if let Some(uri) = explicit_redirect_uri {
        let (p, path) = parse_loopback_redirect_uri(uri)?;
        (p, path, uri.to_string())
    } else {
        let port = fixed_port.unwrap_or(0);
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("failed to reserve callback port: {e}"))?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("failed reading callback local addr: {e}"))?
            .port();
        drop(listener);

        (
            actual_port,
            DEFAULT_CALLBACK_PATH.to_string(),
            format!("http://127.0.0.1:{actual_port}{DEFAULT_CALLBACK_PATH}"),
        )
    };
    let state = Uuid::new_v4().to_string();
    let code_verifier = random_verifier();
    let code_challenge = code_challenge_s256(&code_verifier);
    let auth_url = build_auth_url(client_id, &redirect_uri, &state, &code_challenge)?;

    println!("\n=== Gmail OAuth (Rust) ===");
    println!("Set this callback URI in Google OAuth Desktop client:");
    println!("  {redirect_uri}");
    println!("\nOpen this URL if browser does not auto-open:");
    println!("{auth_url}\n");
    open_browser(&auth_url);

    let code = receive_auth_code(actual_port, &state, &callback_path)?;
    let token = exchange_code(
        client,
        client_id,
        client_secret,
        &code,
        &code_verifier,
        &redirect_uri,
    )
    .await?;

    let cache = TokenCache {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now_unix() + token.expires_in.unwrap_or(3600),
        scope: token.scope,
        token_type: token.token_type,
    };

    save_token_cache(&cache)?;
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

async fn read_one_email(client: &reqwest::Client, access_token: &str, query: &str) -> Result<(), String> {
    let list = client
        .get(format!("{GMAIL_BASE}/messages"))
        .bearer_auth(access_token)
        .query(&[("maxResults", "1"), ("q", query)])
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

    let msg_id = list_json
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let Some(msg_id) = msg_id else {
        println!("No messages found for query: {query}");
        return Ok(());
    };

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

    println!("\n=== Read One Message ===");
    println!("id: {}", msg.get("id").and_then(Value::as_str).unwrap_or(""));
    println!(
        "threadId: {}",
        msg.get("threadId").and_then(Value::as_str).unwrap_or("")
    );
    println!("from: {}", header_value(&payload_headers, "From"));
    println!("subject: {}", header_value(&payload_headers, "Subject"));
    println!("date: {}", header_value(&payload_headers, "Date"));
    println!(
        "snippet: {}",
        msg.get("snippet").and_then(Value::as_str).unwrap_or("")
    );

    Ok(())
}

fn usage() {
    println!("gmail_read_one options:");
    println!("  --force-auth          ignore cached token and run OAuth again");
    println!("  --query <gmail-q>     Gmail search query (default: in:inbox)");
    println!("  --port <n>            fixed callback port for preconfigured redirect URI");
    println!("  --self-test           run local test only");
    println!("  --redirect-uri <uri>  exact redirect URI to use (must match Google client)");
}

#[tokio::main]
async fn main() {
    let mut force_auth = false;
    let mut query = "in:inbox".to_string();
    let mut port: Option<u16> = None;
    let mut self_test = false;
    let mut redirect_uri: Option<String> = None;

    let mut args = env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--force-auth" => force_auth = true,
            "--self-test" => self_test = true,
            "--query" => {
                if let Some(v) = args.next() {
                    query = v;
                } else {
                    eprintln!("missing value for --query");
                    usage();
                    std::process::exit(2);
                }
            }
            "--port" => {
                if let Some(v) = args.next() {
                    match v.parse::<u16>() {
                        Ok(parsed) => port = Some(parsed),
                        Err(_) => {
                            eprintln!("invalid --port value: {v}");
                            std::process::exit(2);
                        }
                    }
                } else {
                    eprintln!("missing value for --port");
                    std::process::exit(2);
                }
            }
            "--redirect-uri" => {
                if let Some(v) = args.next() {
                    redirect_uri = Some(v);
                } else {
                    eprintln!("missing value for --redirect-uri");
                    std::process::exit(2);
                }
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            other => {
                eprintln!("unknown arg: {other}");
                usage();
                std::process::exit(2);
            }
        }
    }

    if self_test {
        let verifier = random_verifier();
        assert!(verifier.len() > 40);
        println!("self-test ok");
        return;
    }

    let client_id = env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    if client_id.trim().is_empty() {
        eprintln!("Missing GOOGLE_CLIENT_ID env var");
        std::process::exit(2);
    }
    let client_secret = env::var("GOOGLE_CLIENT_SECRET").ok();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed building reqwest client");

    let token = match ensure_access_token(
        &client,
        client_id.trim(),
        client_secret.as_deref(),
        force_auth,
        port,
        redirect_uri.as_deref(),
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            eprintln!("OAuth/token error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = read_one_email(&client, &token, &query).await {
        eprintln!("Gmail read error: {e}");
        std::process::exit(1);
    }

    println!("\nDone.");
}
