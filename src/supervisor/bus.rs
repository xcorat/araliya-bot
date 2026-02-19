//! Supervisor event bus — typed message protocol between subsystems and the supervisor.
//!
//! Protocol follows JSON-RPC 2.0 semantics:
//!   - `BusMessage::Request`      — caller expects a reply (correlated via `oneshot`)
//!   - `BusMessage::Notification` — fire-and-forget, no reply expected
//!
//! Method names use `/`-separated namespaces: `"subsystem/component/action"`.
//! Reserved system methods are prefixed with `$/` (e.g. `"$/cancel"`).
//!
//! IPC migration path: when crossing a process boundary, remove `reply_tx` from
//! `Request`, store it in a `HashMap<Uuid, oneshot::Sender<BusResult>>` in the
//! supervisor, serialize `{ id, method, payload }` as JSON, and match responses
//! back by `id`. `BusHandle::request` is unchanged from the caller's perspective.

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

// ── Payload ──────────────────────────────────────────────────────────────────

/// All known message bodies. Add one variant per new message type.
///
/// Derives `Serialize + Deserialize` so every payload is IPC-ready without
/// any callsite changes when crossing a process boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusPayload {
    /// A raw text message from a comms channel (PTY, HTTP, etc.).
    /// `channel_id` identifies the originating instance (e.g. "pty0", "pty1")
    /// so the supervisor can maintain per-channel context. The return path is
    /// always the `reply_tx` in the enclosing `Request` — `channel_id` is for
    /// routing/logging only, not for sending the reply.
    CommsMessage { channel_id: String, content: String },
    /// A completion request to the LLM subsystem.
    /// `channel_id` is threaded through so the LLM subsystem can attach it to
    /// the `CommsMessage` it returns, allowing the caller to re-associate the
    /// reply with the originating channel without extra bookkeeping.
    LlmRequest { channel_id: String, content: String },
    /// Targets an in-flight request for cancellation.
    CancelRequest { id: Uuid },
    /// No payload — used by notifications whose meaning is in the method alone.
    Empty,
}

// ── Error ────────────────────────────────────────────────────────────────────

/// A structured error returned inside a `BusResult`.
/// Mirrors the JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusError {
    /// Application-defined error code.
    pub code: i32,
    /// Human-readable description.
    pub message: String,
}

impl BusError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

/// Method not found — mirrors JSON-RPC 2.0 error code -32601.
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;

pub type BusResult = Result<BusPayload, BusError>;

// ── Message ──────────────────────────────────────────────────────────────────

/// A message on the supervisor bus.
///
/// - `Request`: caller awaits a reply via the embedded `oneshot::Sender`.
///   The `!Clone` nature of `reply_tx` enforces single-recipient delivery at
///   compile time — it is impossible to accidentally reply to two handlers.
///
/// - `Notification`: fire-and-forget. The payload needs only `Send`; there is
///   no return path or ownership entanglement after delivery.
pub enum BusMessage {
    /// A request expecting exactly one reply.
    Request {
        /// Unique correlation ID. Becomes the wire `id` field when IPC is added.
        id: Uuid,
        /// Method path: `"subsystem/component/action"`. Reserved: `"$/<name>"`.
        method: String,
        /// Message body.
        payload: BusPayload,
        /// Pre-addressed return envelope. `!Clone` — moves to exactly one handler.
        reply_tx: oneshot::Sender<BusResult>,
    },
    /// A notification with no reply expected.
    Notification {
        /// Method path: `"subsystem/component/action"`. Reserved: `"$/<name>"`.
        method: String,
        /// Message body.
        payload: BusPayload,
    },
}

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
        self.tx
            .send(BusMessage::Request { id, method, payload, reply_tx })
            .await
            .map_err(|_| BusCallError::Send)?;
        reply_rx.await.map_err(|_| BusCallError::Recv)
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
        match self.tx.try_send(BusMessage::Notification { method, payload }) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Err(BusCallError::Full),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => Err(BusCallError::Send),
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
        let (tx, rx) = mpsc::channel(buffer);
        Self { rx, handle: BusHandle { tx } }
    }
}
