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

use crate::subsystems::agents::core::AgentRuntimeClass;

use tokio::sync::oneshot;

use crate::config::{AgenticChatConfig, AgentsConfig, DocsAgentConfig, RuntimeCmdAgentConfig};
use crate::error::AppError;
use crate::llm::ModelRates;
use crate::supervisor::bus::{BusError, BusHandle, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::{ComponentInfo, ComponentStatusResponse};
use crate::supervisor::dispatch::BusHandler;
use crate::supervisor::health::HealthReporter;

use crate::identity::{self, Identity};
use crate::subsystems::memory::handle::SessionHandle;
use crate::subsystems::memory::{AGENTS_DIRNAME, MemorySystem};

// CHECK: wat?
pub(crate) mod core;

// Chat-family plugins (basic_chat, session_chat) and shared ChatCore.
#[cfg(feature = "plugin-agentic-chat")]
mod agentic_chat;
#[cfg(any(feature = "plugin-basic-chat", feature = "plugin-chat"))]
mod chat;
#[cfg(feature = "plugin-docs")]
mod docs;
#[cfg(feature = "plugin-docs-agent")]
mod docs_agent;
#[cfg(feature = "plugin-docs")]
mod docs_import;
#[cfg(feature = "plugin-gmail-agent")]
mod gmail;
#[cfg(feature = "plugin-news-agent")]
mod news;
#[cfg(feature = "plugin-runtime-cmd")]
mod runtime_cmd;
#[cfg(feature = "plugin-uniweb")]
mod uniweb;

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
    /// Per-agent docstore configuration: agent_id → docs config.
    /// Agents with `docsdir` set in config get an entry here.
    pub agent_docs: HashMap<String, DocsAgentConfig>,
    /// Per-agent bus-tool allowlists: agent_id → tool names.
    /// Each agent only sees tools declared in its `skills` config.
    pub agent_skills: HashMap<String, Vec<String>>,
    /// Enable per-turn debug logging to session KV store.
    pub debug_logging: bool,
}

