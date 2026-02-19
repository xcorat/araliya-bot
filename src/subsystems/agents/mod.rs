//! Agents subsystem — receives agent-targeted requests and routes to plugins.
//!
//! [`AgentPlugin`] is the extension point: each plugin is a `Send + Sync`
//! struct registered in the subsystem by name.  Built-in plugins (`echo`,
//! `basic_chat`) live in this module.  Third-party plugins can be added later.
//!
//! [`AgentsSubsystem`] implements [`BusHandler`] with prefix `"agents"` and
//! is never blocked: sync plugins resolve immediately, async ones spawn tasks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::config::AgentsConfig;
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

// ── AgentsState ───────────────────────────────────────────────────────────────

/// Shared capability surface passed to agent plugins.
///
/// The raw [`BusHandle`] is private — plugins call typed methods and cannot
/// address arbitrary bus targets.
pub struct AgentsState {
    /// Supervisor bus — private to this module.
    bus: BusHandle,
}

impl AgentsState {
    fn new(bus: BusHandle) -> Self {
        Self { bus }
    }

    /// Forward content to the LLM subsystem and return the completion.
    pub async fn complete_via_llm(
        &self,
        channel_id: &str,
        content: &str,
    ) -> BusResult {
        let result = self
            .bus
            .request(
                "llm/complete",
                BusPayload::LlmRequest {
                    channel_id: channel_id.to_string(),
                    content: content.to_string(),
                },
            )
            .await;
        match result {
            Ok(r) => r,
            Err(e) => Err(BusError::new(-32000, e.to_string())),
        }
    }
}

// ── AgentPlugin ───────────────────────────────────────────────────────────────

/// A plugin loaded by the agents subsystem.
///
/// Implementations must be `Send + Sync` and must not block the caller:
/// synchronous work resolves `reply_tx` immediately; async work spawns a task
/// and resolves it when done.
pub trait AgentPlugin: Send + Sync {
    /// Unique plugin identifier (matches config name, e.g. `"echo"`).
    fn id(&self) -> &str;

    /// Handle an incoming request.
    fn handle(
        &self,
        channel_id: String,
        content: String,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}

// ── Built-in plugins ──────────────────────────────────────────────────────────

struct EchoPlugin;

impl AgentPlugin for EchoPlugin {
    fn id(&self) -> &str { "echo" }
    fn handle(&self, channel_id: String, content: String, reply_tx: oneshot::Sender<BusResult>, _state: Arc<AgentsState>) {
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage { channel_id, content }));
    }
}

struct BasicChatPlugin;

impl AgentPlugin for BasicChatPlugin {
    fn id(&self) -> &str { "basic_chat" }
    fn handle(&self, channel_id: String, content: String, reply_tx: oneshot::Sender<BusResult>, state: Arc<AgentsState>) {
        // Spawn so the supervisor loop is not blocked on the LLM round-trip.
        tokio::spawn(async move {
            let result = state.complete_via_llm(&channel_id, &content).await;
            let _ = reply_tx.send(result);
        });
    }
}

// ── AgentsSubsystem ───────────────────────────────────────────────────────────

/// Agents subsystem.
///
/// Method grammar:
/// - `agents`                         -> default agent, default action
/// - `agents/{agent_id}`              -> explicit agent, default action
/// - `agents/{agent_id}/{action}`     -> explicit agent + action
pub struct AgentsSubsystem {
    state: Arc<AgentsState>,
    plugins: HashMap<String, Box<dyn AgentPlugin>>,
    default_agent: String,
    channel_map: HashMap<String, String>,
    enabled_agents: HashSet<String>,
}

impl AgentsSubsystem {
    pub fn new(config: AgentsConfig, bus: BusHandle) -> Self {
        // Default falls back to "echo" if config omits the default entirely.
        let default_agent = if config.default_agent.is_empty() {
            "echo".to_string()
        } else {
            config.default_agent
        };
        let enabled_agents = config.enabled;

        // Register all known built-in plugins.
        // Uses plugin.id() as the HashMap key so the trait method is the
        // single source of truth for each plugin's identity.
        let mut plugins: HashMap<String, Box<dyn AgentPlugin>> = HashMap::new();
        for plugin in [Box::new(EchoPlugin) as Box<dyn AgentPlugin>, Box::new(BasicChatPlugin)] {
            plugins.insert(plugin.id().to_string(), plugin);
        }

        Self {
            state: Arc::new(AgentsState::new(bus)),
            plugins,
            default_agent,
            channel_map: config.channel_map,
            enabled_agents,
        }
    }

    fn resolve_agent<'a>(&'a self, method_agent_id: Option<&'a str>, channel_id: &str) -> Result<&'a str, BusError> {
        if let Some(agent_id) = method_agent_id {
            return if self.enabled_agents.contains(agent_id) {
                Ok(agent_id)
            } else {
                Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("agent not found: {agent_id}"),
                ))
            };
        }

        if let Some(mapped) = self.channel_map.get(channel_id)
            && self.enabled_agents.contains(mapped)
        {
            return Ok(mapped.as_str());
        }

        Ok(self.default_agent.as_str())
    }
}

impl BusHandler for AgentsSubsystem {
    fn prefix(&self) -> &str {
        "agents"
    }

    /// Route a request. Ownership of `reply_tx` is forwarded to the plugin —
    /// the supervisor loop returns immediately after this call.
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
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
                match self.plugins.get(agent_id) {
                    Some(plugin) => plugin.handle(channel_id, content, reply_tx, self.state.clone()),
                    None => {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_METHOD_NOT_FOUND,
                            format!("agent plugin not loaded: {agent_id}"),
                        )));
                    }
                }
            }
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unsupported payload for method: {method}"),
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
    use crate::supervisor::dispatch::BusHandler;

    fn echo_bus() -> (SupervisorBus, BusHandle) {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        (bus, handle)
    }

    #[tokio::test]
    async fn routes_to_default_agent_when_unmapped() {
        let (_bus, handle) = echo_bus();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
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
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
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
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
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
            default_agent: "echo".to_string(),
            enabled: HashSet::new(),
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
            default_agent: "basic_chat".to_string(),
            enabled: HashSet::from(["basic_chat".to_string()]),
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
