//! Agents subsystem — receives agent-targeted requests and routes to agents.
//!
//! [`Agent`] is the extension trait: each agent is a `Send + Sync` struct
//! registered in the subsystem by name.  Built-in agents (`echo`,
//! `basic_chat`, `chat`) live in this module.  Third-party agents can be
//! added later.
//!
//! Chat-family agents share logic through the [`chat::core::ChatCore`]
//! composition layer — see the `chat/` submodule.
//!
//! [`AgentsSubsystem`] implements [`BusHandler`] with prefix `"agents"` and
//! is never blocked: sync agents resolve immediately, async ones spawn tasks.

use std::collections::{HashMap, HashSet};
//TODO: check if we should be using more fine-grained locks.
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::config::AgentsConfig;
use crate::error::AppError;
use crate::llm::ModelRates;
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

use crate::identity::{self, Identity};
use crate::subsystems::memory::MemorySystem;

// Chat-family plugins (basic_chat, session_chat) and shared ChatCore.
#[cfg(any(feature = "plugin-basic-chat", feature = "plugin-chat"))]
mod chat;
#[cfg(feature = "plugin-gmail-agent")]
mod gmail;
#[cfg(feature = "plugin-news-agent")]
mod news;

// ── AgentsState ───────────────────────────────────────────────────────────────

/// Shared capability surface passed to agent plugins.
///
/// The raw [`BusHandle`] is private — plugins call typed methods and cannot
/// address arbitrary bus targets.
pub struct AgentsState {
    /// Supervisor bus — private to this module.
    bus: BusHandle,
    /// Memory system — always present.  Agents create or load sessions via this handle.
    pub memory: Arc<MemorySystem>,
    /// Per-agent memory store requirements from config.
    pub agent_memory: HashMap<String, Vec<String>>,
    /// Cryptographic identities for each registered agent.
    pub agent_identities: HashMap<String, Identity>,
    /// Pricing rates for the active LLM model — used to compute per-session spend.
    pub llm_rates: ModelRates,
}

impl AgentsState {
    fn new(
        bus: BusHandle,
        memory: Arc<MemorySystem>,
        agent_memory: HashMap<String, Vec<String>>,
        agent_identities: HashMap<String, Identity>,
    ) -> Self {
        Self { bus, memory, agent_memory, agent_identities, llm_rates: ModelRates::default() }
    }

    /// Get or create a subagent identity under the parent agent's memory directory.
    ///
    /// Subagents are ephemeral or task-specific workers that operate under their parent's
    /// identity structure. Their keys are stored in `memory/agent/{agent_name}-{pkhash}/subagents/{subagent_name}-{pkhash}/`.
    pub fn get_or_create_subagent(&self, agent_id: &str, subagent_name: &str) -> Result<Identity, AppError> {
        let agent_identity = self.agent_identities.get(agent_id).ok_or_else(|| {
            AppError::Identity(format!("agent '{}' not found", agent_id))
        })?;
        let subagents_dir = agent_identity.identity_dir.join("subagents");
        identity::setup_named_identity(&subagents_dir, subagent_name)
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

    /// Execute a tool through the tools subsystem.
    pub async fn execute_tool(
        &self,
        tool: &str,
        action: &str,
        args_json: String,
        channel_id: &str,
        session_id: Option<String>,
    ) -> BusResult {
        let result = self
            .bus
            .request(
                "tools/execute",
                BusPayload::ToolRequest {
                    tool: tool.to_string(),
                    action: action.to_string(),
                    args_json,
                    channel_id: channel_id.to_string(),
                    session_id,
                },
            )
            .await;

        match result {
            Ok(r) => r,
            Err(e) => Err(BusError::new(-32000, e.to_string())),
        }
    }
}

// ── Agent trait ───────────────────────────────────────────────────────────────

/// An agent loaded by the agents subsystem.
///
/// Implementations must be `Send + Sync` and must not block the caller:
/// synchronous work resolves `reply_tx` immediately; async work spawns a task
/// and resolves it when done.
pub trait Agent: Send + Sync {
    /// Unique agent identifier (matches config name, e.g. `"echo"`).
    fn id(&self) -> &str;