impl AgentsState {
    fn new(
        bus: BusHandle,
        memory: Arc<MemorySystem>,
        agent_memory: HashMap<String, Vec<String>>,
        agent_identities: HashMap<String, Identity>,
        news_query_args_json: String,
        agent_docs: HashMap<String, DocsAgentConfig>,
        agent_skills: HashMap<String, Vec<String>>,
        debug_logging: bool,
    ) -> Self {
        Self {
            bus,
            memory,
            agent_memory,
            agent_identities,
            llm_rates: ModelRates::default(),
            news_query_args_json,
            agent_docs,
            agent_skills,
            debug_logging,
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
        let identity = self
            .agent_identities
            .get(agent_id)
            .ok_or_else(|| AppError::Identity(format!("agent '{}' not found", agent_id)))?;
        crate::subsystems::memory::stores::agent::AgentStore::open(&identity.identity_dir)
    }

    /// Get or create a subagent identity under the parent agent's memory directory.
    ///
    /// Subagents are ephemeral or task-specific workers that operate under their parent's
    /// identity structure under the shared per-agent identities directory.
    pub fn get_or_create_subagent(
        &self,
        agent_id: &str,
        subagent_name: &str,
    ) -> Result<Identity, AppError> {
        let agent_identity = self
            .agent_identities
            .get(agent_id)
            .ok_or_else(|| AppError::Identity(format!("agent '{}' not found", agent_id)))?;
        let subagents_dir = agent_identity.identity_dir.join("subagents");
        identity::setup_named_identity(&subagents_dir, subagent_name)
    }

    /// Forward content to the LLM subsystem and return the completion.
    pub async fn complete_via_llm(&self, channel_id: &str, content: &str) -> BusResult {
        self.complete_via_llm_with_system(channel_id, content, None)
            .await
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

    /// Forward content to the instruction LLM via `llm/instruct`.
    ///
    /// If no separate instruction LLM is configured, the LLM subsystem
    /// transparently falls back to the main provider.
    pub async fn complete_via_instruct_llm(
        &self,
        channel_id: &str,
        content: &str,
        system: Option<&str>,
    ) -> BusResult {
        let result = self
            .bus
            .request(
                "llm/instruct",
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

    /// Stream a completion from the main LLM with an explicit system prompt.
    ///
    /// Returns a channel receiver that yields [`StreamChunk`] values.  The
    /// caller should forward these as SSE events.
    pub async fn stream_via_llm_with_system(
        &self,
        channel_id: &str,
        content: &str,
        system: Option<&str>,
    ) -> Result<tokio::sync::mpsc::Receiver<crate::llm::StreamChunk>, BusError> {
        use crate::supervisor::bus::StreamReceiver;
        let result = self
            .bus
            .request(
                "llm/stream",
                BusPayload::LlmRequest {
                    channel_id: channel_id.to_string(),
                    content: content.to_string(),
                    system: system.map(|s| s.to_string()),
                },
            )
            .await;
        match result {
            Ok(Ok(BusPayload::LlmStreamResult {
                rx: StreamReceiver(rx),
            })) => Ok(rx),
            Ok(Ok(_)) => Err(BusError::new(-32000, "unexpected reply to llm/stream")),
            Ok(Err(e)) => Err(e),
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

    /// Send a `runtimes/init` request through the bus.
    pub async fn runtime_init(&self, json: String) -> BusResult {
        let result = self
            .bus
            .request("runtimes/init", BusPayload::JsonResponse { data: json })
            .await;
        match result {
            Ok(r) => r,
            Err(e) => Err(BusError::new(-32000, e.to_string())),
        }
    }

    /// Send a `runtimes/exec` request through the bus.
    pub async fn runtime_exec(&self, json: String) -> BusResult {
        let result = self
            .bus
            .request("runtimes/exec", BusPayload::JsonResponse { data: json })
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
///
/// The trait is intentionally kept stable across the v0.6 PR1 transition.
/// Runtime-class metadata is carried by the [`AgentRegistration`] wrapper
/// rather than being added to this trait, to minimise disruption to existing
/// agent plugin implementations.
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

    /// Handle a streaming request.
    ///
    /// The agent should perform its full pipeline (instruction pass, tools)
    /// and stream the final response via `llm/stream`, replying with
    /// `BusPayload::LlmStreamResult`.
    ///
    /// Default: falls back to [`handle`](Agent::handle) (non-streaming).
    fn handle_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        self.handle(
            "handle".into(),
            channel_id,
            content,
            session_id,
            reply_tx,
            state,
        );
    }
}

// ── AgentRegistration ─────────────────────────────────────────────────────────

/// A registered agent entry in the agents subsystem.
///
/// This is the v0.6 PR1 runtime foundation: every agent stored in
/// [`AgentsSubsystem`] is now wrapped in an `AgentRegistration` that carries
/// its [`AgentRuntimeClass`] alongside the implementation.
///
/// ## Design choice (Option B from the PR spec)
///
/// The [`Agent`] trait is left unchanged.  Runtime-class metadata is stored
/// *beside* the agent implementation in this wrapper rather than *inside* the
/// trait.  This avoids a breaking change to all existing `impl Agent` blocks
/// while still making runtime class a first-class concept in the subsystem.
///
/// Later PRs may introduce `StaticAgent` registrations using the same wrapper,
/// keeping built-in and config-defined agents on a unified registration path.
pub struct AgentRegistration {
    /// Execution model for this agent — the v0.6 runtime class.
    pub runtime_class: AgentRuntimeClass,
    /// The agent implementation.
    pub agent: Box<dyn Agent>,
}

impl AgentRegistration {
    /// Wrap an agent implementation with its runtime class classification.
    pub fn new(runtime_class: AgentRuntimeClass, agent: Box<dyn Agent>) -> Self {
        Self {
            runtime_class,
            agent,
        }
    }
}

// ── Built-in agents ───────────────────────────────────────────────────────────

#[cfg(feature = "plugin-echo")]
struct EchoAgent;

#[cfg(feature = "plugin-echo")]
impl Agent for EchoAgent {
    fn id(&self) -> &str {
        "echo"
    }
    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        _state: Arc<AgentsState>,
    ) {
        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
            channel_id,
            content,
            session_id,
            usage: None,
            timing: None,
            thinking: None,
            cost_usd: None,
        }));
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
    /// All registered agents, keyed by agent ID, each carrying runtime metadata.
    ///
    /// Using [`AgentRegistration`] as the map value (rather than bare
    /// `Box<dyn Agent>`) is the core structural change introduced by v0.6 PR1.
    agents: HashMap<String, AgentRegistration>,
    default_agent: String,
    channel_map: HashMap<String, String>,
    enabled_agents: HashSet<String>,
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
                    map.insert(
                        "t_interval".to_string(),
                        serde_json::Value::String(t_interval),
                    );
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

        let agent_docs = config.agent_docs;

        // Register all known built-in agents.
        //
        // Each agent is wrapped in an [`AgentRegistration`] that pairs the
        // implementation with its v0.6 runtime class.  The agent's own `.id()`
        // method remains the single source of truth for the HashMap key.
        //
        // Runtime class mapping (v0.6 PR1):
        //   echo          → RequestResponse  (stateless echo)
        //   basic_chat    → RequestResponse  (single-turn LLM pass-through)
        //   chat          → Session          (multi-turn with transcript)
        //   agentic-chat  → Agentic          (instruction → tools → response loop)
        //   docs          → Agentic          (RAG retrieval + multi-pass LLM)
        //   news          → Specialized      (external fetch + LLM summary)
        //   gmail         → Specialized      (tool delegation, no conversation)
        //   runtime_cmd   → Specialized      (pure command passthrough)
        let mut agents: HashMap<String, AgentRegistration> = HashMap::new();

        #[cfg(feature = "plugin-docs")]
        {
            let agent: Box<dyn Agent> = Box::new(docs::DocsAgentPlugin);
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Agentic, agent),
            );
        }

        #[cfg(feature = "plugin-echo")]
        {
            let agent: Box<dyn Agent> = Box::new(EchoAgent);
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::RequestResponse, agent),
            );
        }

        #[cfg(feature = "plugin-basic-chat")]
        {
            let agent: Box<dyn Agent> = Box::new(chat::BasicChatPlugin);
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::RequestResponse, agent),
            );
        }

        #[cfg(feature = "plugin-chat")]
        {
            let agent: Box<dyn Agent> = Box::new(chat::SessionChatPlugin::new());
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Session, agent),
            );
        }

        #[cfg(feature = "plugin-gmail-agent")]
        {
            let agent: Box<dyn Agent> = Box::new(gmail::GmailAgentPlugin);
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Specialized, agent),
            );
        }

        #[cfg(feature = "plugin-news-agent")]
        {
            let agent: Box<dyn Agent> = Box::new(news::NewsAgentPlugin);
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Specialized, agent),
            );
        }

        #[cfg(feature = "plugin-agentic-chat")]
        if enabled_agents.contains("agentic-chat") {
            if let Some(ref ac_cfg) = config.agentic_chat {
                let agent: Box<dyn Agent> = Box::new(agentic_chat::AgenticChatPlugin::new(ac_cfg));
                agents.insert(
                    agent.id().to_string(),
                    AgentRegistration::new(AgentRuntimeClass::Agentic, agent),
                );
            } else {
                // Use default config when [agents.agentic-chat] has no extra fields.
                let default_cfg = AgenticChatConfig {
                    use_instruction_llm: false,
                };
                let agent: Box<dyn Agent> =
                    Box::new(agentic_chat::AgenticChatPlugin::new(&default_cfg));
                agents.insert(
                    agent.id().to_string(),
                    AgentRegistration::new(AgentRuntimeClass::Agentic, agent),
                );
            }
        }

        #[cfg(feature = "plugin-runtime-cmd")]
        if enabled_agents.contains("runtime_cmd") {
            let rc_cfg = config.runtime_cmd.unwrap_or_default();
            let agent: Box<dyn Agent> = Box::new(runtime_cmd::RuntimeCmdPlugin::new(&rc_cfg));
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Specialized, agent),
            );
        }

        #[cfg(feature = "plugin-uniweb")]
        {
            // Uniweb — shared-session "front-porch" agentic chat agent.
            // All visitors share a single global session; requests are serialised.
            let configured_sid = config.uniweb_session_id.as_deref().unwrap_or("");
            let agent: Box<dyn Agent> = Box::new(uniweb::UniwebAgent::new(
                configured_sid,
                config.uniweb_use_instruction_llm,
            ));
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Agentic, agent),
            );
        }

        #[cfg(feature = "plugin-docs-agent")]
        {
            let agent: Box<dyn Agent> = Box::new(docs_agent::DocsAgentWrapper::new());
            agents.insert(
                agent.id().to_string(),
                AgentRegistration::new(AgentRuntimeClass::Agentic, agent),
            );
        }

        // Per-agent skills from config — only tools declared here are visible
        // to each agent's instruction manifest.
        let agent_skills = config.agent_skills;

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
                agent_docs,
                agent_skills,
                config.debug_logging,
            )),
            agents,
            default_agent,
            channel_map: config.channel_map,
            enabled_agents,
            reporter: None,
        })
    }

    /// Initialise docstores for all agents that have `docsdir` configured.
    /// Should be called once after construction, before the subsystem receives
    /// any requests.  Safe to call from an async context.
    ///
    /// If no agent has a `docsdir` configured this is a no-op.
    #[cfg(feature = "plugin-docs")]
    pub async fn init_docs(&self) -> Result<(), AppError> {
        for (agent_id, docs_cfg) in &self.state.agent_docs {
            let source_dir = match docs_cfg.docsdir.as_deref() {
                Some(d) => std::path::PathBuf::from(d),
                None => continue,
            };

            let identity = match self.state.agent_identities.get(agent_id) {
                Some(id) => id.clone(),
                None => {
                    tracing::warn!(
                        "init_docs: agent '{}' identity not found; skipping import",
                        agent_id
                    );
                    continue;
                }
            };

            let index_name = docs_cfg
                .index
                .clone()
                .unwrap_or_else(|| "index.md".to_string());
            let identity_dir = identity.identity_dir.clone();

            #[cfg(feature = "ikgdocstore")]
            let use_kg = docs_cfg.use_kg;
            #[cfg(feature = "ikgdocstore")]
            let kg_cfg = docs_cfg.kg.clone();

            tokio::task::spawn_blocking(move || -> Result<(), AppError> {
                docs_import::populate_docstore_from_source(
                    &identity_dir,
                    &source_dir,
                    &index_name,
                )?;

                #[cfg(feature = "ikgdocstore")]
                if use_kg {
                    docs_import::populate_kgdocstore_from_source(
                        &identity_dir,
                        &source_dir,
                        &index_name,
                        &kg_cfg,
                    )?;
                }

                Ok(())
            })
            .await
            .map_err(|e| {
                AppError::Memory(format!(
                    "init_docs({}): spawn_blocking panicked: {e}",
                    agent_id
                ))
            })??;
        }
        Ok(())
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
            )
            .await;
        });
        self.reporter = Some(reporter);
        self
    }

    fn resolve_agent<'a>(
        &'a self,
        method_agent_id: Option<&'a str>,
        channel_id: &str,
    ) -> Result<&'a str, BusError> {
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
            return self.state.memory.load_session_in(
                &sessions_root,
                &index_path,
                session_id,
                Some(agent),
            );
        }

        self.state.memory.load_session(session_id, None)
    }

    /// Handle `agents/sessions` — return a JSON list of all global sessions.
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

    /// Handle `agents/session` — return the primary session transcript for an agent.
    ///
    /// Reads `active_session_id` from the agent's KV store and returns
    /// `{session_id, transcript}` in `SessionDetailResponse` shape.
    /// Returns `{session_id: null, transcript: []}` when no session exists yet.
    fn handle_agent_session(&self, agent_id: String, reply_tx: oneshot::Sender<BusResult>) {
        let store = match self.state.open_agent_store(&agent_id) {
            Ok(s) => s,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

        let session_id = match store.kv_get("active_session_id") {
            Ok(Some(id)) => id,
            Ok(None) => {
                // Agent has never been used — return an empty transcript.
                let body = serde_json::json!({ "session_id": null, "transcript": [] });
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: body.to_string(),
                }));
                return;
            }
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

        let handle = match self.state.memory.load_session_in(
            &store.agent_sessions_dir(),
            &store.agent_sessions_index(),
            &session_id,
            Some(&agent_id),
        ) {
            Ok(h) => h,
            Err(e) => {
                let _ = reply_tx.send(Err(BusError::new(-32000, format!("{e}"))));
                return;
            }
        };

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

    /// Handle `agents/kg_graph` — return the knowledge graph JSON for an agent.
    fn handle_agent_kg_graph(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let agent_id = match payload {
            BusPayload::SessionQuery {
                agent_id: Some(id), ..
            } => id,
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected agent_id in payload")));
                return;
            }
        };

        let identity = match self.state.agent_identities.get(&agent_id) {
            Some(id) => id.clone(),
            None => {
                let _ = reply_tx.send(Err(BusError::new(
                    -32000,
                    format!("agent not found: {agent_id}"),
                )));
                return;
            }
        };

        let kg_path = identity
            .identity_dir
            .join("kgdocstore")
            .join("kg")
            .join("graph.json");

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
                let _ = reply_tx.send(Err(BusError::new(
                    -32000,
                    format!("failed to read KG graph: {e}"),
                )));
                return;
            }
        };

        let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
            data: body.to_string(),
        }));
    }

    /// Handle `agents/list` — return metadata for all registered agents.
    ///
    /// Each entry now includes a `runtime_class` field (v0.6 PR1) so that
    /// admin surfaces and future tooling can inspect the execution model of
    /// each registered agent without requiring a separate lookup.
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
                // Include runtime class from the registration record (v0.6 PR1).
                let runtime_class = self
                    .agents
                    .get(agent_id)
                    .map(|r| r.runtime_class.label())
                    .unwrap_or("unknown");
                serde_json::json!({
                    "agent_id": agent_id,
                    "name": agent_id,
                    "runtime_class": runtime_class,
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
            BusPayload::SessionQuery {
                session_id,
                agent_id,
            } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected SessionQuery payload")));
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
            BusPayload::SessionQuery {
                session_id,
                agent_id,
            } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected SessionQuery payload")));
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
            BusPayload::SessionQuery {
                session_id,
                agent_id,
            } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected SessionQuery payload")));
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

    /// Handle `agents/sessions/debug` — return per-turn debug data from session KV store.
    fn handle_session_debug(&self, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>) {
        let (session_id, agent_id) = match payload {
            BusPayload::SessionQuery {
                session_id,
                agent_id,
            } => (session_id, agent_id),
            _ => {
                let _ = reply_tx.send(Err(BusError::new(-32600, "expected SessionQuery payload")));
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
            // Read the turn counter.
            let turn_count: usize = handle
                .kv_get("debug:turn_count")
                .await
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let mut turns = Vec::with_capacity(turn_count);
            for n in 1..=turn_count {
                let read = |key: &str| {
                    let handle = handle.clone();
                    let key = key.to_string();
                    async move { handle.kv_get(&key).await.ok().flatten().unwrap_or_default() }
                };
                let user_input = read(&format!("debug:turn:{n}:user_input")).await;
                let instruct_prompt = read(&format!("debug:turn:{n}:instruct_prompt")).await;
                let instruction_response =
                    read(&format!("debug:turn:{n}:instruction_response")).await;
                let tool_calls_json = read(&format!("debug:turn:{n}:tool_calls_json")).await;
                let tool_outputs_json = read(&format!("debug:turn:{n}:tool_outputs_json")).await;
                let context = read(&format!("debug:turn:{n}:context")).await;
                let response_prompt = read(&format!("debug:turn:{n}:response_prompt")).await;

                turns.push(serde_json::json!({
                    "n": n,
                    "user_input": user_input,
                    "instruct_prompt": instruct_prompt,
                    "instruction_response": instruction_response,
                    "tool_calls_json": tool_calls_json,
                    "tool_outputs_json": tool_outputs_json,
                    "context": context,
                    "response_prompt": response_prompt,
                }));
            }

            let body = serde_json::json!({
                "session_id": session_id,
                "turns": turns,
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
    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        // ── Subsystem-level health (must be intercepted before parse_method
        //    which would interpret "agents/health" as agent_id="health") ──────
        if method == "agents/health" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let h = match reporter {
                    Some(r) => r.get_current().await.unwrap_or_else(|| {
                        crate::supervisor::health::SubsystemHealth::ok("agents")
                    }),
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
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: resp.to_json(),
                }));
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
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: data.to_string(),
                }));
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
        if method == "agents/session" {
            let agent_id = match payload {
                BusPayload::SessionQuery {
                    agent_id: Some(id), ..
                } => id,
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32600,
                        "agents/session requires agent_id in SessionQuery payload",
                    )));
                    return;
                }
            };
            self.handle_agent_session(agent_id, reply_tx);
            return;
        }
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
        if method == "agents/sessions/debug" {
            self.handle_session_debug(payload, reply_tx);
            return;
        }

        // ── Agent routing ───────────────────────────────────────────
        let (method_agent_id, action) = match parse_method(method) {
            Ok(v) => v,
            Err(e) => {
                let _ = reply_tx.send(Err(e));
                return;
            }
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
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                        data: resp.to_json(),
                    }));
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
                        let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                            data: resp.to_json(),
                        }));
                        return;
                    }
                    let kv_path = identities
                        .get(&id)
                        .map(|ident| ident.identity_dir.join("store").join("kv.json"));
                    let last_fetched = kv_path
                        .as_ref()
                        .and_then(|p| read_agent_kv_value(p, "last_fetched"));
                    let index_path = identities
                        .get(&id)
                        .map(|ident| ident.identity_dir.join("sessions.json"));
                    let session_count = index_path
                        .as_ref()
                        .map(|p| count_agent_sessions(p))
                        .unwrap_or(0);
                    let data = serde_json::json!({
                        "id": id,
                        "status": "running",
                        "state": "on",
                        "session_count": session_count,
                        "last_fetched": last_fetched,
                    });
                    let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                        data: data.to_string(),
                    }));
                });
                return;
            }
        }

        match payload {
            BusPayload::CommsMessage {
                channel_id,
                content,
                session_id,
                ..
            } => {
                let agent_id = match self.resolve_agent(method_agent_id.as_deref(), &channel_id) {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = reply_tx.send(Err(e));
                        return;
                    }
                };
                match self.agents.get(agent_id) {
                    Some(reg) => reg.agent.handle(
                        action,
                        channel_id,
                        content,
                        session_id,
                        reply_tx,
                        self.state.clone(),
                    ),
                    None => {
                        let _ = reply_tx.send(Err(BusError::new(
                            ERR_METHOD_NOT_FOUND,
                            format!("agent not loaded: {agent_id}"),
                        )));
                    }
                }
            }
            BusPayload::CommsStreamRequest {
                channel_id,
                content,
                session_id,
            } => {
                let agent_id = match self.resolve_agent(method_agent_id.as_deref(), &channel_id) {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = reply_tx.send(Err(e));
                        return;
                    }
                };
                match self.agents.get(agent_id) {
                    Some(reg) => reg.agent.handle_stream(
                        channel_id,
                        content,
                        session_id,
                        reply_tx,
                        self.state.clone(),
                    ),
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
    let data = std::fs::read_to_string(sessions_root.join(session_id).join("spend.json")).ok()?;
    #[derive(serde::Deserialize)]
    struct SpendTs {
        last_updated: String,
    }
    serde_json::from_str::<SpendTs>(&data)
        .ok()
        .map(|s| s.last_updated)
}

