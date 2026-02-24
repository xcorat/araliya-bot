//! API route handlers for the HTTP channel (`/api/*`).

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tracing::warn;

use crate::error::AppError;
use crate::subsystems::comms::CommsState;

const NO_SESSION_ID: &str = "00000000-0000-0000-0000-000000000000";

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MessageRequest {
    message: String,
    session_id: Option<String>,
    mode: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/health
pub(super) async fn handle_health(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
) -> Result<(), AppError> {
    let response = tokio::time::timeout(Duration::from_secs(3), state.management_http_get()).await;

    match response {
        Ok(Ok(body)) => {
            super::write_json_response(socket, "200 OK", body.as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, "management health request failed: {e}");
            super::write_response(
                socket,
                "502 Bad Gateway",
                "text/plain; charset=utf-8",
                b"management adapter error\n",
            )
            .await
        }
        Err(_) => {
            warn!(%channel_id, "management health request timed out");
            super::write_response(
                socket,
                "504 Gateway Timeout",
                "text/plain; charset=utf-8",
                b"management adapter timeout\n",
            )
            .await
        }
    }
}

/// GET /api/tree — component tree (no private data).
pub(super) async fn handle_tree(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
) -> Result<(), AppError> {
    let response = tokio::time::timeout(Duration::from_secs(3), state.management_http_tree()).await;

    match response {
        Ok(Ok(body)) => super::write_json_response(socket, "200 OK", body.as_bytes()).await,
        Ok(Err(e)) => {
            warn!(%channel_id, "management tree request failed: {e}");
            super::write_response(
                socket,
                "502 Bad Gateway",
                "text/plain; charset=utf-8",
                b"management adapter error\n",
            )
            .await
        }
        Err(_) => {
            warn!(%channel_id, "management tree request timed out");
            super::write_response(
                socket,
                "504 Gateway Timeout",
                "text/plain; charset=utf-8",
                b"management adapter timeout\n",
            )
            .await
        }
    }
}

/// POST /api/message
pub(super) async fn handle_message(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
    body: Vec<u8>,
) -> Result<(), AppError> {
    let body_str = String::from_utf8(body)
        .map_err(|_| AppError::Comms("request body is not valid utf-8".to_string()))?;

    let msg_req: MessageRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(e) => {
            let err_body = serde_json::json!({
                "error": "bad_request",
                "message": format!("invalid JSON: {e}")
            });
            return super::write_json_response(
                socket,
                "400 Bad Request",
                err_body.to_string().as_bytes(),
            )
            .await;
        }
    };

    let requested_session_id = msg_req
        .session_id
        .as_deref()
        .filter(|sid| !sid.is_empty() && *sid != NO_SESSION_ID)
        .map(ToString::to_string);

    let reply_result = tokio::time::timeout(
        Duration::from_secs(120),
        state.send_message(channel_id, msg_req.message.clone(), requested_session_id),
    )
    .await;

    match reply_result {
        Ok(Ok(reply)) => {
            let resp_body = serde_json::json!({
                "session_id": reply.session_id.unwrap_or_else(|| NO_SESSION_ID.to_string()),
                "mode": msg_req.mode.as_deref().unwrap_or("chat"),
                "reply": reply.reply,
                "working_memory_updated": false,
            });
            super::write_json_response(socket, "200 OK", resp_body.to_string().as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, "message send failed: {e}");
            let err_body = serde_json::json!({
                "error": "internal",
                "message": format!("{e}")
            });
            super::write_json_response(
                socket,
                "502 Bad Gateway",
                err_body.to_string().as_bytes(),
            )
            .await
        }
        Err(_) => {
            let err_body = serde_json::json!({
                "error": "timeout",
                "message": "LLM request timed out"
            });
            super::write_json_response(
                socket,
                "504 Gateway Timeout",
                err_body.to_string().as_bytes(),
            )
            .await
        }
    }
}

/// GET /api/sessions
pub(super) async fn handle_sessions(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
) -> Result<(), AppError> {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        state.request_sessions(),
    )
    .await;

    match result {
        Ok(Ok(data)) => {
            super::write_json_response(socket, "200 OK", data.as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, "sessions request failed: {e}");
            let err_body = serde_json::json!({
                "error": "internal",
                "message": format!("{e}")
            });
            super::write_json_response(
                socket,
                "502 Bad Gateway",
                err_body.to_string().as_bytes(),
            )
            .await
        }
        Err(_) => {
            let err_body = serde_json::json!({
                "error": "timeout",
                "message": "sessions request timed out"
            });
            super::write_json_response(
                socket,
                "504 Gateway Timeout",
                err_body.to_string().as_bytes(),
            )
            .await
        }
    }
}

/// GET /api/session/{session_id}
pub(super) async fn handle_session_detail(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        state.request_session_detail(session_id),
    )
    .await;

    match result {
        Ok(Ok(data)) => {
            super::write_json_response(socket, "200 OK", data.as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, %session_id, "session detail request failed: {e}");
            let err_body = serde_json::json!({
                "error": "not_found",
                "message": format!("{e}")
            });
            super::write_json_response(
                socket,
                "404 Not Found",
                err_body.to_string().as_bytes(),
            )
            .await
        }
        Err(_) => {
            let err_body = serde_json::json!({
                "error": "timeout",
                "message": "session detail request timed out"
            });
            super::write_json_response(
                socket,
                "504 Gateway Timeout",
                err_body.to_string().as_bytes(),
            )
            .await
        }
    }
}

/// GET /api/sessions/{session_id}/memory
pub(super) async fn handle_session_memory(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        state.request_session_memory(session_id),
    )
    .await;

    match result {
        Ok(Ok(data)) => {
            super::write_json_response(socket, "200 OK", data.as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, %session_id, "session memory request failed: {e}");
            let err_body = serde_json::json!({
                "error": "not_found",
                "message": format!("{e}")
            });
            super::write_json_response(
                socket,
                "404 Not Found",
                err_body.to_string().as_bytes(),
            )
            .await
        }
        Err(_) => {
            let err_body = serde_json::json!({
                "error": "timeout",
                "message": "session memory request timed out"
            });
            super::write_json_response(
                socket,
                "504 Gateway Timeout",
                err_body.to_string().as_bytes(),
            )
            .await
        }
    }
}

/// GET /api/sessions/{session_id}/files
pub(super) async fn handle_session_files(
    socket: &mut tokio::net::TcpStream,
    state: &Arc<CommsState>,
    channel_id: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        state.request_session_files(session_id),
    )
    .await;

    match result {
        Ok(Ok(data)) => {
            super::write_json_response(socket, "200 OK", data.as_bytes()).await
        }
        Ok(Err(e)) => {
            warn!(%channel_id, %session_id, "session files request failed: {e}");
            let err_body = serde_json::json!({
                "error": "not_found",
                "message": format!("{e}")
            });
            super::write_json_response(
                socket,
                "404 Not Found",
                err_body.to_string().as_bytes(),
            )
            .await
        }
        Err(_) => {
            let err_body = serde_json::json!({
                "error": "timeout",
                "message": "session files request timed out"
            });
            super::write_json_response(
                socket,
                "504 Gateway Timeout",
                err_body.to_string().as_bytes(),
            )
            .await
        }
    }
}
