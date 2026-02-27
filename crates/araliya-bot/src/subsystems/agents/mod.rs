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
// _TODO_: check if we should be using more fine-grained locks.
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::config::{AgentsConfig, DocsAgentConfig, DocsKgConfig};
use crate::error::AppError;
use crate::llm::ModelRates;
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::{ComponentInfo, ComponentStatusResponse};
use crate::supervisor::dispatch::BusHandler;
use crate::supervisor::health::HealthReporter;

use crate::identity::{self, Identity};
use crate::subsystems::memory::{AGENTS_DIRNAME, MemorySystem};
use crate::subsystems::memory::handle::SessionHandle;

// CHECK: wat?
pub(crate) mod core;

// Chat-family plugins (basic_chat, session_chat) and shared ChatCore.
#[cfg(any(feature = "plugin-basic-chat", feature = "plugin-chat"))]
mod chat;
#[cfg(feature = "plugin-gmail-agent")]
mod gmail;
#[cfg(feature = "plugin-news-agent")]
mod news;
#[cfg(feature = "plugin-docs")]
mod docs;
#[cfg(feature = "plugin-docs")]
mod docs_import;

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
    /// Default args JSON forwarded by the `news` agent to `newsmail_aggregator/get`.
    pub news_query_args_json: String,
    /// Index document ID (relative path) that the docs agent falls back to
    /// when no RAG search result is returned for a query.
    pub docs_index_name: Option<String>,
    /// Enable the KG-RAG pipeline in the docs agent.
    pub docs_use_kg: bool,
    /// KG tuning parameters forwarded to IKGDocStore.
    pub docs_kg_config: DocsKgConfig,
    /// Names of tool plugins enabled at startup — injected into prompt layer 2.
    pub enabled_tools: Vec<String>,
}

impl AgentsState {
    fn new(
        bus: BusHandle,
        memory: Arc<MemorySystem>,
        agent_memory: HashMap<String, Vec<String>>,
        agent_identities: HashMap<String, Identity>,
        news_query_args_json: String,
        docs_index_name: Option<String>,
        docs_use_kg: bool,
        docs_kg_config: DocsKgConfig,
        enabled_tools: Vec<String>,
    ) -> Self {
        Self {
            bus,
            memory,
            agent_memory,
            agent_identities,
            llm_rates: ModelRates::default(),
            news_query_args_json,
            docs_index_name,
            docs_use_kg,
            docs_kg_config,
            enabled_tools,
        }
    }

    /// Open (or create) the persistent [`AgentStore`] for `agent_id`.
    ///
    /// The store is rooted at `{agent_identity_dir}/store/` and survives
    /// restarts.  This call is synchronous (blocking I/O) — wrap in
    /// `spawn_blocking` when called from an async context.
    ///
    /// [`AgentStore`]: crate::subsystems::memory::stores::agent::AgentStore
    pub fn open_agent_store(
        &self,
        agent_id: &str,
    ) -> Result<crate::subsystems::memory::stores::agent::AgentStore, AppError> {
        let identity = self.agent_identities.get(agent_id).ok_or_else(|| {
            AppError::Identity(format!("agent '{}' not found", agent_id))
        })?;
        crate::subsystems::memory::stores::agent::AgentStore::open(&identity.identity_dir)
    }

