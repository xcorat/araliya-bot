//! HTTP comms channel — serves API endpoints under `/api/` and delegates
//! all other paths to the UI backend when a [`UiServeHandle`] is provided.

mod api;
mod ui;

use std::sync::Arc;

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

/// Optional UI serve handle — typed alias so the struct works with or without
/// the `subsystem-ui` feature.

// TODO: Maybe we should split the HTTP channel into two components: a pure API server (no UI
// support) and a UI server that serves static files and delegates to the API server for `/api/*`.
// Each would inherit common traits, but we dont have to have an option ui that way
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

// ── Server loop ───────────────────────────────────────────────────────────────

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

// ── Connection dispatch ───────────────────────────────────────────────────────

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

    let method = req.method;
    let path = req.path;
    let body = req.body;

    let session_memory = parse_session_subresource_path(&path, "memory");
    let session_files = parse_session_subresource_path(&path, "files");

    match (method.as_str(), path.as_str()) {
        ("GET", "/api/health")    => api::handle_health(&mut socket, &state, &channel_id).await,
        ("POST", "/api/health/refresh") => api::handle_health_refresh(&mut socket, &state, &channel_id).await,
        ("GET", "/api/tree")      => api::handle_tree(&mut socket, &state, &channel_id).await,
        ("POST", "/api/message")  => api::handle_message(&mut socket, &state, &channel_id, body).await,
        ("GET", "/api/sessions")  => api::handle_sessions(&mut socket, &state, &channel_id).await,
        ("GET", _) if session_memory.is_some() => {
            api::handle_session_memory(
                &mut socket,
                &state,
                &channel_id,
                session_memory.unwrap(),
            )
            .await
        }
        ("GET", _) if session_files.is_some() => {
            api::handle_session_files(
                &mut socket,
                &state,
                &channel_id,
                session_files.unwrap(),
            )
            .await
        }
        ("GET", p) if p.starts_with("/api/session/") => {
            let session_id = &p["/api/session/".len()..];
            api::handle_session_detail(&mut socket, &state, &channel_id, session_id).await
        }
        ("GET", "/favicon.ico") => write_response(
            &mut socket,
            "204 No Content",
            "image/x-icon",
            b"",
        )
        .await,
        ("GET", "/" | "/index.html") => ui::handle_root(&mut socket).await,
        ("GET", p) if p.starts_with("/ui") => ui::handle_ui_path(&mut socket, p, &ui_handle).await,
        _ => ui::handle_not_found(&mut socket).await,
    }
}

fn parse_session_subresource_path<'a>(path: &'a str, subresource: &str) -> Option<&'a str> {
    let prefix = "/api/sessions/";
    let suffix = format!("/{subresource}");

    if !path.starts_with(prefix) || !path.ends_with(&suffix) {
        return None;
    }

    let inner = &path[prefix.len()..path.len() - suffix.len()];
    if inner.is_empty() || inner.contains('/') {
        return None;
    }

    Some(inner)
}

// ── Request parsing ───────────────────────────────────────────────────────────

/// Parsed HTTP request with method, path, and optional body.
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

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

// ── Response helpers ──────────────────────────────────────────────────────────

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
