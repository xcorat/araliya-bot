//! UI route handlers — serves the root welcome page and delegates to the
//! embedded UI backend when the `subsystem-ui` feature is enabled.

use crate::error::AppError;

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

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET / | /index.html — root welcome page.
pub(super) async fn handle_root(
    socket: &mut tokio::net::TcpStream,
) -> Result<(), AppError> {
    super::write_response(
        socket,
        "200 OK",
        "text/html; charset=utf-8",
        ROOT_INDEX_HTML.as_bytes(),
    )
    .await
}

/// GET /ui/* — delegate to the embedded UI backend, or 404.
pub(super) async fn handle_ui_path(
    socket: &mut tokio::net::TcpStream,
    path: &str,
    ui_handle: &super::OptionalUiHandle,
) -> Result<(), AppError> {
    #[cfg(feature = "subsystem-ui")]
    if let Some(ui) = ui_handle {
        if let Some(resp) = ui.serve(path) {
            return super::write_response(socket, resp.status, resp.content_type, &resp.body).await;
        }
    }
    #[cfg(not(feature = "subsystem-ui"))]
    let _ = (path, ui_handle);

    handle_not_found(socket).await
}

/// Catch-all 404 response.
pub(super) async fn handle_not_found(
    socket: &mut tokio::net::TcpStream,
) -> Result<(), AppError> {
    super::write_response(
        socket,
        "404 Not Found",
        "text/plain; charset=utf-8",
        b"not found\n",
    )
    .await
}