    /// Get or create a subagent identity under the parent agent's memory directory.
    ///
    /// Subagents are ephemeral or task-specific workers that operate under their parent's
    /// identity structure under the shared per-agent identities directory.
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
        self.complete_via_llm_with_system(channel_id, content, None).await
    }

    /// Forward content to the LLM subsystem with an explicit system prompt.
    ///
    /// `system` is sent as the `"system"` role message before the user content.
    /// When `None`, behaviour is identical to [`complete_via_llm`].
    pub async fn complete_via_llm_with_system(
        &self,
        channel_id: &str,
        content: &str,
        system: Option<&str>,
    ) -> BusResult {
        let result = self
            .bus
            .request(
                "llm/complete",
                BusPayload::LlmRequest {
                    channel_id: channel_id.to_string(),
                    content: content.to_string(),
                    system: system.map(|s| s.to_string()),
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
    /// Source directory for docs import (populated from config).
    #[cfg(feature = "plugin-docs")]
    docs_source_dir: Option<std::path::PathBuf>,
    reporter: Option<HealthReporter>,
}

impl AgentsSubsystem {
    fn effective_enabled_agent_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = if self.enabled_agents.is_empty() {
            self.agents.keys().cloned().collect()
        } else {
            self.enabled_agents
                .iter()
                .filter(|id| self.agents.contains_key(id.as_str()))
                .cloned()
                .collect()
        };
        ids.sort();
        ids
    }

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
        let news_query_args_json = match config.news_query {
            Some(q) => {
                let mut map = serde_json::Map::new();
                if let Some(label) = q.label {
                    map.insert("label".to_string(), serde_json::Value::String(label));
                }
                if let Some(n_last) = q.n_last {
                    map.insert("n_last".to_string(), serde_json::json!(n_last));
                }
                if let Some(t_interval) = q.t_interval {
                    map.insert("t_interval".to_string(), serde_json::Value::String(t_interval));
                }
                if let Some(tsec_last) = q.tsec_last {
                    map.insert("tsec_last".to_string(), serde_json::json!(tsec_last));
                }
                if let Some(extra_q) = q.q {
                    map.insert("q".to_string(), serde_json::Value::String(extra_q));
                }
                serde_json::Value::Object(map).to_string()
            }
            None => "{}".to_string(),
        };

        #[cfg(feature = "plugin-docs")]
        let docs_source_dir: Option<std::path::PathBuf> = config
            .docs
            .as_ref()
            .and_then(|d| d.docsdir.as_deref())
            .map(std::path::PathBuf::from);
        let docs_index_name: Option<String> = config
            .docs
            .as_ref()
            .filter(|d| d.docsdir.is_some())
            .map(|d| d.index.clone().unwrap_or_else(|| "index.md".to_string()));
        let docs_use_kg = config.docs.as_ref().map(|d| d.use_kg).unwrap_or(false);
        let docs_kg_config = config
            .docs
            .as_ref()
            .map(|d| d.kg.clone())
            .unwrap_or_default();

        // Register all known built-in agents.
        // Uses agent.id() as the HashMap key so the trait method is the
        // single source of truth for each agent's identity.
        let mut agents: HashMap<String, Box<dyn Agent>> = HashMap::new();

        #[cfg(feature = "plugin-docs")]
        {
            let agent: Box<dyn Agent> = Box::new(docs::DocsAgentPlugin);
            agents.insert(agent.id().to_string(), agent);
        }

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

        // Collect which tool plugins are compiled in.
        let mut enabled_tools: Vec<String> = Vec::new();
        #[cfg(feature = "plugin-gmail-tool")]
        {
            enabled_tools.push("gmail".to_string());
            enabled_tools.push("newsmail_aggregator".to_string());
        }

        // Initialize cryptographic identities for all registered agents.
        let mut agent_identities = HashMap::new();
        let agent_memory_root = memory.memory_root().join(AGENTS_DIRNAME);
        for agent_id in agents.keys() {
            let identity = identity::setup_named_identity(&agent_memory_root, agent_id)?;
            agent_identities.insert(agent_id.clone(), identity);
        }

        Ok(Self {
            state: Arc::new(AgentsState::new(
                bus,
                memory,
                agent_memory,
                agent_identities,
                news_query_args_json,
                docs_index_name,
                docs_use_kg,
                docs_kg_config,
                enabled_tools,
            )),
            agents,
            default_agent,
            channel_map: config.channel_map,
            enabled_agents,
            #[cfg(feature = "plugin-docs")]
            docs_source_dir,
            reporter: None,
        })
    }

    /// Initialise the docs agent docstore by importing content from the configured
    /// source directory.  Should be called once after construction, before the
    /// subsystem receives any requests.  Safe to call from an async context.
    ///
    /// If no `docsdir` is configured this is a no-op.
    #[cfg(feature = "plugin-docs")]
    pub async fn init_docs(&self) -> Result<(), AppError> {
        let source_dir = match &self.docs_source_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        let identity = match self.state.agent_identities.get("docs") {
            Some(id) => id.clone(),
            None => {
                tracing::warn!("init_docs: 'docs' agent identity not found; skipping import");
                return Ok(());
            }
        };

        let index_name = self
            .state
            .docs_index_name
            .clone()
            .unwrap_or_else(|| "index.md".to_string());
        let identity_dir = identity.identity_dir.clone();

        tokio::task::spawn_blocking(move || {
            docs_import::populate_docstore_from_source(&identity_dir, &source_dir, &index_name)
        })
        .await
        .map_err(|e| AppError::Memory(format!("init_docs: spawn_blocking panicked: {e}")))?
    }

    /// Set the LLM pricing rates on the shared state.
    /// Call this after `new()` when rates are available from config.
    pub fn with_llm_rates(mut self, rates: ModelRates) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("AgentsState Arc must be exclusive at build time")
            .llm_rates = rates;
        self
    }

    /// Attach a health reporter and report initial healthy state.
    pub fn with_health_reporter(mut self, reporter: HealthReporter) -> Self {
        let enabled = self.effective_enabled_agent_ids();
        let agent_count = enabled.len();
        let r = reporter.clone();
        tokio::spawn(async move {
            r.set_healthy_with(
                "ok",
                Some(serde_json::json!({
                    "agent_count": agent_count,
                    "agents": enabled,
                })),
            ).await;
        });
        self.reporter = Some(reporter);
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

    fn load_scoped_session(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionHandle, AppError> {
        if let Some(agent) = agent_id {
            let store = self.state.open_agent_store(agent)?;
            let sessions_root = store.agent_sessions_dir();
            let index_path = store.agent_sessions_index();
            return self
                .state
                .memory
                .load_session_in(&sessions_root, &index_path, session_id, Some(agent));
        }

        self.state.memory.load_session(session_id, None)
    }

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

        let sessions_root = memory.sessions_root().to_path_buf();
        let body = serde_json::json!({
            "sessions": sessions.iter().map(|s| {
                let updated_at = read_session_updated_at(&sessions_root, &s.session_id)
                    .unwrap_or_else(|| s.created_at.clone());
                serde_json::json!({
                    "session_id": s.session_id,
                    "created_at": s.created_at,
                    "updated_at": updated_at,
                    "store_types": s.store_types,
                    "last_agent": s.last_agent,
                })
            }).collect::<Vec<_>>()
        });

        let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
            data: body.to_string(),
        }));
    }

    /// Handle `agents/kg_graph` — return the knowledge graph JSON for an agent.
    fn handle_agent_kg_graph(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let agent_id = match payload {
            BusPayload::SessionQuery { agent_id: Some(id), .. } => id,
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected agent_id in payload")));
                return;
            }
        };

        let identity = match self.state.agent_identities.get(&agent_id) {
            Some(id) => id.clone(),
            None => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("agent not found: {agent_id}"))));
                return;
            }
        };

        let kg_path = identity.identity_dir.join("kgdocstore").join("kg").join("graph.json");

        let body = match std::fs::read_to_string(&kg_path) {
            Ok(json) => {
                let graph = serde_json::from_str::<serde_json::Value>(&json)
                    .unwrap_or_else(|_| serde_json::json!({"entities": {}, "relations": []}));
                serde_json::json!({ "agent_id": agent_id, "graph": graph })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                serde_json::json!({ "agent_id": agent_id, "graph": { "entities": {}, "relations": [] } })
            }
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("failed to read KG graph: {e}"))));
                return;
            }
        };

        let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: body.to_string() }));
    }

    /// Handle `agents/list` — return metadata for all registered agents.
    fn handle_agents_list(&self, reply_tx: oneshot::Sender<BusResult>) {
        let identities = &self.state.agent_identities;
        let agents: Vec<serde_json::Value> = identities
            .iter()
            .map(|(agent_id, identity)| {
                let kv_path = identity.identity_dir.join("store").join("kv.json");
                let last_fetched = read_agent_kv_value(&kv_path, "last_fetched");
                let index_path = identity.identity_dir.join("sessions.json");
                let session_count = count_agent_sessions(&index_path);
                let store_types = detect_agent_store_types(
                    &identity.identity_dir,
                    self.state.agent_memory.get(agent_id),
                    &index_path,
                );
                serde_json::json!({
                    "agent_id": agent_id,
                    "name": agent_id,
                    "last_fetched": last_fetched,
                    "session_count": session_count,
                    "store_types": store_types,
                })
            })
            .collect();

        let body = serde_json::json!({ "agents": agents });
        let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
            data: body.to_string(),
        }));
    }

    /// Handle `agents/sessions/detail` — return session metadata + transcript.
    fn handle_session_detail(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let (session_id, agent_id) = match payload {
            BusPayload::SessionQuery { session_id, agent_id } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let handle = match self.load_scoped_session(&session_id, agent_id.as_deref()) {
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
                "agent_id": agent_id,
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
        let (session_id, agent_id) = match payload {
            BusPayload::SessionQuery { session_id, agent_id } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let handle = match self.load_scoped_session(&session_id, agent_id.as_deref()) {
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
                "agent_id": agent_id,
                "content": content,
            });

            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: body.to_string(),
            }));
        });
    }

    /// Handle `agents/sessions/files` — return files in the session directory.
    fn handle_session_files(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let (session_id, agent_id) = match payload {
            BusPayload::SessionQuery { session_id, agent_id } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected SessionQuery payload",
                )));
                return;
            }
        };

        let handle = match self.load_scoped_session(&session_id, agent_id.as_deref()) {
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
                "agent_id": agent_id,
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
        // ── Subsystem-level health (must be intercepted before parse_method
        //    which would interpret "agents/health" as agent_id="health") ──────
        if method == "agents/health" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let h = match reporter {
                    Some(r) => r.get_current().await
                        .unwrap_or_else(|| crate::supervisor::health::SubsystemHealth::ok("agents")),
                    None => crate::supervisor::health::SubsystemHealth::ok("agents"),
                };
                let data = serde_json::to_string(&h).unwrap_or_default();
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
            });
            return;
        }

        // ── Status routes ──────────────────────────────────────
        if method == "agents/status" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let resp = status_from_reporter("agents", reporter).await;
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: resp.to_json() }));
            });
            return;
        }

        if method == "agents/detailed_status" {
            let reporter = self.reporter.clone();
            let memory = self.state.memory.clone();
            let default_agent = self.default_agent.clone();
            let enabled_agents = self.effective_enabled_agent_ids();
            tokio::spawn(async move {
                let base = status_from_reporter("agents", reporter).await;
                let session_count = memory.list_sessions().map(|s| s.len()).unwrap_or(0);
                let data = serde_json::json!({
                    "id": base.id,
                    "status": base.status,
                    "state": base.state,
                    "default_agent": default_agent,
                    "enabled_agents": enabled_agents,
                    "session_count": session_count,
                });
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: data.to_string() }));
            });
            return;
        }

        // ── Agent metadata queries ─────────────────────────────
        if method == "agents/list" {
            self.handle_agents_list(reply_tx);
            return;
        }
        if method == "agents/kg_graph" {
            self.handle_agent_kg_graph(payload, reply_tx);
            return;
        }

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

        // Per-agent status routes (intercepted before payload dispatch).
        if let Some(ref agent_id) = method_agent_id {
            if action == "status" {
                let exists = self.agents.contains_key(agent_id.as_str());
                let id = agent_id.clone();
                tokio::spawn(async move {
                    let resp = if exists {
                        ComponentStatusResponse::running(id)
                    } else {
                        ComponentStatusResponse::error(id, "not found")
                    };
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: resp.to_json() }));
                });
                return;
            }

            if action == "detailed_status" {
                let exists = self.agents.contains_key(agent_id.as_str());
                let id = agent_id.clone();
                let identities = self.state.agent_identities.clone();
                tokio::spawn(async move {
                    if !exists {
                        let resp = ComponentStatusResponse::error(&id, "not found");
                        let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: resp.to_json() }));
                        return;
                    }
                    let kv_path = identities.get(&id)
                        .map(|ident| ident.identity_dir.join("store").join("kv.json"));
                    let last_fetched = kv_path.as_ref()
                        .and_then(|p| read_agent_kv_value(p, "last_fetched"));
                    let index_path = identities.get(&id)
                        .map(|ident| ident.identity_dir.join("sessions.json"));
                    let session_count = index_path.as_ref()
                        .map(|p| count_agent_sessions(p))
                        .unwrap_or(0);
                    let data = serde_json::json!({
                        "id": id,
                        "status": "running",
                        "state": "on",
                        "session_count": session_count,
                        "last_fetched": last_fetched,
                    });
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: data.to_string() }));
                });
                return;
            }
        }

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

    fn component_info(&self) -> ComponentInfo {
        let mut children: Vec<ComponentInfo> = self
            .effective_enabled_agent_ids()
            .into_iter()
            .map(|id| {
                let name = ComponentInfo::capitalise(&id);
                let mut node = ComponentInfo::leaf(&id, &name);
                if id == self.default_agent {
                    node.name = format!("{name} (default)");
                }
                node
            })
            .collect();
        children.sort_by(|a, b| a.id.cmp(&b.id));
        ComponentInfo::running("agents", "Agents", children)
    }
}