    /// Handle an incoming request.
    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}

// ── Built-in agents ───────────────────────────────────────────────────────────

#[cfg(feature = "plugin-echo")]
struct EchoAgent;

#[cfg(feature = "plugin-echo")]
impl Agent for EchoAgent {
    fn id(&self) -> &str { "echo" }
    fn handle(&self, _action: String, channel_id: String, content: String, session_id: Option<String>, reply_tx: oneshot::Sender<BusResult>, _state: Arc<AgentsState>) {
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage { channel_id, content, session_id, usage: None }));
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
    agents: HashMap<String, Box<dyn Agent>>,
    default_agent: String,
    channel_map: HashMap<String, String>,
    enabled_agents: HashSet<String>,
}

impl AgentsSubsystem {
    pub fn new(
        config: AgentsConfig,
        bus: BusHandle,
        memory: Arc<MemorySystem>,
    ) -> Result<Self, AppError> {
        // Default falls back to "echo" if config omits the default entirely.
        let default_agent = if config.default_agent.is_empty() {
            "echo".to_string()
        } else {
            config.default_agent
        };
        let enabled_agents = config.enabled;
        let agent_memory = config.agent_memory;

        // Register all known built-in agents.
        // Uses agent.id() as the HashMap key so the trait method is the
        // single source of truth for each agent's identity.
        let mut agents: HashMap<String, Box<dyn Agent>> = HashMap::new();

        #[cfg(feature = "plugin-echo")]
        {
            let agent: Box<dyn Agent> = Box::new(EchoAgent);
            agents.insert(agent.id().to_string(), agent);
        }

        #[cfg(feature = "plugin-basic-chat")]
        {
            let agent: Box<dyn Agent> = Box::new(chat::BasicChatPlugin);
            agents.insert(agent.id().to_string(), agent);
        }

        #[cfg(feature = "plugin-chat")]
        {
            let agent: Box<dyn Agent> = Box::new(chat::SessionChatPlugin::new());
            agents.insert(agent.id().to_string(), agent);
        }

        #[cfg(feature = "plugin-gmail-agent")]
        {
            let agent: Box<dyn Agent> = Box::new(gmail::GmailAgentPlugin);
            agents.insert(agent.id().to_string(), agent);
        }

        #[cfg(feature = "plugin-news-agent")]
        {
            let agent: Box<dyn Agent> = Box::new(news::NewsAgentPlugin);
            agents.insert(agent.id().to_string(), agent);
        }

        // Initialize cryptographic identities for all registered agents.
        let mut agent_identities = HashMap::new();
        let agent_memory_root = memory.memory_root().join("agent");
        for agent_id in agents.keys() {
            let identity = identity::setup_named_identity(&agent_memory_root, agent_id)?;
            agent_identities.insert(agent_id.clone(), identity);
        }

        Ok(Self {
            state: Arc::new(AgentsState::new(bus, memory, agent_memory, agent_identities)),
            agents,
            default_agent,
            channel_map: config.channel_map,
            enabled_agents,
        })
    }

    /// Set the LLM pricing rates on the shared state.
    /// Call this after `new()` when rates are available from config.
    pub fn with_llm_rates(mut self, rates: ModelRates) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("AgentsState Arc must be exclusive at build time")
            .llm_rates = rates;
        self
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

