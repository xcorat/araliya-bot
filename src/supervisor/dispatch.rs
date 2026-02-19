//! Supervisor dispatch — generic trait for subsystem request handlers.
//!
//! Each subsystem implements [`BusHandler`] and registers with the supervisor
//! under its [`BusHandler::prefix`].  The supervisor routes incoming bus
//! messages to the matching handler without knowing the concrete type.
//!
//! # Method routing
//!
//! Method strings follow the form `"prefix/component/action"`.  The
//! supervisor extracts the first `/`-delimited segment and looks it up in its
//! handler table.  Everything after that first segment is passed verbatim as
//! `method` to the handler, so subsystems can do their own secondary routing.
//!
//! # Default notification handler
//!
//! `handle_notification` has a no-op default implementation; subsystems that
//! do not care about notifications need not override it.

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusPayload, BusResult};

/// A subsystem that can handle [`crate::supervisor::bus::BusMessage`]s.
///
/// Implementations must be `Send + Sync` so the supervisor can hold them
/// behind `Arc` or pass references into spawned tasks if needed.
pub trait BusHandler: Send + Sync {
    /// The method prefix this handler owns (e.g. `"agents"`, `"llm"`).
    ///
    /// Must be unique across all registered handlers.  The supervisor panics
    /// on startup if two handlers share the same prefix.
    fn prefix(&self) -> &str;

    /// Handle an incoming request, taking ownership of `reply_tx`.
    ///
    /// Implementations **must not block** the caller — either resolve
    /// `reply_tx` synchronously or move it into a `tokio::spawn` task.
    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    );

    /// Handle an incoming notification (fire-and-forget, no reply expected).
    ///
    /// Default: silently ignore.
    fn handle_notification(&self, _method: &str, _payload: BusPayload) {}
}
