//! Axum handlers for `/api/*` routes.

use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use std::convert::Infallible;

use futures_util::stream;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::StreamExt;
use tracing::warn;

use araliya_core::types::llm::StreamChunk;

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

#[derive(Deserialize)]
pub(super) struct SetDefaultRequest {
    provider: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn json_error(code: &str, msg: impl std::fmt::Display) -> Json<serde_json::Value> {
    Json(json!({ "error": code, "message": format!("{msg}") }))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

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

pub(super) async fn llm_providers(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(Duration::from_secs(10), state.comms.request_llm_providers()).await {
        Ok(Ok(data)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            data,
        )
            .into_response(),
        Ok(Err(e)) => {
            warn!(channel_id = %state.channel_id, "llm providers request failed: {e}");
            (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "llm providers request timed out"),
        )
            .into_response(),
    }
}

pub(super) async fn llm_set_default(
    State(state): State<AxumState>,
    Json(req): Json<SetDefaultRequest>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.set_llm_default(&req.provider),
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
            warn!(channel_id = %state.channel_id, "llm set_default request failed: {e}");
            (StatusCode::BAD_GATEWAY, json_error("internal", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "llm set_default request timed out"),
        )
            .into_response(),
    }
}

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

pub(super) async fn memory_agent_kg(
    State(state): State<AxumState>,
    Path(agent_id): Path<String>,
) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(10),
        state.comms.request_memory_kg(&agent_id),
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
            warn!(channel_id = %state.channel_id, %agent_id, "memory KG request failed: {e}");
            (StatusCode::NOT_FOUND, json_error("not_found", e)).into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            json_error("timeout", "memory KG request timed out"),
        )
            .into_response(),
    }
}

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

pub(super) async fn observe_snapshot(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(3),
        state.comms.management_observe_snapshot(),
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
            warn!(channel_id = %state.channel_id, "observe snapshot request failed: {e}");
            (StatusCode::BAD_GATEWAY, "management adapter error\n").into_response()
        }
        Err(_) => {
            warn!(channel_id = %state.channel_id, "observe snapshot request timed out");
            (StatusCode::GATEWAY_TIMEOUT, "management adapter timeout\n").into_response()
        }
    }
}

pub(super) async fn observe_clear(State(state): State<AxumState>) -> Response {
    match tokio::time::timeout(
        Duration::from_secs(3),
        state.comms.management_observe_clear(),
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
            warn!(channel_id = %state.channel_id, "observe clear request failed: {e}");
            (StatusCode::BAD_GATEWAY, "management adapter error\n").into_response()
        }
        Err(_) => {
            warn!(channel_id = %state.channel_id, "observe clear request timed out");
            (StatusCode::GATEWAY_TIMEOUT, "management adapter timeout\n").into_response()
        }
    }
}

/// `GET /api/observe/events` — Server-Sent Events stream of observability events.
///
/// Each event is a JSON-serialized [`araliya_core::obs::ObsEvent`].
///
/// When the ring buffer overflows and events are dropped, a special
/// `event: lagged` message is sent carrying `{"skipped": N}` so the client
/// knows to re-fetch the snapshot from `manage/observe/snapshot` if needed.
pub(super) async fn observe_events(State(state): State<AxumState>) -> impl IntoResponse {
    let Some(obs_bus) = &state.obs_bus else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "observability bus not enabled",
        )
            .into_response();
    };

    let rx = obs_bus.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|r| match r {
        Ok(event) => serde_json::to_string(&event)
            .ok()
            .map(|data| Ok::<Event, Infallible>(Event::default().data(data))),
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            // Notify the client how many events were skipped — it can re-fetch the snapshot.
            Some(Ok(Event::default()
                .event("lagged")
                .data(format!(r#"{{"skipped":{n}}}"#))))
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ── Notes routes ─────────────────────────────────────────────────────────────

/// `GET /notes/` — list all markdown files in notes_dir.
#[cfg(feature = "plugin-homebuilder")]
pub(crate) async fn notes_index(State(state): State<super::AxumState>) -> Response {
    use axum::http::header;

    let Some(notes_dir) = &state.notes_dir else {
        return (StatusCode::NOT_FOUND, "notes not configured").into_response();
    };

    let entries = match collect_note_entries(notes_dir) {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };

    let links: String = entries
        .iter()
        .map(|name| format!(r#"<li><a href="/notes/{name}">{name}</a></li>"#))
        .collect::<Vec<_>>()
        .join("\n");

    let html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<title>Notes</title>
<style>body{{font-family:monospace;background:#080c0e;color:#b8ced6;padding:2rem;max-width:800px;margin:0 auto}}
a{{color:#4db8cc}}h1{{color:#e8eef0;margin-bottom:1.5rem}}li{{margin:.5rem 0}}</style>
</head><body><h1>Notes</h1><ul>{links}</ul><p><a href="/home/">← Home</a></p></body></html>"#
    );

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

/// `GET /notes/{*path}` — serve a single note; render `.md` as HTML.
#[cfg(feature = "plugin-homebuilder")]
pub(crate) async fn notes_serve(
    State(state): State<super::AxumState>,
    Path(path): Path<String>,
) -> Response {
    use axum::http::header;

    let Some(notes_dir) = &state.notes_dir else {
        return (StatusCode::NOT_FOUND, "notes not configured").into_response();
    };

    // Security: reject path traversal attempts
    if path.contains("..") || path.contains('\0') {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let file_path = notes_dir.join(&path);

    // Only serve files inside notes_dir (double-check after joining)
    let canonical = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let notes_canonical = notes_dir
        .canonicalize()
        .unwrap_or_else(|_| notes_dir.clone());
    if !canonical.starts_with(&notes_canonical) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    if path.ends_with(".md") {
        let html_body = render_markdown(&content);
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path);
        let page = format!(
            r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<title>{filename}</title>
<style>body{{font-family:monospace;background:#080c0e;color:#b8ced6;padding:2rem;max-width:800px;margin:0 auto;line-height:1.7}}
a{{color:#4db8cc}}h1,h2,h3{{color:#e8eef0}}code,pre{{background:#111b1f;padding:.2em .4em;border-radius:2px;color:#7ed4e2}}
pre code{{padding:0}}blockquote{{border-left:3px solid #1c2d33;padding-left:1rem;color:#7a9aa5}}</style>
</head><body>{html_body}<p><a href="/notes/">← Notes</a></p></body></html>"#
        );
        ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
    } else {
        // Serve non-markdown files as plain text
        (
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            content,
        )
            .into_response()
    }
}

#[cfg(feature = "plugin-homebuilder")]
fn collect_note_entries(dir: &std::path::Path) -> Result<Vec<String>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("cannot read notes dir: {e}"))?;
    let mut names: Vec<String> = read
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // Only list markdown and text files
            if name.ends_with(".md") || name.ends_with(".txt") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

/// Render a markdown string to an HTML fragment using pulldown-cmark.
#[cfg(feature = "plugin-homebuilder")]
fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, opts);
    let mut html_out = String::with_capacity(md.len() * 2);
    html::push_html(&mut html_out, parser);
    html_out
}