        // Use the default agent only if it is enabled, or if no agents have
        // been explicitly enabled (empty set = no restrictions, for backward
        // compat and minimal / test configurations).
        if self.enabled_agents.is_empty() || self.enabled_agents.contains(&self.default_agent) {
            return Ok(self.default_agent.as_str());
        }

        Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("default agent '{}' is not enabled", self.default_agent),
        ))
    }

    // ── Session query handlers ─────────────────────────────────────────────

    /// Handle `agents/sessions` — return a JSON list of all sessions.
    fn handle_session_list(&self, reply_tx: oneshot::Sender<BusResult>) {
        let memory = self.state.memory.clone();

        let sessions = match memory.list_sessions() {
            Ok(s) => s,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("memory error: {e}"))));
                return;
            }
        };

        let body = serde_json::json!({
            "sessions": sessions.iter().map(|s| serde_json::json!({
                "session_id": s.session_id,
                "created_at": s.created_at,
                "store_types": s.store_types,
                "last_agent": s.last_agent,
            })).collect::<Vec<_>>()
        });

        let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
            data: body.to_string(),
        }));
    }

    /// Handle `agents/sessions/detail` — return session metadata + transcript.
    fn handle_session_detail(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let session_id = match payload {
            BusPayload::SessionQuery { session_id } => session_id,
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let memory = self.state.memory.clone();

        let handle = match memory.load_session(&session_id, None) {
            Ok(h) => h,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

        // Read transcript asynchronously and reply.
        tokio::spawn(async move {
            let transcript = match handle.transcript_read_last(1000).await {
                Ok(entries) => entries
                    .into_iter()
                    .map(|e| {
                        serde_json::json!({
                            "role": e.role,
                            "timestamp": e.timestamp,
                            "content": e.content,
                        })
                    })
                    .collect::<Vec<_>>(),
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!("transcript read failed: {e}"),
                    )));
                    return;
                }
            };

            let session_usage_totals = match handle.read_spend().await {
                Ok(spend) => spend.map(|spend| {
                    serde_json::json!({
                        "prompt_tokens": spend.total_input_tokens + spend.total_cached_tokens,
                        "completion_tokens": spend.total_output_tokens,
                        "total_tokens": spend.total_input_tokens + spend.total_cached_tokens + spend.total_output_tokens,
                        "estimated_cost_usd": spend.total_cost_usd,
                    })
                }),
                Err(_) => None,
            };

            let body = serde_json::json!({
                "session_id": session_id,
                "transcript": transcript,
                "session_usage_totals": session_usage_totals,
            });

            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: body.to_string(),
            }));
        });
    }

    /// Handle `agents/sessions/memory` — return working memory content.
    fn handle_session_memory(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let session_id = match payload {
            BusPayload::SessionQuery { session_id } => session_id,
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let memory = self.state.memory.clone();

        let handle = match memory.load_session(&session_id, None) {
            Ok(h) => h,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

        tokio::spawn(async move {
            let content = match handle.working_memory_read().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!("working memory read failed: {e}"),
                    )));
                    return;
                }
            };

            let body = serde_json::json!({
                "session_id": session_id,
                "content": content,
            });

            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: body.to_string(),
            }));
        });
    }

    /// Handle `agents/sessions/files` — return files in the session directory.
    fn handle_session_files(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let session_id = match payload {
            BusPayload::SessionQuery { session_id } => session_id,
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let memory = self.state.memory.clone();

        let handle = match memory.load_session(&session_id, None) {
            Ok(h) => h,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

        tokio::spawn(async move {
            let files = match handle.list_files().await {
                Ok(files) => files,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!("session file listing failed: {e}"),
                    )));
                    return;
                }
            };

            let body = serde_json::json!({
                "session_id": session_id,
                "files": files.into_iter().map(|f| serde_json::json!({
                    "name": f.name,
                    "size_bytes": f.size_bytes,
                    "modified": f.modified,
                })).collect::<Vec<_>>(),
            });

            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: body.to_string(),
            }));
        });
    }
}

impl BusHandler for AgentsSubsystem {
    fn prefix(&self) -> &str {
        "agents"
    }

    /// Route a request. Ownership of `reply_tx` is forwarded to the agent —
    /// the supervisor loop returns immediately after this call.
    ///
    /// Session queries (`agents/sessions`, `agents/sessions/detail`,
    /// `agents/sessions/memory`, `agents/sessions/files`) are
    /// intercepted before agent routing to return session metadata.
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        // ── Session queries (intercepted before agent routing) ──────
        if method == "agents/sessions" {
            self.handle_session_list(reply_tx);
            return;
        }
        if method == "agents/sessions/detail" {
            self.handle_session_detail(payload, reply_tx);
            return;
        }
        if method == "agents/sessions/memory" {
            self.handle_session_memory(payload, reply_tx);
            return;
        }
        if method == "agents/sessions/files" {
            self.handle_session_files(payload, reply_tx);
            return;
        }

        // ── Agent routing ───────────────────────────────────────────
        let (method_agent_id, action) = match parse_method(method) {
            Ok(v) => v,
            Err(e) => { let _ = reply_tx.send(Err(e)); return; }
        };

