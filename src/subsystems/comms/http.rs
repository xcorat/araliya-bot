//! HTTP comms channel — serves API endpoints under `/api/` and delegates
//! all other paths to the UI backend when a [`UiServeHandle`] is provided.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::subsystems::runtime::{Component, ComponentFuture};
#[cfg(feature = "subsystem-ui")]
use crate::subsystems::ui::UiServeHandle;
use super::state::{CommsEvent, CommsState};

const MAX_HEADER_BYTES: usize = 8 * 1024;

/// Simple welcome page served at the root path.
const ROOT_INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Araliya</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: system-ui, -apple-system, sans-serif;
      background: #0f0f0f; color: #e0e0e0;
      display: flex; align-items: center; justify-content: center;
      height: 100vh;
    }
    .card {
      text-align: center; padding: 2rem 3rem;
      border: 1px solid #333; border-radius: 12px;
      background: #1a1a1a;
    }
    h1 { font-size: 1.5rem; margin-bottom: 0.5rem; }
    p  { font-size: 0.9rem; color: #888; margin-bottom: 1rem; }
    a {
      display: inline-block; padding: 0.5rem 1.5rem;
      border-radius: 8px; background: #2a2a3a; color: #c0c0e0;
      text-decoration: none; font-size: 0.9rem;
      transition: background 0.15s;
    }
    a:hover { background: #3a3a5a; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Araliya</h1>
    <p>Bot is running.</p>
    <a href="/ui/">Open UI &rarr;</a>
  </div>
</body>
</html>
"#;

/// Optional UI serve handle — typed alias so the struct works with or without
/// the `subsystem-ui` feature.
#[cfg(feature = "subsystem-ui")]
type OptionalUiHandle = Option<UiServeHandle>;
#[cfg(not(feature = "subsystem-ui"))]
type OptionalUiHandle = Option<()>;

pub struct HttpChannel {
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
    ui_handle: OptionalUiHandle,
}

impl HttpChannel {
    pub fn new(
        channel_id: impl Into<String>,
        bind_addr: impl Into<String>,
        state: Arc<CommsState>,
        ui_handle: OptionalUiHandle,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            bind_addr: bind_addr.into(),
            state,
            ui_handle,
        }
    }
}

impl Component for HttpChannel {
    fn id(&self) -> &str {
        &self.channel_id
    }

    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture {
        Box::pin(run_http(
            self.channel_id,
            self.bind_addr,
            self.state,
            self.ui_handle,
            shutdown,
        ))
    }
}

async fn run_http(
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
    ui_handle: OptionalUiHandle,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| AppError::Comms(format!("http bind failed on {bind_addr}: {e}")))?;

    info!(%channel_id, %bind_addr, "http channel listening");

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                info!(%channel_id, "http channel shutting down");
                break;
            }

            accepted = listener.accept() => {
                match accepted {
                    Ok((socket, peer)) => {
                        debug!(%channel_id, %peer, "http client connected");
                        state.report_event(CommsEvent::SessionStarted { channel_id: channel_id.clone() });
                        let state = state.clone();
                        let channel_id = channel_id.clone();
                        let ui_handle = ui_handle.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(state, channel_id, socket, ui_handle).await {
                                warn!("http connection handling failed: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(%channel_id, "http accept error: {e}");
                    }
                }
            }
        }
    }

    state.report_event(CommsEvent::ChannelShutdown { channel_id });
    Ok(())
}

