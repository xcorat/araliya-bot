//! Shared state for the Comms subsystem — capability boundary for channels.
//!
//! Channels receive an `Arc<CommsState>` and are restricted to the typed
//! methods below.  The raw [`BusHandle`] is private; channels cannot call
//! arbitrary bus methods or supervisor internals.
//!
//! # Intra-subsystem events
//!
//! [`CommsState::report_event`] lets a running channel signal the comms
//! subsystem manager (e.g. "I shut down", "new session started") without
//! going through the supervisor bus.  The manager owns the receiver end.

use tokio::sync::mpsc;
use tracing::warn;

use crate::error::AppError;
use crate::supervisor::bus::{BusHandle, BusPayload};

#[derive(Debug, Clone)]
pub struct CommsReply {
    pub reply: String,
    pub session_id: Option<String>,
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Events a channel sends back to the comms subsystem manager.
#[derive(Debug)]
pub enum CommsEvent {
    /// Channel has stopped (clean exit or EOF).
    ChannelShutdown { channel_id: String },
    /// A new session/connection was established on the channel.
    // Not yet emitted; planned for HTTP and Telegram channels.
    SessionStarted { channel_id: String },
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Shared state passed as `Arc<CommsState>` to every channel task.
pub struct CommsState {
    /// Supervisor bus — private so channels can't call arbitrary methods.
    bus: BusHandle,
    /// Back-channel to the comms subsystem manager.
    event_tx: mpsc::Sender<CommsEvent>,
}

impl CommsState {
    pub fn new(bus: BusHandle, event_tx: mpsc::Sender<CommsEvent>) -> Self {
        Self { bus, event_tx }
    }

    /// Send `content` from `channel_id` to the agents subsystem and await the
    /// reply string.
    ///
    /// This is the primary outbound path for all comms channels.  Channels
    /// do not need to know about the supervisor bus protocol.
    pub async fn send_message(
        &self,
        channel_id: &str,
        content: String,
        session_id: Option<String>,
        agent_id: Option<String>,
    ) -> Result<CommsReply, AppError> {
        let method = match agent_id.as_deref() {
            Some(agent) if !agent.trim().is_empty() => format!("agents/{agent}"),
            _ => "agents".to_string(),
        };
        let payload = BusPayload::CommsMessage {
            channel_id: channel_id.to_string(),
            content,
            session_id,
            usage: None,
        };

        match self.bus.request(method, payload).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agent error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content: reply, session_id, .. })) => Ok(CommsReply {
                reply,
                session_id,
            }),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request management HTTP GET handling via the supervisor bus.
    ///
    /// Currently this is used by the comms HTTP channel for `/health`.
    pub async fn management_http_get(&self) -> Result<String, AppError> {
        match self.bus.request("manage/http/get", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected management reply payload".to_string())),
        }
    }

    /// Request the component tree (JSON) via the management bus route `manage/tree`.
    pub async fn management_component_tree(&self) -> Result<String, AppError> {
        match self.bus.request("manage/tree", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected management reply payload".to_string())),
        }
    }

    /// Trigger a live health refresh across all subsystems and return the updated
    /// health JSON (same format as [`Self::management_http_get`]).
    ///
    /// Sends `manage/health/refresh` on the bus, which fans out `{prefix}/health`
    /// to every registered subsystem concurrently (5 s timeout each), then
    /// returns the full health body with fresh data.
    pub async fn management_health_refresh(&self) -> Result<String, AppError> {
        match self.bus.request("manage/health/refresh", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected management reply payload".to_string())),
        }
    }

    /// Request the component tree for HTTP (GET /api/tree). Same data as `manage/tree`; no private data.
    pub async fn management_http_tree(&self) -> Result<String, AppError> {
        match self.bus.request("manage/http/tree", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected management reply payload".to_string())),
        }
    }

    /// Request a list of all sessions from the agents subsystem.
    pub async fn request_sessions(&self) -> Result<String, AppError> {
        match self.bus.request("agents/sessions", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request a list of all registered agents from the agents subsystem.
    pub async fn request_agents(&self) -> Result<String, AppError> {
        match self.bus.request("agents/list", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request detail (metadata + transcript) for a specific session.
    pub async fn request_session_detail(&self, session_id: &str, agent_id: Option<String>) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/sessions/detail",
                BusPayload::SessionQuery {
                    session_id: session_id.to_string(),
                    agent_id,
                },
            )
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request working-memory content for a specific session.
    pub async fn request_session_memory(&self, session_id: &str, agent_id: Option<String>) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/sessions/memory",
                BusPayload::SessionQuery {
                    session_id: session_id.to_string(),
                    agent_id,
                },
            )
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request session file list for a specific session.
    pub async fn request_session_files(&self, session_id: &str, agent_id: Option<String>) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/sessions/files",
                BusPayload::SessionQuery {
                    session_id: session_id.to_string(),
                    agent_id,
                },
            )
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Request the knowledge graph for a specific agent (from its kgdocstore).
    pub async fn request_agent_kg(&self, agent_id: &str) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/kg_graph",
                BusPayload::SessionQuery {
                    session_id: String::new(),
                    agent_id: Some(agent_id.to_string()),
                },
            )
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agents error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    /// Report an event to the comms subsystem manager.
    ///
    /// Non-blocking: drops the event and logs a warning if the manager is not
    /// keeping up (channel full) or has already exited (closed).
    pub fn report_event(&self, event: CommsEvent) {
        if let Err(e) = self.event_tx.try_send(event) {
            warn!("comms event dropped: {e}");
        }
    }
}