        match payload {
            BusPayload::CommsMessage { channel_id, content, session_id, .. } => {
                let agent_id = match self.resolve_agent(method_agent_id.as_deref(), &channel_id) {
                    Ok(id) => id,
                    Err(e) => { let _ = reply_tx.send(Err(e)); return; }
                };
                match self.agents.get(agent_id) {
                    Some(agent) => agent.handle(action, channel_id, content, session_id, reply_tx, self.state.clone()),
                    None => {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_METHOD_NOT_FOUND,
                            format!("agent not loaded: {agent_id}"),
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

    /// Create a throwaway `MemorySystem` backed by a temporary directory.
    /// The returned `TempDir` must be kept alive for the duration of the test.
    fn test_memory() -> (tempfile::TempDir, Arc<MemorySystem>) {
        let dir = tempfile::TempDir::new().unwrap();
        let mem = MemorySystem::new(dir.path(), crate::subsystems::memory::MemoryConfig::default()).unwrap();
        (dir, Arc::new(mem))
    }

    #[tokio::test]
    async fn routes_to_default_agent_when_unmapped() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string(), session_id: None, usage: None },
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
        let (_dir, memory) = test_memory();
        let mut channel_map = HashMap::new();
        channel_map.insert("pty0".to_string(), "echo".to_string());

        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map,
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "mapped".to_string(), session_id: None, usage: None },
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
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/unknown",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hi".to_string(), session_id: None, usage: None },
            tx,
        );

        assert!(rx.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn empty_enabled_falls_back_to_echo() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::new(),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "fallback".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "fallback"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// When `enabled_agents` is non-empty and does not contain the default
    /// agent, `resolve_agent` must return an error rather than silently
    /// routing to a disabled agent.
    #[tokio::test]
    async fn disabled_default_agent_returns_error() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        // "chat" is the default but it is not in the enabled set.
        let cfg = AgentsConfig {
            default_agent: "chat".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hi".to_string(), session_id: None, usage: None },
            tx,
        );

        assert!(rx.await.unwrap().is_err());
    }

    /// Verifies the full basic_chat -> llm/complete round-trip through the bus.
    /// A fake LLM responder runs concurrently and answers the spawned request.
    #[tokio::test]
    async fn basic_chat_routes_through_llm_subsystem() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        // Spawn a fake LLM responder that echoes with a marker prefix.
        tokio::spawn(async move {
            if let Some(BusMessage::Request { payload, reply_tx, .. }) = rx.recv().await {
                if let BusPayload::LlmRequest { channel_id, content } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("[fake] {content}"),
                        session_id: None,
                        usage: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "basic_chat".to_string(),
            enabled: HashSet::from(["basic_chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "[fake] hello"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Verifies news-agent returns raw `newsmail_aggregator/get` payload as comms content.
    #[cfg(feature = "plugin-news-agent")]
    #[tokio::test]
    async fn news_agent_returns_raw_tool_payload() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        tokio::spawn(async move {
            if let Some(BusMessage::Request { method, payload, reply_tx, .. }) = rx.recv().await {
                assert_eq!(method, "tools/execute");
                if let BusPayload::ToolRequest { tool, action, args_json, channel_id, .. } = payload {
                    assert_eq!(tool, "newsmail_aggregator");
                    assert_eq!(action, "get");
                    assert_eq!(args_json, "{}");
                    assert_eq!(channel_id, "pty0");
                    let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                        tool: "newsmail_aggregator".to_string(),
                        action: "get".to_string(),
                        ok: true,
                        data_json: Some("[{\"subject\":\"A\"}]".to_string()),
                        error: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "news-agent".to_string(),
            enabled: HashSet::from(["news-agent".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "ignored".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert_eq!(content, "[{\"subject\":\"A\"}]")
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_or_create_subagent() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let subagent = agents.state.get_or_create_subagent("echo", "worker1").unwrap();
        
        assert!(subagent.identity_dir.exists());
        
        let dir_name = subagent.identity_dir.file_name().unwrap().to_str().unwrap();
        assert!(dir_name.starts_with("worker1-"));
        assert!(dir_name.ends_with(&subagent.public_id));
        
        let parent_dir = subagent.identity_dir.parent().unwrap();
        assert!(parent_dir.ends_with("subagents"));
    }
}