// ── Module-level helpers ──────────────────────────────────────────────────────

/// Read `last_updated` from `{session_dir}/spend.json`, fall back to None.
fn read_session_updated_at(sessions_root: &std::path::Path, session_id: &str) -> Option<String> {
    let data = std::fs::read_to_string(
        sessions_root.join(session_id).join("spend.json")
    ).ok()?;
    #[derive(serde::Deserialize)]
    struct SpendTs { last_updated: String }
    serde_json::from_str::<SpendTs>(&data).ok().map(|s| s.last_updated)
}

/// Read a single string value from an AgentStore `kv.json` file.
fn read_agent_kv_value(kv_path: &std::path::Path, key: &str) -> Option<String> {
    let data = std::fs::read_to_string(kv_path).ok()?;
    #[derive(serde::Deserialize)]
    struct KvPartial { values: std::collections::HashMap<String, String> }
    serde_json::from_str::<KvPartial>(&data).ok()?.values.remove(key)
}

/// Return the number of sessions in an agent's `sessions.json` index.
fn count_agent_sessions(index_path: &std::path::Path) -> usize {
    if !index_path.exists() { return 0; }
    let data = match std::fs::read_to_string(index_path) { Ok(d) => d, Err(_) => return 0 };
    #[derive(serde::Deserialize)]
    struct Idx { sessions: std::collections::HashMap<String, serde_json::Value> }
    serde_json::from_str::<Idx>(&data).map(|i| i.sessions.len()).unwrap_or(0)
}