/// Read a single string value from an AgentStore `kv.json` file.
fn read_agent_kv_value(kv_path: &std::path::Path, key: &str) -> Option<String> {
    let data = std::fs::read_to_string(kv_path).ok()?;
    #[derive(serde::Deserialize)]
    struct KvPartial {
        values: std::collections::HashMap<String, String>,
    }
    serde_json::from_str::<KvPartial>(&data)
        .ok()?
        .values
        .remove(key)
}

/// Return the number of sessions in an agent's `sessions.json` index.
fn count_agent_sessions(index_path: &std::path::Path) -> usize {
    if !index_path.exists() {
        return 0;
    }
    let data = match std::fs::read_to_string(index_path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    #[derive(serde::Deserialize)]
    struct Idx {
        sessions: std::collections::HashMap<String, serde_json::Value>,
    }
    serde_json::from_str::<Idx>(&data)
        .map(|i| i.sessions.len())
        .unwrap_or(0)
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
    use crate::subsystems::agents::core::AgentRuntimeClass;
    use crate::supervisor::bus::{BusMessage, SupervisorBus};
    use crate::supervisor::dispatch::BusHandler;
    use tokio::sync::oneshot;

    fn echo_bus() -> (SupervisorBus, BusHandle) {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        (bus, handle)
    }

    /// Create a throwaway `MemorySystem` backed by a temporary directory.
    /// The returned `TempDir` must be kept alive for the duration of the test.
    fn test_memory() -> (tempfile::TempDir, Arc<MemorySystem>) {
        let dir = tempfile::TempDir::new().unwrap();
        let mem = MemorySystem::new(
            dir.path(),
            crate::subsystems::memory::MemoryConfig::default(),
        )
        .unwrap();
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hello".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "mapped".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/unknown",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hi".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "fallback".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hi".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            if let Some(BusMessage::Request {
                payload, reply_tx, ..
            }) = rx.recv().await
            {
                if let BusPayload::LlmRequest {
                    channel_id,
                    content,
                    ..
                } = payload
                {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("[fake] {content}"),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "basic_chat".to_string(),
            enabled: HashSet::from(["basic_chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hello".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            if let Some(BusMessage::Request {
                method,
                payload,
                reply_tx,
                ..
            }) = rx.recv().await
            {
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
            if let Some(BusMessage::Request {
                method,
                payload,
                reply_tx,
                ..
            }) = rx.recv().await
            {
                assert_eq!(method, "llm/complete");
                if let BusPayload::LlmRequest {
                    content,
                    channel_id,
                    ..
                } = payload
                {
                    assert!(
                        content.contains("Test Headline"),
                        "prompt should contain subject"
                    );
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "Summary: Test Headline from news@example.com.".to_string(),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "news".to_string(),
            enabled: HashSet::from(["news".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert!(
                    content.contains("Summary:"),
                    "response should be LLM summary, got: {content}"
                );
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/health",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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

        // Fake LLM responder — handles two bus requests from the AgenticLoop:
        //   1. Instruction pass: return a docs_search tool call
        //   2. Response pass: reply with "brown"
        //
        // Note: we don't assert on prompt content because the prompt template
        // files live at the workspace root (config/prompts/) but binary tests
        // run from the package root (crates/araliya-bot/), so PromptBuilder
        // silently skips missing template files.
        tokio::spawn(async move {
            // ── Instruction pass ──────────────────────────────────────
            if let Some(BusMessage::Request {
                method,
                payload,
                reply_tx,
                ..
            }) = rx.recv().await
            {
                assert_eq!(method, "llm/complete");
                if let BusPayload::LlmRequest { channel_id, .. } = payload {
                    // Return a tool call that invokes docs_search.
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: r#"[{"tool":"docs_search","action":"search","params":{"query":"what color"}}]"#.to_string(),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }

            // ── Response pass ─────────────────────────────────────────
            if let Some(BusMessage::Request {
                method,
                payload,
                reply_tx,
                ..
            }) = rx.recv().await
            {
                assert_eq!(method, "llm/complete");
                if let BusPayload::LlmRequest { channel_id, .. } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "brown".to_string(),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "docs".to_string(),
            enabled: HashSet::from(["docs".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: HashMap::from([(
                "docs".to_string(),
                DocsAgentConfig {
                    docsdir: Some(docsdir),
                    index: Some("index.md".to_string()),
                    use_kg: false,
                    kg: crate::config::DocsKgConfig::default(),
                },
            )]),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        // Populate the docs docstore before handling any queries.
        agents.init_docs().await.unwrap();

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/ask",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "what color".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            // No docsdir configured — docstore will remain empty.
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        // Do NOT call init_docs — docstore stays empty.

        let (tx, rx) = oneshot::channel();
        agents.handle_request(
            "agents/docs/ask",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hi".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let subagent = agents
            .state
            .get_or_create_subagent("echo", "worker1")
            .unwrap();

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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
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
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
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

    // ── AgenticLoop integration tests ─────────────────────────────────────────
    //
    // These tests verify the full 3-phase instruction loop:
    //   1. Instruction pass (llm/instruct) → parse tool calls
    //   2. Tool execution (skipped when instruction returns [])
    //   3. Response pass (llm/complete) → reply with session_id
    //
    // The fake bus responder handles llm/instruct and llm/complete sequentially.
    // Instruction pass returning "[]" exercises graceful degradation (no tools called).

    /// Verifies agentic-chat completes the full 3-phase loop and returns a session_id.
    ///
    /// Fake LLM handles two requests:
    ///   - llm/instruct → "[]"  (no tool calls, graceful)
    ///   - llm/complete → "[fake] hello" (response)
    #[cfg(feature = "plugin-agentic-chat")]
    #[tokio::test]
    async fn agentic_chat_returns_session_id() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut bus_rx = bus.rx;
        let (_dir, memory) = test_memory();

        tokio::spawn(async move {
            // Request 1: llm/instruct → empty JSON array (no tools)
            if let Some(crate::supervisor::bus::BusMessage::Request {
                payload, reply_tx, ..
            }) = bus_rx.recv().await
            {
                if let BusPayload::LlmRequest { channel_id, .. } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "[]".to_string(),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }
            // Request 2: llm/complete → response
            if let Some(crate::supervisor::bus::BusMessage::Request {
                payload, reply_tx, ..
            }) = bus_rx.recv().await
            {
                if let BusPayload::LlmRequest { channel_id, .. } = payload {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "[fake] hello".to_string(),
                        session_id: None,
                        usage: None,

                        timing: None,

                        thinking: None,

                        cost_usd: None,
                    }));
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "agentic-chat".to_string(),
            enabled: HashSet::from(["agentic-chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: Some(crate::config::AgenticChatConfig {
                use_instruction_llm: false,
            }),
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hello".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage {
                content,
                session_id,
                ..
            }) => {
                assert_eq!(content, "[fake] hello");
                assert!(
                    session_id.is_some(),
                    "agentic loop must return a session_id"
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Verifies that a second message with the returned session_id reuses the same session.
    #[cfg(feature = "plugin-agentic-chat")]
    #[tokio::test]
    async fn agentic_chat_second_turn_reuses_session() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut bus_rx = bus.rx;
        let (_dir, memory) = test_memory();

        // Handle 4 sequential LLM requests (2 per turn: instruct + complete).
        tokio::spawn(async move {
            for i in 0..4u32 {
                if let Some(crate::supervisor::bus::BusMessage::Request {
                    payload, reply_tx, ..
                }) = bus_rx.recv().await
                {
                    if let BusPayload::LlmRequest { channel_id, .. } = payload {
                        let content = if i % 2 == 0 {
                            "[]".to_string() // instruct pass → no tools
                        } else {
                            format!("[fake-turn-{}] response", i / 2 + 1)
                        };
                        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                            channel_id,
                            content,
                            session_id: None,
                            usage: None,

                            timing: None,

                            thinking: None,

                            cost_usd: None,
                        }));
                    }
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "agentic-chat".to_string(),
            enabled: HashSet::from(["agentic-chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: Some(crate::config::AgenticChatConfig {
                use_instruction_llm: false,
            }),
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        // Turn 1 — no session_id provided.
        let (tx1, rx1) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "first message".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx1,
        );
        let session_id = match rx1.await.unwrap() {
            Ok(BusPayload::CommsMessage { session_id, .. }) => {
                session_id.expect("first turn must return a session_id")
            }
            other => panic!("unexpected response: {other:?}"),
        };

        // Turn 2 — pass the session_id back.
        let (tx2, rx2) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "second message".to_string(),
                session_id: Some(session_id.clone()),
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx2,
        );
        match rx2.await.unwrap() {
            Ok(BusPayload::CommsMessage {
                session_id: sid2, ..
            }) => {
                assert_eq!(
                    sid2,
                    Some(session_id),
                    "session_id must be reused across turns"
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Verifies debug_logging writes turn KV data when enabled.
    ///
    /// Checks that the session returned by the loop has a session_id — the actual
    /// KV key contents are verifiable via the debug API endpoint once it is wired up.
    /// This test ensures the flag does not break the loop.
    #[cfg(feature = "plugin-agentic-chat")]
    #[tokio::test]
    async fn agentic_chat_debug_logging_does_not_break_loop() {
        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut bus_rx = bus.rx;
        let (_dir, memory) = test_memory();

        tokio::spawn(async move {
            for _ in 0..2u32 {
                if let Some(crate::supervisor::bus::BusMessage::Request {
                    payload, reply_tx, ..
                }) = bus_rx.recv().await
                {
                    if let BusPayload::LlmRequest { channel_id, .. } = payload {
                        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                            channel_id,
                            content: "[]".to_string(),
                            session_id: None,
                            usage: None,

                            timing: None,

                            thinking: None,

                            cost_usd: None,
                        }));
                    }
                }
            }
        });

        let cfg = AgentsConfig {
            default_agent: "agentic-chat".to_string(),
            enabled: HashSet::from(["agentic-chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: Some(crate::config::AgenticChatConfig {
                use_instruction_llm: false,
            }),
            runtime_cmd: None,
            debug_logging: true,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "test debug".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { session_id, .. }) => {
                assert!(
                    session_id.is_some(),
                    "debug_logging=true must not break session creation"
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    // ── runtime_cmd tests ─────────────────────────────────────────────────────

    /// When `runtime_cmd` is the default and only enabled agent, an unrouted
    /// `CommsMessage` must reach the runtime_cmd plugin, trigger `runtimes/init`
    /// then `runtimes/exec` on the bus, and return the formatted stdout.
    #[cfg(feature = "plugin-runtime-cmd")]
    #[tokio::test]
    async fn runtime_cmd_is_routed_as_default_and_execs() {
        use crate::config::RuntimeCmdAgentConfig;

        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        // Fake runtimes subsystem: respond to init then exec.
        tokio::spawn(async move {
            // First message: runtimes/init — just ack success.
            if let Some(BusMessage::Request { reply_tx, .. }) = rx.recv().await {
                let init_result = serde_json::json!({
                    "success": true,
                    "exit_code": null,
                    "stdout": "",
                    "stderr": "",
                    "runtime_dir": "/tmp/test"
                })
                .to_string();
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: init_result }));
            }
            // Second message: runtimes/exec — return "2" as stdout.
            if let Some(BusMessage::Request { reply_tx, .. }) = rx.recv().await {
                let exec_result = serde_json::json!({
                    "success": true,
                    "exit_code": 0,
                    "stdout": "2\n",
                    "stderr": "",
                    "duration_ms": 5
                })
                .to_string();
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data: exec_result }));
            }
        });

        let cfg = AgentsConfig {
            default_agent: "runtime_cmd".to_string(),
            enabled: HashSet::from(["runtime_cmd".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: Some(RuntimeCmdAgentConfig {
                runtime: "node".to_string(),
                command: "node".to_string(),
                setup_script: None,
            }),
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "console.log(1+1)".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "2"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    // ── v0.6 PR1: Runtime class registration tests ────────────────────────────
    //
    // These tests verify that each built-in agent is registered with the
    // expected runtime class.  They are the acceptance tests for the first-class
    // runtime classification layer introduced in agents v0.6 PR1.
    //
    // Ground rules verified here:
    //   - Existing agent behavior is not changed (routing tests above still pass)
    //   - Runtime class mapping matches the PR1 spec table
    //   - AgentRegistration wraps the agent without changing its ID

    #[cfg(feature = "plugin-echo")]
    #[tokio::test]
    async fn echo_agent_classified_as_request_response() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("echo")
            .expect("echo must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::RequestResponse);
        assert_eq!(reg.agent.id(), "echo");
    }

    #[cfg(feature = "plugin-basic-chat")]
    #[tokio::test]
    async fn basic_chat_agent_classified_as_request_response() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "basic_chat".to_string(),
            enabled: HashSet::from(["basic_chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("basic_chat")
            .expect("basic_chat must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::RequestResponse);
        assert_eq!(reg.agent.id(), "basic_chat");
    }

    #[cfg(feature = "plugin-chat")]
    #[tokio::test]
    async fn session_chat_agent_classified_as_session() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "chat".to_string(),
            enabled: HashSet::from(["chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("chat")
            .expect("chat must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::Session);
        assert_eq!(reg.agent.id(), "chat");
    }

    #[cfg(feature = "plugin-agentic-chat")]
    #[tokio::test]
    async fn agentic_chat_agent_classified_as_agentic() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "agentic-chat".to_string(),
            enabled: HashSet::from(["agentic-chat".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("agentic-chat")
            .expect("agentic-chat must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::Agentic);
        assert_eq!(reg.agent.id(), "agentic-chat");
    }

    #[cfg(feature = "plugin-news-agent")]
    #[tokio::test]
    async fn news_agent_classified_as_specialized() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string(), "news".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("news")
            .expect("news must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::Specialized);
        assert_eq!(reg.agent.id(), "news");
    }

    #[cfg(feature = "plugin-runtime-cmd")]
    #[tokio::test]
    async fn runtime_cmd_agent_classified_as_specialized() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string(), "runtime_cmd".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();
        let reg = subsystem
            .agents
            .get("runtime_cmd")
            .expect("runtime_cmd must be registered");
        assert_eq!(reg.runtime_class, AgentRuntimeClass::Specialized);
        assert_eq!(reg.agent.id(), "runtime_cmd");
    }

    /// Verifies that `agents/list` includes a `runtime_class` field for each agent.
    #[cfg(feature = "plugin-echo")]
    #[tokio::test]
    async fn agents_list_includes_runtime_class() {
        let (_bus, handle) = echo_bus();
        let (_dir, memory) = test_memory();
        let cfg = AgentsConfig {
            default_agent: "echo".to_string(),
            enabled: HashSet::from(["echo".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let subsystem = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx) = oneshot::channel();
        subsystem.handle_request("agents/list", BusPayload::Empty, tx);
        let payload = rx.await.unwrap().unwrap();
        let BusPayload::JsonResponse { data } = payload else {
            panic!("expected JsonResponse");
        };
        let value: serde_json::Value = serde_json::from_str(&data).unwrap();
        let agents = value["agents"].as_array().expect("agents array");
        let echo_entry = agents
            .iter()
            .find(|e| e["agent_id"] == "echo")
            .expect("echo entry in list");
        assert_eq!(
            echo_entry["runtime_class"],
            serde_json::Value::String("request_response".to_string()),
            "echo must be listed with runtime_class=request_response"
        );
    }

    /// When `runtimes/exec` returns a non-zero exit code the reply must contain
    /// the error message from stderr.
    #[cfg(feature = "plugin-runtime-cmd")]
    #[tokio::test]
    async fn runtime_cmd_exec_error_is_formatted() {
        use crate::config::RuntimeCmdAgentConfig;

        let bus = SupervisorBus::new(16);
        let handle = bus.handle.clone();
        let mut rx = bus.rx;
        let (_dir, memory) = test_memory();

        tokio::spawn(async move {
            // init
            if let Some(BusMessage::Request { reply_tx, .. }) = rx.recv().await {
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: serde_json::json!({
                        "success": true, "exit_code": null,
                        "stdout": "", "stderr": "", "runtime_dir": "/tmp/test"
                    })
                    .to_string(),
                }));
            }
            // exec — simulate a syntax error
            if let Some(BusMessage::Request { reply_tx, .. }) = rx.recv().await {
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: serde_json::json!({
                        "success": false, "exit_code": 1,
                        "stdout": "", "stderr": "SyntaxError: Unexpected token\n",
                        "duration_ms": 3
                    })
                    .to_string(),
                }));
            }
        });

        let cfg = AgentsConfig {
            default_agent: "runtime_cmd".to_string(),
            enabled: HashSet::from(["runtime_cmd".to_string()]),
            channel_map: HashMap::new(),
            agent_memory: HashMap::new(),
            agent_skills: HashMap::new(),
            news_query: None,
            agent_docs: std::collections::HashMap::new(),
            agentic_chat: None,
            runtime_cmd: Some(RuntimeCmdAgentConfig {
                runtime: "node".to_string(),
                command: "node".to_string(),
                setup_script: None,
            }),
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
        };
        let agents = AgentsSubsystem::new(cfg, handle, memory).unwrap();

        let (tx, rx_reply) = oneshot::channel();
        agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "console.log(1+1".to_string(),
                session_id: None,
                usage: None,

                timing: None,

                thinking: None,

                cost_usd: None,
            },
            tx,
        );

        match rx_reply.await.unwrap() {
            Ok(BusPayload::CommsMessage { content, .. }) => {
                assert!(content.contains("Error (exit 1)"), "got: {content}");
                assert!(content.contains("SyntaxError"), "got: {content}");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
