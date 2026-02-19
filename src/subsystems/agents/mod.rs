//! Agents subsystem — receives agent-targeted requests and routes to agents.
//!
//! `handle_request` never blocks the supervisor loop: synchronous agents
//! (echo) resolve `reply_tx` immediately; async agents (basic_chat) move
//! `reply_tx` into a spawned task and resolve it when work completes.

use std::collections::{HashMap, HashSet};

use tokio::sync::oneshot;

use crate::config::AgentsConfig;
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

/// Basic agents runtime with method-based routing.
///
/// Method grammar:
/// - `agents`                         -> default agent, default action
/// - `agents/{agent_id}`              -> explicit agent, default action
/// - `agents/{agent_id}/{action}`     -> explicit agent + action
pub struct AgentsSubsystem {
    enabled_agents: HashSet<String>,
    default_agent: String,
    channel_map: HashMap<String, String>,
    bus: BusHandle,
}

impl AgentsSubsystem {
    pub fn new(config: AgentsConfig, bus: BusHandle) -> Self {
        let mut enabled = config.enabled;
        if enabled.is_empty() {
            enabled.push("echo".to_string());
        }

        let default_agent = enabled[0].clone();
        let enabled_agents = enabled.into_iter().collect();

        Self {
            enabled_agents,
            default_agent,
            channel_map: config.channel_map,
            bus,
        }
    }

    /// Route a request. Ownership of `reply_tx` is forwarded to the agent
    /// handler — the supervisor loop returns immediately after this call.
    pub fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let (method_agent_id, _action) = match parse_method(method) {
            Ok(v) => v,
            Err(e) => { let _ = reply_tx.send(Err(e)); return; }
        };

        match payload {
            BusPayload::CommsMessage { channel_id, content } => {
                let agent_id = match self.resolve_agent(method_agent_id.as_deref(), &channel_id) {
                    Ok(id) => id,
                    Err(e) => { let _ = reply_tx.send(Err(e)); return; }
                };
                self.run_agent(agent_id, channel_id, content, reply_tx);
            }
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unsupported payload for method: {method}"),
                )));
            }
        }
    }

    fn resolve_agent(&self, method_agent_id: Option<&str>, channel_id: &str) -> Result<String, BusError> {
        if let Some(agent_id) = method_agent_id {
            return if self.enabled_agents.contains(agent_id) {
                Ok(agent_id.to_string())
            } else {
                Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("agent not found: {agent_id}"),
                ))
            };
        }

        if let Some(mapped_agent) = self.channel_map.get(channel_id)
            && self.enabled_agents.contains(mapped_agent)
        {
            return Ok(mapped_agent.clone());
        }

        Ok(self.default_agent.clone())
    }

    fn run_agent(&self, agent_id: String, channel_id: String, content: String, reply_tx: oneshot::Sender<BusResult>) {
        match agent_id.as_str() {
            "basic_chat" => {
                // Spawn so the supervisor loop is not blocked on the LLM round-trip.
                let bus = self.bus.clone();
                tokio::spawn(async move {
                    let result = bus
                        .request(
                            "llm/complete",
                            BusPayload::LlmRequest { channel_id: channel_id.clone(), content },
                        )
                        .await;
                    let bus_result = match result {
                        Ok(r) => r,
                        Err(e) => Err(BusError::new(-32000, e.to_string())),
                    };
                    let _ = reply_tx.send(bus_result);
                });
            }
            "echo" => {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage { channel_id, content }));
            }
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("agent not found: {agent_id}"),
                )));
            }
        }
    }
}

fn parse_method(method: &str) -> Result<(Option<String>, String), BusError> {
    let parts: Vec<&str> = method.split('/').collect();

    if parts.is_empty() || parts[0] != "agents" {
        return Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        ));
    }

    match parts.len() {
        1 => Ok((None, "handle".to_string())),
        2 => Ok((Some(parts[1].to_string()), "handle".to_string())),
        3 => Ok((Some(parts[1].to_string()), parts[2].to_string())),
        _ => Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;
    use crate::supervisor::bus::{BusMessage, SupervisorBus};

    fn echo_bus() -> (SupervisorBus, BusHandle) {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        (bus, handle)
    }

    #[tokio::test]
    async fn routes_to_default_agent_when_unmapped() {
        let (_bus, handle) = echo_bus();
        let cfg = AgentsConfig {
            enabled: vec!["echo".to_string()],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle);

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string() },
            tx,
        );

        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "hello"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn routes_by_channel_mapping() {
        let (_bus, handle) = echo_bus();
        let mut channel_map = HashMap::new();
        channel_map.insert("pty0".to_string(), "echo".to_string());

        let cfg = AgentsConfig {
            enabled: vec!["echo".to_string()],
            channel_map,
        };
        let agents = AgentsSubsystem::new(cfg, handle);

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "mapped".to_string() },
            tx,
        );

        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "mapped"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_unknown_agent_errors() {
        let (_bus, handle) = echo_bus();
        let cfg = AgentsConfig {
            enabled: vec!["echo".to_string()],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle);

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/unknown",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hi".to_string() },
            tx,
        );

        assert!(rx.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn empty_enabled_falls_back_to_echo() {
        let (_bus, handle) = echo_bus();
        let cfg = AgentsConfig {
            enabled: vec![],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle);

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "fallback".to_string() },
            tx,
        );

        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "fallback"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Verifies the full basic_chat -> llm/complete round-trip through the bus.
    /// A fake LLM responder runs concurrently and answers the spawned request.
    #[tokio::test]
    async fn basic_chat_routes_through_llm_subsystem() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;

        // Spawn a fake LLM responder that echoes with a marker prefix.
        tokio::spawn(async move {
            if let Some(BusMessage::Request { payload, reply_tx, .. }) = rx.recv().await {
                if let BusPayload::LlmRequest { channel_id, content } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("[fake] {content}"),
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            enabled: vec!["basic_chat".to_string()],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle);

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string() },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "[fake] hello"),
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
