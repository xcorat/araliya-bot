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

pub use crate::types::llm::StreamChunk;

// ── Streaming receiver wrapper ────────────────────────────────────────────────

/// A wrapper around [`tokio::sync::mpsc::Receiver<StreamChunk>`] returned by
/// `llm/stream` requests.
///
/// Serde impls are no-ops (this type is in-process only; the IPC migration
/// path would need a separate mechanism for streaming).
pub struct StreamReceiver(pub mpsc::Receiver<StreamChunk>);

impl Serialize for StreamReceiver {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for StreamReceiver {
    fn deserialize<D: serde::Deserializer<'de>>(_d: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom(
            "StreamReceiver cannot be deserialized (in-process only)",
        ))
    }
}

impl std::fmt::Debug for StreamReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamReceiver").finish_non_exhaustive()
    }
}

impl Clone for StreamReceiver {
    fn clone(&self) -> Self {
        // Receivers are not actually cloneable; this impl exists only because
        // `BusPayload` derives `Clone`. `LlmStreamResult` must never be cloned.
        panic!("StreamReceiver::clone called — LlmStreamResult must not be cloned");
    }
}

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
    CommsMessage {
        channel_id: String,
        content: String,
        session_id: Option<String>,
        /// Token usage reported by the LLM provider for this turn.
        /// `None` when the message did not originate from an LLM call,
        /// or when the provider does not report usage (e.g. dummy).
        #[serde(default)]
        usage: Option<crate::types::llm::LlmUsage>,
        /// Wall-clock latency for this turn's LLM call.
        /// `None` when the message did not come from an LLM call.
        #[serde(default)]
        timing: Option<crate::types::llm::LlmTiming>,
        /// Internal chain-of-thought produced by reasoning models (Qwen3, QwQ,
        /// DeepSeek-R1, …). `None` for standard models.
        #[serde(default)]
        thinking: Option<String>,
    },
    /// A completion request to the LLM subsystem.
    /// `channel_id` is threaded through so the LLM subsystem can attach it to
    /// the `CommsMessage` it returns, allowing the caller to re-associate the
    /// reply with the originating channel without extra bookkeeping.
    LlmRequest {
        channel_id: String,
        content: String,
        system: Option<String>,
    },
    /// Request tool execution in the tools subsystem.
    ToolRequest {
        tool: String,
        action: String,
        args_json: String,
        channel_id: String,
        session_id: Option<String>,
    },
    /// Structured tool execution reply.
    ToolResponse {
        tool: String,
        action: String,
        ok: bool,
        data_json: Option<String>,
        error: Option<String>,
    },
    /// Targets an in-flight request for cancellation.
    CancelRequest { id: Uuid },
    /// Query a specific session by ID.
    SessionQuery {
        session_id: String,
        #[serde(default)]
        agent_id: Option<String>,
    },
    /// Generic JSON response from a subsystem query.
    JsonResponse { data: String },

    // ── Cron ─────────────────────────────────────────────────────────────
    /// Schedule a recurring or one-shot event.
    /// Sent to `cron/schedule`. The cron service will emit `target_method`
    /// as a bus notification when the timer fires.
    CronSchedule {
        /// Bus method to emit when the timer fires (e.g. `"agents/cron/check-email"`).
        target_method: String,
        /// JSON-serialized `BusPayload` to attach to the emitted notification.
        payload_json: String,
        /// Scheduling specification.
        spec: CronScheduleSpec,
    },
    /// Reply to a successful `cron/schedule` request.
    CronScheduleResult { schedule_id: String },
    /// Cancel an active schedule by ID.
    CronCancel { schedule_id: String },
    /// Request a listing of all active schedules.
    CronList,
    /// Reply to `cron/list`.
    CronListResult { entries: Vec<CronEntryInfo> },

    /// A streaming message request routed through the agent pipeline.
    ///
    /// The agent runs its full instruction + tool pipeline and streams the
    /// final response.  Returns `LlmStreamResult` on success.
    CommsStreamRequest {
        channel_id: String,
        content: String,
        session_id: Option<String>,
    },

    /// Reply to an `llm/stream` request: the caller reads chunks from `rx`.
    ///
    /// The receiver is in-process only and not serializable; see [`StreamReceiver`].
    LlmStreamResult { rx: StreamReceiver },

    /// No payload — used by notifications whose meaning is in the method alone.
    Empty,
}

// ── Cron types ───────────────────────────────────────────────────────────────

/// How a scheduled event should repeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CronScheduleSpec {
    /// Fire once at a specific instant (ISO-8601 timestamp).
    Once { at_unix_ms: u64 },
    /// Fire repeatedly at a fixed interval.
    Interval { every_secs: u64 },
}

/// Summary of a single active schedule, returned by `cron/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntryInfo {
    pub schedule_id: String,
    pub target_method: String,
    pub spec: CronScheduleSpec,
    pub next_fire_unix_ms: u64,
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
        Self {
            code,
            message: message.into(),
        }
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
