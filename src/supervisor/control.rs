//! Supervisor-internal control plane.
//!
//! This interface is intentionally separate from the supervisor bus. It is
//! designed as a thin internal control core that transport adapters (stdio,
//! HTTP, etc.) can target without going through bus method routing.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

/// Control command set owned by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlCommand {
    Health,
    Status,
    SubsystemsList,
    SubsystemEnable { id: String },
    SubsystemDisable { id: String },
    Shutdown,
}

/// Control plane response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlResponse {
    Health { uptime_ms: u64 },
    Status { uptime_ms: u64, handlers: Vec<String> },
    Subsystems { handlers: Vec<String> },
    Ack { message: String },
}

/// Control plane error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlError {
    NotImplemented { message: String },
    Invalid { message: String },
}

pub type ControlResult = Result<ControlResponse, ControlError>;

/// Internal messages delivered to the supervisor control loop.
#[derive(Debug)]
pub enum ControlMessage {
    Request {
        command: ControlCommand,
        reply_tx: oneshot::Sender<ControlResult>,
    },
    Notification {
        command: ControlCommand,
    },
}

/// Client-facing handle for the control plane.
#[derive(Clone)]
pub struct ControlHandle {
    tx: mpsc::Sender<ControlMessage>,
}

impl ControlHandle {
    pub fn new(tx: mpsc::Sender<ControlMessage>) -> Self {
        Self { tx }
    }

    pub async fn request(&self, command: ControlCommand) -> Result<ControlResult, ControlCallError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ControlMessage::Request { command, reply_tx })
            .await
            .map_err(|_| ControlCallError::Send)?;

        reply_rx.await.map_err(|_| ControlCallError::Recv)
    }

    pub fn notify(&self, command: ControlCommand) -> Result<(), ControlCallError> {
        self.tx
            .try_send(ControlMessage::Notification { command })
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => ControlCallError::Full,
                mpsc::error::TrySendError::Closed(_) => ControlCallError::Send,
            })
    }
}

/// Owns the supervisor-side control receiver.
pub struct SupervisorControl {
    pub rx: mpsc::Receiver<ControlMessage>,
    pub handle: ControlHandle,
}

impl SupervisorControl {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self {
            rx,
            handle: ControlHandle::new(tx),
        }
    }
}

/// Call-level transport errors for control requests/notifications.
#[derive(Debug)]
pub enum ControlCallError {
    Send,
    Recv,
    Full,
}

impl fmt::Display for ControlCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlCallError::Send => write!(f, "control send failed: supervisor is not running"),
            ControlCallError::Recv => write!(f, "control recv failed: supervisor dropped reply sender"),
            ControlCallError::Full => write!(f, "control queue full"),
        }
    }
}

impl Error for ControlCallError {}

/// Wire-format response envelope for socket transports.
///
/// Serialises as `{"ok": <response>}` or `{"err": <error>}`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum WireResponse {
    #[serde(rename = "ok")]
    Ok(ControlResponse),
    #[serde(rename = "err")]
    Err(ControlError),
}

impl From<ControlResult> for WireResponse {
    fn from(r: ControlResult) -> Self {
        match r {
            Ok(resp) => WireResponse::Ok(resp),
            Err(err) => WireResponse::Err(err),
        }
    }
}
