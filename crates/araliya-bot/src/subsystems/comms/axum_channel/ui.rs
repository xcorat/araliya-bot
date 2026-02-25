//! UI route handlers for the axum channel.
//!
//! The root handler serves a static welcome page.  The catch-all `serve_path`
//! handler delegates to the [`UiServe`] backend when one is configured —
//! calling it via [`tokio::task::spawn_blocking`] because [`UiServe::serve`]
//! may perform blocking file I/O.

use axum::{
    body::Body,
    extract::State,
    http::{header, Response, StatusCode},
  response::{Html, IntoResponse},
};

use super::AxumState;

// ── Root page ─────────────────────────────────────────────────────────────────

/// Simple welcome page served at the root path (mirrors `http/ui.rs`).
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

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET / — root welcome page.
pub(super) async fn root() -> Html<&'static str> {
    Html(ROOT_INDEX_HTML)
}

/// GET /*path — delegate to the UI backend (SPA fallback) or 404.
pub(super) async fn serve_path(
    State(state): State<AxumState>,
    uri: axum::http::Uri,
) -> axum::response::Response {
    let path = uri.path().to_string();

    #[cfg(feature = "subsystem-ui")]
    if let Some(ui) = state.ui {
        // UiServe::serve does blocking file I/O — run it off the async executor.
        let result = tokio::task::spawn_blocking(move || ui.serve(&path)).await;
        match result {
            Ok(Some(resp)) => {
                let status = parse_status_code(resp.status);
                return Response::builder()
                    .status(status)
                    .header(header::CONTENT_TYPE, resp.content_type)
                    .body(Body::from(resp.body))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
            Ok(None) => {}
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
    #[cfg(not(feature = "subsystem-ui"))]
    let _ = path;

    StatusCode::NOT_FOUND.into_response()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse the status code out of a `"NNN Reason"` string (e.g. `"200 OK"`).
fn parse_status_code(s: &str) -> StatusCode {
    s.split_once(' ')
        .and_then(|(code, _)| code.parse::<u16>().ok())
        .and_then(|n| StatusCode::from_u16(n).ok())
        .unwrap_or(StatusCode::OK)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn parse_status_ok() {
        assert_eq!(parse_status_code("200 OK"), StatusCode::OK);
    }

    #[test]
    fn parse_status_not_found() {
        assert_eq!(parse_status_code("404 Not Found"), StatusCode::NOT_FOUND);
    }

    #[test]
    fn parse_status_with_no_reason() {
        // Some ServeResponse impls may have no reason phrase.
        assert_eq!(parse_status_code("500 "), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn parse_status_invalid_falls_back_to_ok() {
        // Unknown / unparseable must never panic.
        assert_eq!(parse_status_code("garbage"), StatusCode::OK);
    }
}