async fn handle_connection(
    state: Arc<CommsState>,
    channel_id: String,
    mut socket: tokio::net::TcpStream,
    ui_handle: OptionalUiHandle,
) -> Result<(), AppError> {
    let request = read_request(&mut socket).await?;

    let Some(req) = request else {
        return Ok(());
    };

    match (req.method.as_str(), req.path.as_str()) {
        // ── GET /api/health ──────────────────────────────────────────
        ("GET", "/api/health") => {
            let response = tokio::time::timeout(Duration::from_secs(3), state.management_http_get()).await;
            match response {
                Ok(Ok(body)) => {
                    write_json_response(&mut socket, "200 OK", body.as_bytes()).await?;
                }
                Ok(Err(e)) => {
                    warn!(%channel_id, "management health request failed: {e}");
                    write_response(
                        &mut socket,
                        "502 Bad Gateway",
                        "text/plain; charset=utf-8",
                        b"management adapter error\n",
                    )
                    .await?;
                }
                Err(_) => {
                    warn!(%channel_id, "management health request timed out");
                    write_response(
                        &mut socket,
                        "504 Gateway Timeout",
                        "text/plain; charset=utf-8",
                        b"management adapter timeout\n",
                    )
                    .await?;
                }
            }
        }

        // ── POST /api/message ────────────────────────────────────────
        ("POST", "/api/message") => {
            let body_str = String::from_utf8(req.body)
                .map_err(|_| AppError::Comms("request body is not valid utf-8".to_string()))?;

            let msg_req: MessageRequest = match serde_json::from_str(&body_str) {
                Ok(r) => r,
                Err(e) => {
                    let err_body = serde_json::json!({
                        "error": "bad_request",
                        "message": format!("invalid JSON: {e}")
                    });
                    write_json_response(
                        &mut socket,
                        "400 Bad Request",
                        err_body.to_string().as_bytes(),
                    )
                    .await?;
                    return Ok(());
                }
            };

            let reply_result = tokio::time::timeout(
                Duration::from_secs(120),
                state.send_message(&channel_id, msg_req.message.clone()),
            )
            .await;

            match reply_result {
                Ok(Ok(reply)) => {
                    let resp_body = serde_json::json!({
                        "session_id": msg_req.session_id.unwrap_or_default(),
                        "mode": msg_req.mode.as_deref().unwrap_or("chat"),
                        "reply": reply,
                        "working_memory_updated": false,
                    });
                    write_json_response(&mut socket, "200 OK", resp_body.to_string().as_bytes())
                        .await?;
                }
                Ok(Err(e)) => {
                    warn!(%channel_id, "message send failed: {e}");
                    let err_body = serde_json::json!({
                        "error": "internal",
                        "message": format!("{e}")
                    });
                    write_json_response(
                        &mut socket,
                        "502 Bad Gateway",
                        err_body.to_string().as_bytes(),
                    )
                    .await?;
                }
                Err(_) => {
                    let err_body = serde_json::json!({
                        "error": "timeout",
                        "message": "LLM request timed out"
                    });
                    write_json_response(
                        &mut socket,
                        "504 Gateway Timeout",
                        err_body.to_string().as_bytes(),
                    )
                    .await?;
                }
            }
        }

        // ── GET /api/sessions ────────────────────────────────────────
        ("GET", "/api/sessions") => {
            // Stub: no session listing on the bus yet — return empty list.
            let body = serde_json::json!({ "sessions": [] });
            write_json_response(&mut socket, "200 OK", body.to_string().as_bytes()).await?;
        }

        // ── Root welcome page ────────────────────────────────────────
        ("GET", "/" | "/index.html") => {
            write_response(
                &mut socket,
                "200 OK",
                "text/html; charset=utf-8",
                ROOT_INDEX_HTML.as_bytes(),
            )
            .await?;
        }

        // ── UI delegation ────────────────────────────────────────────
        ("GET", path) if path.starts_with("/ui") => {
            #[cfg(feature = "subsystem-ui")]
            if let Some(ref ui) = ui_handle {
                if let Some(resp) = ui.serve(path) {
                    write_response(&mut socket, resp.status, resp.content_type, &resp.body).await?;
                    return Ok(());
                }
            }
            #[cfg(not(feature = "subsystem-ui"))]
            let _ = &ui_handle;

            write_response(
                &mut socket,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"not found\n",
            )
            .await?;
        }

        // ── Catch-all 404 ────────────────────────────────────────────
        _ => {
            #[cfg(not(feature = "subsystem-ui"))]
            let _ = &ui_handle;

            write_response(
                &mut socket,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"not found\n",
            )
            .await?;
        }
    }

    Ok(())
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MessageRequest {
    message: String,
    session_id: Option<String>,
    mode: Option<String>,
}

/// Parsed HTTP request with method, path, and optional body.
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

// ── Request parsing ───────────────────────────────────────────────────────────

async fn read_request(
    socket: &mut tokio::net::TcpStream,
) -> Result<Option<HttpRequest>, AppError> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    // Read until we have the full header block (terminated by \r\n\r\n).
    loop {
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            return Err(AppError::Comms("http request truncated".to_string()));
        }

        buffer.extend_from_slice(&chunk[..n]);

        if buffer.len() > MAX_HEADER_BYTES {
            return Err(AppError::Comms("http request headers too large".to_string()));
        }

        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    // Split headers from any body bytes already read.
    let header_end = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap();
    let body_start = header_end + 4;
    let header_bytes = &buffer[..header_end];

    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| AppError::Comms("http request was not valid utf-8".to_string()))?;

    let first_line = header_str
        .lines()
        .next()
        .ok_or_else(|| AppError::Comms("empty http request".to_string()))?;

    let mut parts = first_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| AppError::Comms("missing http method".to_string()))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| AppError::Comms("missing http path".to_string()))?
        .to_string();

    // Parse Content-Length from headers (case-insensitive).
    let content_length: usize = header_str
        .lines()
        .skip(1)
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-length:") {
                line.split_once(':')
                    .and_then(|(_, v)| v.trim().parse().ok())
            } else {
                None
            }
        })
        .unwrap_or(0);

    // Read body if Content-Length > 0.
    let mut body = buffer[body_start..].to_vec();
    while body.len() < content_length {
        let remaining = content_length - body.len();
        let mut read_buf = vec![0u8; remaining.min(8192)];
        let n = socket.read(&mut read_buf).await?;
        if n == 0 {
            return Err(AppError::Comms("http request body truncated".to_string()));
        }
        body.extend_from_slice(&read_buf[..n]);
    }
    body.truncate(content_length);

    Ok(Some(HttpRequest { method, path, body }))
}

async fn write_redirect(
    socket: &mut tokio::net::TcpStream,
    location: &str,
) -> Result<(), AppError> {
    let header = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    socket.write_all(header.as_bytes()).await?;
    socket.shutdown().await?;
    Ok(())
}

async fn write_json_response(
    socket: &mut tokio::net::TcpStream,
    status: &str,
    body: &[u8],
) -> Result<(), AppError> {
    write_response(socket, status, "application/json", body).await
}

async fn write_response(
    socket: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    socket.write_all(header.as_bytes()).await?;
    socket.write_all(body).await?;
    socket.shutdown().await?;
    Ok(())
}