/// Infer available store types for an agent from config and on-disk layout.
fn detect_agent_store_types(
    identity_dir: &std::path::Path,
    configured: Option<&Vec<String>>,
    sessions_index: &std::path::Path,
) -> Vec<String> {
    let mut stores = std::collections::HashSet::<String>::new();

    if let Some(configured) = configured {
        for store in configured {
            stores.insert(store.clone());
        }
    }

    if sessions_index.exists() {
        stores.insert("basic_session".to_string());
    }
    if identity_dir.join("docstore").exists() {
        stores.insert("docstore".to_string());
    }
    if identity_dir.join("kgdocstore").exists() {
        stores.insert("kgdocstore".to_string());
    }

    let mut out: Vec<String> = stores.into_iter().collect();
    out.sort();
    out
}

/// Derive a [`ComponentStatusResponse`] from an optional [`HealthReporter`].
///
/// Returns `running` when the reporter has not yet written any state (or is
/// absent) — the component is assumed healthy until told otherwise.
async fn status_from_reporter(
    id: &str,
    reporter: Option<HealthReporter>,
) -> ComponentStatusResponse {
    match reporter {
        Some(r) => match r.get_current().await {
            Some(h) if h.healthy => ComponentStatusResponse::running(id),
            Some(h) => ComponentStatusResponse::error(id, h.message),
            None => ComponentStatusResponse::running(id),
        },
        None => ComponentStatusResponse::running(id),
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
            news_query: None,
            docs: None,
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
            news_query: None,
            docs: None,
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
            news_query: None,
            docs: None,
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
            news_query: None,
            docs: None,
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
            news_query: None,
            docs: None,
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
                if let BusPayload::LlmRequest { channel_id, content, .. } = payload {
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
            news_query: None,
            docs: None,
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

    /// Verifies news fetches, calls LLM, and returns the LLM summary.
    #[cfg(feature = "plugin-news-agent")]
    #[tokio::test]
    async fn news_agent_summarises_via_llm() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        // Handle two sequential bus requests: tool then LLM.
        tokio::spawn(async move {
            // First: tools/execute
            if let Some(BusMessage::Request { method, payload, reply_tx, .. }) = rx.recv().await {
                assert_eq!(method, "tools/execute");
                if let BusPayload::ToolRequest { tool, action, .. } = payload {
                    assert_eq!(tool, "newsmail_aggregator");
                    assert_eq!(action, "get");
                }
                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                    tool: "newsmail_aggregator".to_string(),
                    action: "get".to_string(),
                    ok: true,
                    data_json: Some("[{\"subject\":\"Test Headline\",\"from\":\"news@example.com\",\"date\":\"2026-02-21\"}]".to_string()),
                    error: None,
                }));
            }
            // Second: llm/complete
            if let Some(BusMessage::Request { method, payload, reply_tx, .. }) = rx.recv().await {
                assert_eq!(method, "llm/complete");
                if let BusPayload::LlmRequest { content, channel_id, .. } = payload {
                    assert!(content.contains("Test Headline"), "prompt should contain subject");
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "Summary: Test Headline from news@example.com.".to_string(),
                        session_id: None,
                        usage: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "news".to_string(),
            enabled: HashSet::from(["news".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert!(content.contains("Summary:"), "response should be LLM summary, got: {content}");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Empty inbox skips LLM entirely and returns a fixed message.
    #[cfg(feature = "plugin-news-agent")]
    #[tokio::test]
    async fn news_agent_empty_inbox_skips_llm() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        tokio::spawn(async move {
            // Only one bus message: the tool request.
            if let Some(BusMessage::Request { reply_tx, .. }) = rx.recv().await {
                let _ = reply_tx.send(Ok(BusPayload::ToolResponse {
                    tool: "newsmail_aggregator".to_string(),
                    action: "get".to_string(),
                    ok: true,
                    data_json: Some("[]".to_string()),
                    error: None,
                }));
            }
            // No further messages — asserting LLM is not called.
        });

        let cfg = AgentsConfig {
            default_agent: "news".to_string(),
            enabled: HashSet::from(["news".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            docs: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert_eq!(content, "No new news emails.");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Basic health check for docs agent.
    #[cfg(feature = "plugin-docs")]
    #[tokio::test]
    async fn docs_agent_health() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "docs".to_string(),
            enabled: HashSet::from(["docs".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            docs: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/health",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "".to_string(), session_id: None, usage: None },
            tx,
        );
        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert!(content.contains("docs component"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// Asking the docs agent reads the specified file and forwards prompt to LLM.
    #[cfg(feature = "plugin-docs")]
    #[tokio::test]
    async fn docs_agent_reads_file_and_queries_llm() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        // Prepare a temp docs directory with an index.md containing known content.
        let docs_tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(docs_tmp.path().join("index.md"), "the quick brown fox").unwrap();
        let docsdir = docs_tmp.path().to_str().unwrap().to_string();

        // Fake LLM responder that checks the prompt includes the docs content.
        tokio::spawn(async move {
            if let Some(BusMessage::Request { method, payload, reply_tx, .. }) = rx.recv().await {
                assert_eq!(method, "llm/complete");
                if let BusPayload::LlmRequest { content, channel_id, .. } = payload {
                    assert!(content.contains("the quick brown fox"));
                    assert!(content.contains("what color"));
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "brown".to_string(),
                        session_id: None,
                        usage: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "docs".to_string(),
            enabled: HashSet::from(["docs".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            docs: Some(DocsAgentConfig {
                docsdir: Some(docsdir),
                index: Some("index.md".to_string()),
                use_kg: false,
                kg: DocsKgConfig::default(),
            }),
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        // Populate the docs docstore before handling any queries.
        agents.init_docs().await.unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/ask",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "what color".to_string(), session_id: None, usage: None },
            tx,
        );

        match rx.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "brown"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// Asking the docs agent when the docstore is empty returns an error.
    #[cfg(feature = "plugin-docs")]
    #[tokio::test]
    async fn docs_agent_missing_file_errors() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "docs".to_string(),
            enabled: HashSet::from(["docs".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            // No docsdir configured — docstore will remain empty.
            docs: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        // Do NOT call init_docs — docstore stays empty.

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/ask",
            BusPayload::CommsMessage { channel_id: "pty0".to_string(), content: "hi".to_string(), session_id: None, usage: None },
            tx,
        );
        assert!(rx.await.unwrap().is_err());
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
            news_query: None,
            docs: None,
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

    #[tokio::test]
    async fn component_info_shows_only_enabled_agents() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            docs: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let info = agents.component_info();
        let child_ids: Vec<String> = info.children.into_iter().map(|c| c.id).collect();
        assert_eq!(child_ids, vec!["echo".to_string()]);
    }

    #[tokio::test]
    async fn detailed_status_reports_only_enabled_agents() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            docs: None,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request("agents/detailed_status", BusPayload::Empty, tx);

        let payload = rx.await.unwrap().unwrap();
        let BusPayload::JsonResponse { data } = payload else {
            panic!("unexpected payload");
        };
        let value: serde_json::Value = serde_json::from_str(&data).unwrap();
        let enabled = value
            .get("enabled_agents")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(enabled, vec![serde_json::Value::String("echo".to_string())]);
    }
}
