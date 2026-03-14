//! Axum handlers for `/api/*` routes.
//!
//! Each handler receives [`AxumState`] via [`axum::extract::State`] and
//! returns an axum [`Response`].  Timeout logic and bus interactions mirror
//! the hand-rolled implementations in [`super::super::http::api`].

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use std::convert::Infallible;

use futures_util::stream;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::llm::StreamChunk;

use super::AxumState;

const NO_SESSION_ID: &str = "00000000-0000-0000-0000-000000000000";

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(super) struct MessageRequest {
    message: String,
    session_id: Option<String>,
    agent_id: Option<String>,
    mode: Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a JSON error response body.
fn json_error(code: &str, msg: impl std::fmt::Display) -> Json<serde_json::Value> {
    Json(json!({ "error": code, "message": format!("{msg}") }))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/health
pub(super) async fn health(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(Duration::from_secs(3), state.comms.management_http_get()).await {
        Ok(Ok(body)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "management health request failed: {e}");
            (StatusCode::BAD_GATEWAY, "management adapter error\n").into_response()
        }
        Err(_) => {
            warn!(channel_id = %state.channel_id, "management health request timed out");
            (StatusCode::GATEWAY_TIMEOUT, "management adapter timeout\n").into_response()
        }
    }
}

/// POST /api/health/refresh — triggers a live health check across all subsystems.
///
/// Each subsystem reruns its health check synchronously (with a 5 s per-subsystem
/// timeout) and returns the updated aggregated health body, identical in shape
/// to `GET /api/health`.
pub(super) async fn health_refresh(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(15),
        state.comms.management_health_refresh(),
    )
    .await
    {
        Ok(Ok(body)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "health refresh failed: {e}");
            (StatusCode::BAD_GATEWAY, "management adapter error\n").into_response()
        }
        Err(_) => {
            warn!(channel_id = %state.channel_id, "health refresh timed out");
            (StatusCode::GATEWAY_TIMEOUT, "management adapter timeout\n").into_response()
        }
    }
}

/// GET /api/tree — component tree (no private data).
pub(super) async fn tree(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(Duration::from_secs(3), state.comms.management_http_tree()).await {
        Ok(Ok(body)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "management tree request failed: {e}");
            (StatusCode::BAD_GATEWAY, "management adapter error\n").into_response()
        }
        Err(_) => {
            warn!(channel_id = %state.channel_id, "management tree request timed out");
            (StatusCode::GATEWAY_TIMEOUT, "management adapter timeout\n").into_response()
        }
    }
}

/// POST /api/message
pub(super) async fn message(
    State(state): State<AxumState>,
    Json(req): Json<MessageRequest>,
) -> Response {
    let session_id = req
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty() && *s != NO_SESSION_ID)
        .map(ToString::to_string);

    match tokio::time::timeout(
        Duration::from_secs(120),
        state.comms.send_message(
            &state.channel_id,
            req.message.clone(),
            session_id,
            req.agent_id.clone(),
        ),
    )
    .await
    {
        Ok(Ok(reply)) => {
            let body = json!({
                "session_id": reply.session_id.unwrap_or_else(|| NO_SESSION_ID.to_string()),
                "mode": req.mode.as_deref().unwrap_or("chat"),
                "reply": reply.reply,
                "thinking": reply.thinking,
                "working_memory_updated": false,
                "usage": reply.usage.map(|u| json!({
                    "prompt_tokens": u.input_tokens,
                    "completion_tokens": u.output_tokens,
                    "total_tokens": u.input_tokens + u.output_tokens,
                })),
                "timing": reply.timing.map(|t| json!({
                    "ttft_ms": t.ttft_ms,
                    "total_ms": t.total_ms,
                })),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "message send failed: {e}");
            (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "LLM request timed out"),
        )
            .into_response(),
    }
}

