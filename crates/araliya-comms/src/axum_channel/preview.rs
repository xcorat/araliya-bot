//! Preview route — serves static files from `webbuilder` workspace dist dirs.

use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

use super::AxumState;

// ── Handler ───────────────────────────────────────────────────────────────────

pub(crate) async fn preview_handler(
    State(state): State<AxumState>,
    AxumPath((session_id, path)): AxumPath<(String, String)>,
) -> Response {
    let Some(ref preview_root) = state.preview_root else {
        return StatusCode::NOT_FOUND.into_response();
    };

    serve_preview_file(preview_root, &session_id, &path).await
}

pub(crate) async fn preview_index_handler(
    State(state): State<AxumState>,
    AxumPath(session_id): AxumPath<String>,
) -> Response {
    let Some(ref preview_root) = state.preview_root else {
        return StatusCode::NOT_FOUND.into_response();
    };

    serve_preview_file(preview_root, &session_id, "index.html").await
}

async fn serve_preview_file(preview_root: &Path, session_id: &str, path: &str) -> Response {
    if !session_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let workspace_name = format!("webbuilder-{session_id}");
    let dist_root = preview_root.join(&workspace_name).join("dist");

    let requested = PathBuf::from(path.trim_start_matches('/'));
    let file_path = dist_root.join(&requested);

    let file_path = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            if path != "index.html" {
                let index = dist_root.join("index.html");
                match index.canonicalize() {
                    Ok(p) => p,
                    Err(_) => return StatusCode::NOT_FOUND.into_response(),
                }
            } else {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    };

    let dist_canonical = match dist_root.canonicalize() {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !file_path.starts_with(&dist_canonical) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let bytes = match tokio::fs::read(&file_path).await {
        Ok(b) => b,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let content_type = mime_from_extension(&file_path);

    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    resp
}

fn mime_from_extension(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        Some("map") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        _ => "application/octet-stream",
    }
}
