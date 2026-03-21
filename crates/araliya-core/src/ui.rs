//! UI serve trait — shared interface between UI backends and comms channels.

use std::sync::Arc;

/// Response returned by [`UiServe::serve`].
pub struct ServeResponse {
    /// HTTP status line, e.g. `"200 OK"` or `"404 Not Found"`.
    pub status: &'static str,
    /// MIME content type, e.g. `"text/html; charset=utf-8"`.
    pub content_type: &'static str,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// A UI backend that can serve HTTP requests for static assets / pages.
///
/// Implementations capture their configuration (static dir, etc.) at
/// construction time.  [`UiServe::serve`] is called per-request by the HTTP
/// channel and must be cheap (no async, no bus round-trips).
pub trait UiServe: Send + Sync {
    /// Serve a request for `path` (e.g. `"/"`, `"/index.html"`, `"/assets/app.js"`).
    ///
    /// Returns `Some(response)` if this backend handles the path, or `None`
    /// to let the caller fall through to its own 404.
    fn serve(&self, path: &str) -> Option<ServeResponse>;
}

/// Shared handle passed to comms channels.
pub type UiServeHandle = Arc<dyn UiServe>;
