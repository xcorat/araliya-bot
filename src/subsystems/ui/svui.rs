//! SvUI backend — serves a Svelte-based web interface.
//!
//! When a `static_dir` is configured and the directory exists, files are
//! served from disk.  Otherwise a minimal built-in `index.html` placeholder
//! is returned for the root path.
//!
//! The serve function handles SPA-style routing: any path that doesn't match
//! a real static file falls back to `index.html`.

use std::path::{Path, PathBuf};

use tracing::debug;

use super::{ServeResponse, UiServe};

// ── Built-in fallback ─────────────────────────────────────────────────────────

const BUILTIN_INDEX_HTML: &str = r#"<!doctype html>
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
    p  { font-size: 0.9rem; color: #888; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Araliya</h1>
    <p>UI not built yet — run the Svelte build to populate the static directory.</p>
  </div>
</body>
</html>
"#;

// ── SvuiBackend ───────────────────────────────────────────────────────────────

/// Svelte-based UI backend.
///
/// Serves static files from `static_dir` if available, otherwise returns the
/// built-in placeholder page.
pub struct SvuiBackend {
    /// Resolved static directory path — `None` if not configured or missing.
    static_dir: Option<PathBuf>,
}

impl SvuiBackend {
    pub fn new(static_dir: Option<String>) -> Self {
        let resolved = static_dir
            .map(PathBuf::from)
            .filter(|p| p.is_dir());

        if let Some(ref dir) = resolved {
            tracing::info!(dir = %dir.display(), "svui: serving static files from disk");
        } else {
            tracing::info!("svui: no static directory — using built-in placeholder");
        }

        Self { static_dir: resolved }
    }
}

impl UiServe for SvuiBackend {
    fn serve(&self, path: &str) -> Option<ServeResponse> {
        // Reject paths that try to escape the static root.
        if path.contains("..") {
            return Some(ServeResponse {
                status: "400 Bad Request",
                content_type: "text/plain; charset=utf-8",
                body: b"bad request\n".to_vec(),
            });
        }

        // If we have a static directory, try to serve from it.
        if let Some(ref root) = self.static_dir {
            return Some(serve_static(root, path));
        }

        // No static dir — serve built-in placeholder for root paths only.
        match path {
            "/" | "/index.html" => {
                debug!("svui: serving built-in index.html");
                Some(ServeResponse {
                    status: "200 OK",
                    content_type: "text/html; charset=utf-8",
                    body: BUILTIN_INDEX_HTML.as_bytes().to_vec(),
                })
            }
            _ => None,
        }
    }
}

// ── static file serving ───────────────────────────────────────────────────────

/// Serve a file from `root` for the given request `path`.
///
/// - `/` maps to `index.html`.
/// - If the exact file isn't found, falls back to `index.html` (SPA routing).
fn serve_static(root: &Path, path: &str) -> ServeResponse {
    let relative = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    let file_path = root.join(relative);

    // Try exact file first.
    if file_path.is_file() {
        return read_static_file(&file_path);
    }

    // SPA fallback — serve index.html for non-asset paths.
    let index = root.join("index.html");
    if index.is_file() {
        debug!(path, "svui: SPA fallback to index.html");
        return read_static_file(&index);
    }

    // Static dir exists but no index.html — shouldn't happen in a valid build.
    ServeResponse {
        status: "404 Not Found",
        content_type: "text/plain; charset=utf-8",
        body: b"not found\n".to_vec(),
    }
}

/// Read a file from disk and return a [`ServeResponse`] with the appropriate
/// MIME type inferred from the file extension.
fn read_static_file(path: &Path) -> ServeResponse {
    match std::fs::read(path) {
        Ok(body) => {
            let content_type = mime_from_extension(path);
            ServeResponse {
                status: "200 OK",
                content_type,
                body,
            }
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), "svui: failed to read file: {e}");
            ServeResponse {
                status: "500 Internal Server Error",
                content_type: "text/plain; charset=utf-8",
                body: b"internal error\n".to_vec(),
            }
        }
    }
}

/// Map a file extension to a MIME content-type string.
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
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("map") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        _ => "application/octet-stream",
    }
}
