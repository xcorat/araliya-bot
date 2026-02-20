//! HTTP comms channel — exposes minimal HTTP endpoints and bridges requests
//! through the supervisor bus.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::subsystems::runtime::{Component, ComponentFuture};
use super::state::{CommsEvent, CommsState};

const MAX_HEADER_BYTES: usize = 8 * 1024;

pub struct HttpChannel {
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
}

impl HttpChannel {
    pub fn new(
        channel_id: impl Into<String>,
        bind_addr: impl Into<String>,
        state: Arc<CommsState>,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            bind_addr: bind_addr.into(),
            state,
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
            shutdown,
        ))
    }
}

async fn run_http(
    channel_id: String,
    bind_addr: String,
    state: Arc<CommsState>,
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
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(state, channel_id, socket).await {
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
) -> Result<(), AppError> {
    let request = read_request_line(&mut socket).await?;

    let Some((method, path)) = request else {
        return Ok(());
    };

    if method != "GET" {
        write_response(
            &mut socket,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed\n",
        )
        .await?;
        return Ok(());
    }

    match path.as_str() {
        "/health" => {
            let response = tokio::time::timeout(Duration::from_secs(3), state.management_http_get()).await;
            match response {
                Ok(Ok(body)) => {
                    write_response(
                        &mut socket,
                        "200 OK",
                        "application/json",
                        body.as_bytes(),
                    )
                    .await?;
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
        _ => {
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

async fn read_request_line(
    socket: &mut tokio::net::TcpStream,
) -> Result<Option<(String, String)>, AppError> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

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

    let request = String::from_utf8(buffer)
        .map_err(|_| AppError::Comms("http request was not valid utf-8".to_string()))?;

    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| AppError::Comms("empty http request".to_string()))?;

    let mut parts = first_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| AppError::Comms("missing http method".to_string()))?;
    let path = parts
        .next()
        .ok_or_else(|| AppError::Comms("missing http path".to_string()))?;

    Ok(Some((method.to_string(), path.to_string())))
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
