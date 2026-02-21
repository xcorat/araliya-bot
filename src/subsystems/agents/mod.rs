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
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::dispatch::BusHandler;

#[cfg(feature = "subsystem-memory")]
use crate::subsystems::memory::MemorySystem;

// Chat-family plugins (basic_chat, session_chat) and shared ChatCore.
#[cfg(any(feature = "plugin-basic-chat", feature = "plugin-chat"))]
mod chat;
#[cfg(feature = "plugin-gmail-agent")]
mod gmail;

// ── AgentsState ───────────────────────────────────────────────────────────────

/// Shared capability surface passed to agent plugins.
///
/// The raw [`BusHandle`] is private — plugins call typed methods and cannot
/// address arbitrary bus targets.
pub struct AgentsState {
    /// Supervisor bus — private to this module.
    bus: BusHandle,
    /// Memory system — always present when the subsystem-memory feature is
    /// enabled.  Agents create or load sessions via this handle.
    #[cfg(feature = "subsystem-memory")]
    pub memory: Arc<MemorySystem>,
    /// Per-agent memory store requirements from config.
    pub agent_memory: HashMap<String, Vec<String>>,
}

impl AgentsState {
    fn new(
        bus: BusHandle,
        #[cfg(feature = "subsystem-memory")] memory: Arc<MemorySystem>,
        agent_memory: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            bus,
            #[cfg(feature = "subsystem-memory")]
            memory,
            agent_memory,
        }
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
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage { channel_id, content, session_id }));
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
        #[cfg(feature = "subsystem-memory")] memory: Arc<MemorySystem>,
    ) -> Self {
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

        Self {
            state: Arc::new(AgentsState::new(
                bus,
                #[cfg(feature = "subsystem-memory")]
                memory,
                agent_memory,
            )),
            agents,
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

    // ── Session query handlers ─────────────────────────────────────────────

    /// Handle `agents/sessions` — return a JSON list of all sessions.
    fn handle_session_list(&self, reply_tx: oneshot::Sender<BusResult>) {
        #[cfg(feature = "subsystem-memory")]
        {
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

        #[cfg(not(feature = "subsystem-memory"))]
        {
            let body = serde_json::json!({ "sessions": [] });
            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: body.to_string(),
            }));
        }
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

        #[cfg(feature = "subsystem-memory")]
        {
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

                let body = serde_json::json!({
                    "session_id": session_id,
                    "transcript": transcript,
                });

                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: body.to_string(),
                }));
            });
        }

        #[cfg(not(feature = "subsystem-memory"))]
        {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!("session not found: {session_id} (memory system disabled)"),
            )));
        }
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

        #[cfg(feature = "subsystem-memory")]
        {
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

        #[cfg(not(feature = "subsystem-memory"))]
        {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!("session not found: {session_id} (memory system disabled)"),
            )));
        }
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

        #[cfg(feature = "subsystem-memory")]
        {
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

        #[cfg(not(feature = "subsystem-memory"))]
        {
            let _ = reply_tx.send(Err(BusError::new(
                -32000,
                format!("session not found: {session_id} (memory system disabled)"),
            )));
        }
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
            BusPayload::CommsMessage { channel_id, content, session_id } => {
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
    #[cfg(feature = "subsystem-memory")]
    fn test_memory() -> (tempfile::TempDir, Arc<MemorySystem>) {
        let dir = tempfile::TempDir::new().unwrap();
        let mem = MemorySystem::new(dir.path(), crate::subsystems::memory::MemoryConfig::default()).unwrap();
        (dir, Arc::new(mem))
    }

    #[tokio::test]
    async fn routes_to_default_agent_when_unmapped() {
        let (_bus, handle) = echo_bus();
        #[cfg(feature = "subsystem-memory")]
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(
            cfg,
            handle,
            #[cfg(feature = "subsystem-memory")]
            memory,
        );

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string(), session_id: None },
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
        #[cfg(feature = "subsystem-memory")]
        let (_dir, memory) = test_memory();
        let mut channel_map = HashMap::new();
        channel_map.insert("pty0".to_string(), "echo".to_string());

        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map,
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(
            cfg,
            handle,
            #[cfg(feature = "subsystem-memory")]
            memory,
        );

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "mapped".to_string(), session_id: None },
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
        #[cfg(feature = "subsystem-memory")]
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(
            cfg,
            handle,
            #[cfg(feature = "subsystem-memory")]
            memory,
        );

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/unknown",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hi".to_string(), session_id: None },
            tx,
        );

        assert!(rx.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn empty_enabled_falls_back_to_echo() {
        let (_bus, handle) = echo_bus();
        #[cfg(feature = "subsystem-memory")]
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::new(),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(
            cfg,
            handle,
            #[cfg(feature = "subsystem-memory")]
            memory,
        );

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "fallback".to_string(), session_id: None },
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
        #[cfg(feature = "subsystem-memory")]
        let (_dir, memory) = test_memory();

        // Spawn a fake LLM responder that echoes with a marker prefix.
        tokio::spawn(async move {
            if let Some(BusMessage::Request { payload, reply_tx, .. }) = rx.recv().await {
                if let BusPayload::LlmRequest { channel_id, content } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("[fake] {content}"),
                        session_id: None,
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
        let agents = AgentsSubsystem::new(
            cfg,
            handle,
            #[cfg(feature = "subsystem-memory")]
            memory,
        );

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hello".to_string(), session_id: None },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "[fake] hello"),
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