/// POST /api/message/stream — SSE streaming completion routed through the agent pipeline.
///
/// The selected agent runs its full instruction + tool pipeline (buffered),
/// then streams the final response pass as SSE events.
///
/// Events emitted:
/// - `event: thinking` / `data: {"delta": "..."}` — reasoning token delta
/// - `event: content`  / `data: {"delta": "..."}` — answer token delta
/// - `event: done`     / `data: {"usage": {...}}` — end of stream with usage
pub(super) async fn message_stream(
    State(state): State<AxumState>,
    Json(req): Json<MessageRequest>,
) -> Response {
    let channel_id = state.channel_id.to_string();
    let session_id = req
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty() && *s != NO_SESSION_ID)
        .map(ToString::to_string);

    let rx = match state
        .comms
        .stream_via_agent(&channel_id, req.message, session_id, req.agent_id.clone())
        .await
    {
        Ok(rx) => rx,
        Err(e) => {
            warn!(%channel_id, "stream_via_agent failed: {e}");
            return (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response();
        }
    };

    // Convert mpsc::Receiver into a futures Stream for axum's SSE.
    let event_stream = stream::unfold(rx, |mut rx| async move {
        let chunk = rx.recv().await?;
        let event: Result<Event, Infallible> = Ok(match chunk {
            StreamChunk::Thinking(delta) => {
                let data = json!({"delta": delta}).to_string();
                Event::default().event("thinking").data(data)
            }
            StreamChunk::Content(delta) => {
                let data = json!({"delta": delta}).to_string();
                Event::default().event("content").data(data)
            }
            StreamChunk::Done { usage, timing } => {
                let data = json!({
                    "usage": usage.map(|u| json!({
                        "prompt_tokens": u.input_tokens,
                        "completion_tokens": u.output_tokens,
                        "reasoning_tokens": u.reasoning_tokens,
                        "cached_input_tokens": u.cached_input_tokens,
                    })),
                    "timing": timing.map(|t| json!({
                        "ttft_ms": t.ttft_ms,
                        "total_ms": t.total_ms,
                    }))
                })
                .to_string();
                Event::default().event("done").data(data)
            }
        });
        Some((event, rx))
    });

    Sse::new(event_stream).into_response()
}

/// GET /api/sessions
pub(super) async fn sessions(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(Duration::from_secs(10), state.comms.request_sessions()).await {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "sessions request failed: {e}");
            (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "sessions request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/agents
pub(super) async fn agents(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(Duration::from_secs(10), state.comms.request_agents()).await {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "agents request failed: {e}");
            (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "agents request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/agents/{agent_id}/session — primary session transcript for an agent.
///
/// Returns `{session_id, transcript}` (`SessionDetailResponse` shape).
/// `session_id` is `null` and `transcript` is `[]` when the agent has no session yet.
pub(super) async fn agent_session(
    State(state): State<AxumState>,
    Path(agent_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_agent_session(&agent_id),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %agent_id, "agent session request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "agent session request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/agents/{agent_id}/spend — accumulated token/cost totals for an agent's active session.
pub(super) async fn agent_spend(
    State(state): State<AxumState>,
    Path(agent_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_agent_spend(&agent_id),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %agent_id, "agent spend request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "agent spend request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/agents/{agent_id}/kg — knowledge graph for an agent's kgdocstore.
pub(super) async fn agent_kg(
    State(state): State<AxumState>,
    Path(agent_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_agent_kg(&agent_id),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %agent_id, "agent KG request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "agent KG request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/session/{session_id}
pub(super) async fn session_detail(
    State(state): State<AxumState>,
    Path(session_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_session_detail(&session_id, None),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %session_id, "session detail request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "session detail request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/sessions/{session_id}/memory
pub(super) async fn session_memory(
    State(state): State<AxumState>,
    Path(session_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_session_memory(&session_id, None),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %session_id, "session memory request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "session memory request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/sessions/{session_id}/debug
pub(super) async fn session_debug(
    State(state): State<AxumState>,
    Path(session_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_session_debug(&session_id, None),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %session_id, "session debug request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "session debug request timed out"),
        )
            .into_response(),
    }
}

/// GET /api/sessions/{session_id}/files
pub(super) async fn session_files(
    State(state): State<AxumState>,
    Path(session_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_session_files(&session_id, None),
    )
    .await
    {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, %session_id, "session files request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "session files request timed out"),
        )
            .into_response(),
    }
}
