//! Shared state for the Comms subsystem — capability boundary for channels.

use tokio::sync::mpsc;
use tracing::warn;

use araliya_core::bus::{BusHandle, BusPayload, StreamReceiver};
use araliya_core::error::AppError;
use araliya_core::types::llm::StreamChunk;

#[derive(Debug, Clone)]
pub struct CommsReply {
    pub reply: String,
    pub session_id: Option<String>,
    pub thinking: Option<String>,
    pub usage: Option<araliya_core::types::llm::LlmUsage>,
    pub timing: Option<araliya_core::types::llm::LlmTiming>,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum CommsEvent {
    ChannelShutdown { channel_id: String },
    SessionStarted { channel_id: String },
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct CommsState {
    bus: BusHandle,
    event_tx: mpsc::Sender<CommsEvent>,
}

impl CommsState {
    pub fn new(bus: BusHandle, event_tx: mpsc::Sender<CommsEvent>) -> Self {
        Self { bus, event_tx }
    }

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
            timing: None,
            thinking: None,
        };

        match self.bus.request(method, payload).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agent error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage {
                content: reply,
                session_id,
                thinking,
                usage,
                timing,
                ..
            })) => Ok(CommsReply {
                reply,
                session_id,
                thinking,
                usage,
                timing,
            }),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    pub async fn stream_via_agent(
        &self,
        channel_id: &str,
        content: String,
        session_id: Option<String>,
        agent_id: Option<String>,
    ) -> Result<mpsc::Receiver<StreamChunk>, AppError> {
        let method = match agent_id.as_deref() {
            Some(agent) if !agent.trim().is_empty() => format!("agents/{agent}"),
            _ => "agents".to_string(),
        };
        let payload = BusPayload::CommsStreamRequest {
            channel_id: channel_id.to_string(),
            content,
            session_id,
        };
        match self.bus.request(method, payload).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "agent stream error: {}",
                e.message
            ))),
            Ok(Ok(BusPayload::LlmStreamResult {
                rx: StreamReceiver(rx),
            })) => Ok(rx),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected reply to agent stream request".to_string(),
            )),
        }
    }

    pub async fn stream_direct(
        &self,
        channel_id: &str,
        content: String,
        system: Option<String>,
    ) -> Result<mpsc::Receiver<StreamChunk>, AppError> {
        let result = self
            .bus
            .request(
                "llm/stream",
                BusPayload::LlmRequest {
                    channel_id: channel_id.to_string(),
                    content,
                    system,
                    provider_override: None,
                    model_override: None,
                },
            )
            .await;
        match result {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!("llm stream error: {}", e.message))),
            Ok(Ok(BusPayload::LlmStreamResult {
                rx: StreamReceiver(rx),
            })) => Ok(rx),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected reply to llm/stream".to_string(),
            )),
        }
    }

    pub async fn management_http_get(&self) -> Result<String, AppError> {
        match self.bus.request("manage/http/get", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

    pub async fn management_component_tree(&self) -> Result<String, AppError> {
        match self.bus.request("manage/tree", BusPayload::Empty).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

    pub async fn management_health_refresh(&self) -> Result<String, AppError> {
        match self
            .bus
            .request("manage/health/refresh", BusPayload::Empty)
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

    pub async fn management_http_tree(&self) -> Result<String, AppError> {
        match self
            .bus
            .request("manage/http/tree", BusPayload::Empty)
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::CommsMessage { content, .. })) => Ok(content),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

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

    pub async fn request_agent_session(&self, agent_id: &str) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/session",
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

    pub async fn request_agent_spend(&self, agent_id: &str) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/spend",
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

    pub async fn request_llm_providers(&self) -> Result<String, AppError> {
        match self
            .bus
            .request("llm/list_providers", BusPayload::Empty)
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "llm error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    pub async fn set_llm_default(&self, provider: &str) -> Result<String, AppError> {
        let payload = BusPayload::JsonRequest {
            data: serde_json::json!({ "provider": provider }).to_string(),
        };
        match self.bus.request("llm/set_default", payload).await {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "llm error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    pub async fn request_session_detail(
        &self,
        session_id: &str,
        agent_id: Option<String>,
    ) -> Result<String, AppError> {
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

    pub async fn request_session_memory(
        &self,
        session_id: &str,
        agent_id: Option<String>,
    ) -> Result<String, AppError> {
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

    pub async fn request_session_files(
        &self,
        session_id: &str,
        agent_id: Option<String>,
    ) -> Result<String, AppError> {
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

    pub async fn request_session_debug(
        &self,
        session_id: &str,
        agent_id: Option<String>,
    ) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "agents/sessions/debug",
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

    pub async fn request_memory_kg(&self, agent_id: &str) -> Result<String, AppError> {
        match self
            .bus
            .request(
                "memory/kg_graph",
                BusPayload::SessionQuery {
                    session_id: String::new(),
                    agent_id: Some(agent_id.to_string()),
                },
            )
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "memory error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms("unexpected reply payload".to_string())),
        }
    }

    pub async fn management_observe_snapshot(&self) -> Result<String, AppError> {
        match self
            .bus
            .request("manage/observe/snapshot", BusPayload::Empty)
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

    pub async fn management_observe_clear(&self) -> Result<String, AppError> {
        match self
            .bus
            .request("manage/observe/clear", BusPayload::Empty)
            .await
        {
            Err(e) => Err(AppError::Comms(format!("bus error: {e}"))),
            Ok(Err(e)) => Err(AppError::Comms(format!(
                "management error {}: {}",
                e.code, e.message
            ))),
            Ok(Ok(BusPayload::JsonResponse { data })) => Ok(data),
            Ok(Ok(_)) => Err(AppError::Comms(
                "unexpected management reply payload".to_string(),
            )),
        }
    }

    pub fn report_event(&self, event: CommsEvent) {
        if let Err(e) = self.event_tx.try_send(event) {
            warn!("comms event dropped: {e}");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comms_reply_fields_accessible() {
        let r = CommsReply {
            reply: "hi".to_string(),
            session_id: Some("s1".to_string()),
            thinking: None,
            usage: None,
            timing: None,
        };
        assert_eq!(r.reply, "hi");
        assert_eq!(r.session_id.as_deref(), Some("s1"));
        assert!(r.thinking.is_none());
    }

    #[test]
    fn comms_event_channel_shutdown_variant() {
        let ev = CommsEvent::ChannelShutdown {
            channel_id: "pty0".to_string(),
        };
        assert!(matches!(ev, CommsEvent::ChannelShutdown { channel_id } if channel_id == "pty0"));
    }

    #[test]
    fn comms_event_session_started_variant() {
        let ev = CommsEvent::SessionStarted {
            channel_id: "axum0".to_string(),
        };
        assert!(matches!(ev, CommsEvent::SessionStarted { channel_id } if channel_id == "axum0"));
    }

    #[test]
    fn report_event_drops_gracefully_when_channel_closed() {
        let sbus = araliya_core::bus::SupervisorBus::new(1);
        let bus = sbus.handle;
        let (ev_tx, ev_rx) = tokio::sync::mpsc::channel(1);
        let state = CommsState::new(bus, ev_tx);
        // Drop the receiver so the channel is closed.
        drop(ev_rx);
        // Should not panic.
        state.report_event(CommsEvent::ChannelShutdown {
            channel_id: "pty0".to_string(),
        });
    }
}
