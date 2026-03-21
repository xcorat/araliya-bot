//! Bus handle — the cloneable sender surface that subsystems and plugins touch.

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};
use uuid::Uuid;

use super::message::{BusMessage, BusPayload, BusResult};

// ── Call error ───────────────────────────────────────────────────────────────

/// Error returned by `BusHandle::request` or `BusHandle::notify`.
#[derive(Debug)]
pub enum BusCallError {
    /// The supervisor's mpsc receiver was dropped (supervisor is dead).
    Send,
    /// The supervisor dropped `reply_tx` without sending a reply.
    Recv,
    /// The bus buffer is full — notification was dropped (only possible via `notify`).
    Full,
}

impl std::fmt::Display for BusCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusCallError::Send => write!(f, "bus send failed: supervisor is not running"),
            BusCallError::Recv => write!(f, "bus recv failed: supervisor dropped reply sender"),
            BusCallError::Full => write!(f, "bus full: notification dropped (back-pressure)"),
        }
    }
}

impl std::error::Error for BusCallError {}

// ── Handle ───────────────────────────────────────────────────────────────────

/// A cloneable sender handle — the only surface subsystems and plugins touch.
/// Raw channel types are not exposed outside this module.
#[derive(Clone)]
pub struct BusHandle {
    tx: mpsc::Sender<BusMessage>,
}

impl BusHandle {
    /// Send a request and wait for exactly one reply.
    pub async fn request(
        &self,
        method: impl Into<String>,
        payload: BusPayload,
    ) -> Result<BusResult, BusCallError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let id = Uuid::new_v4();
        let method = method.into();
        debug!(%id, %method, "bus: sending request");
        trace!(%id, %method, payload = ?payload, "bus: request payload");
        self.tx
            .send(BusMessage::Request {
                id,
                method,
                payload,
                reply_tx,
            })
            .await
            .map_err(|_| {
                warn!(%id, "bus: request send failed — supervisor not running");
                BusCallError::Send
            })?;
        let result = reply_rx.await.map_err(|_| {
            warn!(%id, "bus: reply channel dropped — supervisor did not reply");
            BusCallError::Recv
        })?;
        debug!(%id, ok = result.is_ok(), "bus: request completed");
        trace!(%id, result = ?result, "bus: request result");
        Ok(result)
    }

    /// Send a notification with no reply expected. Non-blocking.
    ///
    /// Uses `try_send` — if the bus buffer is full the notification is dropped
    /// and `Err(BusCallError::Full)` is returned. Callers should log a warning
    /// but must not block or retry: notifications are intentionally lossy under
    /// back-pressure. If you need guaranteed delivery, use `request` instead.
    pub fn notify(
        &self,
        method: impl Into<String>,
        payload: BusPayload,
    ) -> Result<(), BusCallError> {
        let method = method.into();
        debug!(%method, "bus: sending notification");
        trace!(%method, payload = ?payload, "bus: notification payload");
        match self
            .tx
            .try_send(BusMessage::Notification { method, payload })
        {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(msg)) => {
                let method = match msg {
                    BusMessage::Notification { ref method, .. } => method.as_str(),
                    _ => "<unknown>",
                };
                warn!(%method, "bus: notification dropped — buffer full (back-pressure)");
                Err(BusCallError::Full)
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(msg)) => {
                let method = match msg {
                    BusMessage::Notification { ref method, .. } => method.as_str(),
                    _ => "<unknown>",
                };
                warn!(%method, "bus: notification send failed — supervisor not running");
                Err(BusCallError::Send)
            }
        }
    }
}

// ── Bus ──────────────────────────────────────────────────────────────────────

/// Owns the supervisor-side receiver. Created once at startup; the `handle`
/// is cloned and distributed to subsystems before `rx` is moved into the
/// supervisor task.
pub struct SupervisorBus {
    /// Supervisor receives all inbound messages here.
    pub rx: mpsc::Receiver<BusMessage>,
    /// Cloneable handle distributed to subsystems and plugins.
    pub handle: BusHandle,
}

impl SupervisorBus {
    pub fn new(buffer: usize) -> Self {
        debug!(buffer, "supervisor bus created");
        let (tx, rx) = mpsc::channel(buffer);
        Self {
            rx,
            handle: BusHandle { tx },
        }
    }
}
