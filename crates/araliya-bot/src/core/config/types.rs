//! Public configuration types.
//!
//! These are the resolved, ready-to-use structs that subsystems consume.
//! Raw TOML deserialization types live in `raw.rs`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ── Comms ───────────────────────────────────────────────────────────────────

/// PTY (console) channel configuration.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    /// Whether the PTY channel is explicitly enabled.
    pub enabled: bool,
}

/// Telegram channel configuration.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Whether the Telegram channel is explicitly enabled.
    pub enabled: bool,
}

/// HTTP channel configuration.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Whether the HTTP channel is explicitly enabled.
    pub enabled: bool,
    /// Socket address to bind the HTTP channel to.
    pub bind: String,
}

/// Axum HTTP channel configuration.
#[derive(Debug, Clone)]
pub struct AxumChannelConfig {
    /// Whether the axum channel is explicitly enabled.
    pub enabled: bool,
    /// Socket address to bind the axum listener to.
    pub bind: String,
}

/// Comms subsystem configuration.
#[derive(Debug, Clone)]
pub struct CommsConfig {
    pub pty: PtyConfig,
    pub telegram: TelegramConfig,
    pub http: HttpConfig,
    pub axum_channel: AxumChannelConfig,
}

// ── UI ───────────────────────────────────────────────────────────────────────

/// SvUI (Svelte web UI) configuration.
#[derive(Debug, Clone)]
pub struct SvuiConfig {
    /// Whether the svui backend is explicitly enabled.
    pub enabled: bool,
    /// Optional path to the static build directory.
    pub static_dir: Option<String>,
}

/// UI subsystem configuration.
#[derive(Debug, Clone)]
pub struct UiConfig {
    pub svui: SvuiConfig,
}

// ── Tools ────────────────────────────────────────────────────────────────────

/// Specialized newsmail aggregator tool defaults.
#[derive(Debug, Clone)]
pub struct NewsmailAggregatorConfig {
    /// Label IDs to filter by (e.g. ["INBOX"] or ["Label_xxx"]).
    pub label_ids: Vec<String>,
    pub n_last: usize,
    pub tsec_last: Option<u64>,
    /// Free-form Gmail search terms (e.g. "is:unread").
    pub q: Option<String>,
}

/// Tools subsystem configuration.
#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub newsmail_aggregator: NewsmailAggregatorConfig,
}

// ── Runtimes ─────────────────────────────────────────────────────────────────

/// Runtimes subsystem configuration.
#[derive(Debug, Clone)]
pub struct RuntimesConfig {
    /// Whether the runtimes subsystem is enabled.
    pub enabled: bool,
    /// Default per-execution timeout in seconds (used when the request
    /// does not specify its own `timeout_secs`).
    pub default_timeout_secs: u64,
}

// ── LLM ──────────────────────────────────────────────────────────────────────

/// OpenAI / OpenAI-compatible provider configuration.
/// Populated from `[llm.openai]` in the TOML.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// Full chat completions endpoint URL.
    pub api_base_url: String,
    /// Model name passed in the request body.
    pub model: String,
    /// Sampling temperature (ignored for models that forbid it).
    pub temperature: f32,
    /// Per-request HTTP timeout in seconds.
    pub timeout_seconds: u64,
    /// Maximum output tokens (0 = no limit).
    pub max_tokens: usize,
    /// Token pricing rates (USD per 1 million tokens).
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cached_input_per_million_usd: f64,
}

/// Qwen provider configuration.
/// Populated from `[llm.qwen]` in the TOML.
#[derive(Debug, Clone)]
pub struct QwenConfig {
    /// Full chat completions endpoint URL.
    pub api_base_url: String,
    /// Model name passed in the request body.
    pub model: String,
    /// Sampling temperature.
    pub temperature: f32,
    /// Per-request HTTP timeout in seconds.
    pub timeout_seconds: u64,
    /// Maximum output tokens (limits prompt length to stay within context window).
    pub max_tokens: usize,
    /// Token pricing rates (USD per 1 million tokens).
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    pub cached_input_per_million_usd: f64,
}

/// LLM subsystem configuration.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Which provider is active (e.g. `"dummy"`, `"openai"`).
    pub provider: String,
    /// Config for the OpenAI / OpenAI-compatible provider (`[llm.openai]`).
    pub openai: OpenAiConfig,
    /// Config for the Qwen provider (`[llm.qwen]`).
    pub qwen: QwenConfig,
    /// Optional separate LLM for the instruction pass (`[llm.instruction]`).
    ///
    /// When present, `llm/instruct` bus requests are routed to this provider
    /// rather than the main one.  Boxed to keep the struct size manageable.
    pub instruction: Option<Box<LlmConfig>>,
}

// ── Agents ───────────────────────────────────────────────────────────────────

/// Optional query defaults for the `news` agent.
#[derive(Debug, Clone)]
pub struct NewsAgentQueryConfig {
    pub label: Option<String>,
    pub n_last: Option<usize>,
    pub t_interval: Option<String>,
    pub tsec_last: Option<u64>,
    pub q: Option<String>,
}

/// Optional query defaults for the `gdelt_news` agent.
#[derive(Debug, Clone)]
pub struct GdeltAgentQueryConfig {
    /// How many minutes back to include (default 60).
    pub lookback_minutes: Option<u32>,
    /// Maximum rows to return (default 50).
    pub limit: Option<u32>,
    /// Only include events with at least this many articles.
    pub min_articles: Option<u32>,
    /// Only include events with ABS(GoldsteinScale) >= this value (0–10).
    pub min_importance: Option<f32>,
    /// Sort by ABS(GoldsteinScale) DESC then NumArticles DESC when true.
    pub sort_by_importance: Option<bool>,
    /// Restrict to events covered by English-language sources (eventmentions join).
    pub english_only: Option<bool>,
}

/// Tuning parameters for the docs-agent KG pipeline.
#[derive(Debug, Clone)]
pub struct DocsKgConfig {
    pub min_entity_mentions: usize,
    pub bfs_max_depth: usize,
    pub edge_weight_threshold: f32,
    pub max_chunks: usize,
    pub fts_share: f32,
    pub max_seeds: usize,
}

impl Default for DocsKgConfig {
    fn default() -> Self {
        Self {
            min_entity_mentions: 2,
            bfs_max_depth: 2,
            edge_weight_threshold: 0.15,
            max_chunks: 8,
            fts_share: 0.5,
            max_seeds: 5,
        }
    }
}

/// Configuration for the docs agent.
#[derive(Debug, Clone)]
pub struct DocsAgentConfig {
    /// Directory containing the documentation tree to import into memory.
    pub docsdir: Option<String>,
    /// Relative path of the index document inside `docsdir`.
    pub index: Option<String>,
    /// Enable the KG-RAG pipeline.
    pub use_kg: bool,
    /// Tuning parameters for the KG pipeline.
    pub kg: DocsKgConfig,
}

/// Configuration for the `agentic-chat` agent plugin.
#[derive(Debug, Clone)]
pub struct AgenticChatConfig {
    /// When `true`, the instruction pass is routed through `llm/instruct`
    /// (uses the instruction LLM if configured, falls back to the main LLM).
    /// When `false`, the instruction pass goes through `llm/complete` directly.
    pub use_instruction_llm: bool,
}

/// Configuration for the `webbuilder` agent plugin.
#[derive(Debug, Clone)]
pub struct WebBuilderAgentConfig {
    /// Maximum LLM-tool iteration cycles before the agent gives up (default: 10).
    pub max_iterations: usize,
    /// Scaffold type used for the initial workspace setup.
    /// Currently only `"vite-svelte"` is supported.
    pub scaffold: String,
}

impl Default for WebBuilderAgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            scaffold: "vite-svelte".to_string(),
        }
    }
}

/// Configuration for the `runtime_cmd` agent plugin.
#[derive(Debug, Clone)]
pub struct RuntimeCmdAgentConfig {
    /// Runtime environment name (default: `"bash"`).
    pub runtime: String,
    /// Interpreter command (default: `"bash"`).
    pub command: String,
    /// Optional init script run on first interaction.
    pub setup_script: Option<String>,
}

impl Default for RuntimeCmdAgentConfig {
    fn default() -> Self {
        Self {
            runtime: "bash".to_string(),
            command: "bash".to_string(),
            setup_script: None,
        }
    }
}

/// Agents subsystem configuration.
///
/// # v0.6 extension points (PR2 — StaticAgent config)
///
/// The following fields are planned for PR2 and are not present yet:
///
/// - `static_agents: Vec<StaticAgentConfig>` — startup-defined agents loaded
///   from TOML.  Each entry will carry: agent ID, `kind = "static"`, runtime
///   class, memory requirements, tool allowlist, and prompt file references.
///   See `docs/architecture/subsystems/agents_v0.6.md` for the full schema.
///
/// When PR2 lands, `raw.rs` will gain a corresponding `RawStaticAgentConfig`
/// type, and `load.rs` will map it through validation into a typed
/// `StaticAgentConfig` here.  Supported runtime classes for static agents will
/// be `RequestResponse`, `Session`, and `Agentic` only — `Workflow` and
/// `Background` are deferred to later phases.
// TODO(PR2): add `pub static_agents: Vec<StaticAgentConfig>` here.
// TODO(PR2): add `StaticAgentConfig` struct with id, runtime_class, memory,
//            skills, prompt_files fields.
// TODO(PR2): add validation in load.rs that rejects Workflow/Background classes
//            for static agents in this phase.
#[derive(Debug, Clone)]
pub struct AgentsConfig {
    /// Agent that handles messages with no explicit routing.
    pub default_agent: String,
    /// channel_id -> agent_id overrides (from `[agents.routing]`).
    pub channel_map: HashMap<String, String>,
    /// Set of agent IDs whose config section has `enabled` != false.
    pub enabled: HashSet<String>,
    /// Per-agent memory store requirements: agent_id -> list of store type names.
    pub agent_memory: HashMap<String, Vec<String>>,
    /// Optional default query args for the `news` agent.
    pub news_query: Option<NewsAgentQueryConfig>,
    /// Optional default query args for the `gdelt_news` agent.
    pub gdelt_query: Option<GdeltAgentQueryConfig>,
    /// Optional default query args for the `newsroom` agent.
    pub newsroom_query: Option<GdeltAgentQueryConfig>,
    /// Per-agent docstore configuration: agent_id → docs config.
    /// Any agent with `docsdir` set in its config section gets an entry here.
    pub agent_docs: HashMap<String, DocsAgentConfig>,
    /// Optional configuration for the `agentic-chat` agent.
    pub agentic_chat: Option<AgenticChatConfig>,
    /// Optional configuration for the `runtime_cmd` agent.
    pub runtime_cmd: Option<RuntimeCmdAgentConfig>,
    /// Optional configuration for the `webbuilder` agent.
    pub webbuilder: Option<WebBuilderAgentConfig>,
    /// Per-agent bus-tool allowlists: agent_id → list of tool names the agent
    /// may invoke.  Populated from `skills = [...]` in each `[agents.<id>]`
    /// config section.  Agents without an entry default to no bus tools.
    pub agent_skills: HashMap<String, Vec<String>>,
    /// Enable per-turn debug logging to the session KV store.
    ///
    /// When `true`, each `AgenticLoop` turn writes intermediate data
    /// (`instruct_prompt`, `instruction_response`, `tool_calls_json`, etc.)
    /// under `debug:turn:{n}:*` KV keys.  Off by default.
    pub debug_logging: bool,
    /// Optional explicit session ID for the `uniweb` shared-session agent.
    /// If `None` or empty, a deterministic ID is derived automatically.
    pub uniweb_session_id: Option<String>,
    /// Whether the `uniweb` agent routes its instruction pass through
    /// `llm/instruct`.  Defaults to `false`.
    pub uniweb_use_instruction_llm: bool,
    /// Source-agent → aggregator-agent mapping (e.g. "newsroom" → "news_aggregator").
    /// Used by source agents to dispatch article URLs to an aggregator for KG processing.
    pub agent_aggregation_targets: HashMap<String, String>,
    // TODO(PR2): static_agents: Vec<StaticAgentConfig>,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            default_agent: "echo".to_string(),
            channel_map: HashMap::new(),
            enabled: HashSet::new(),
            agent_memory: HashMap::new(),
            news_query: None,
            gdelt_query: None,
            newsroom_query: None,
            agent_docs: HashMap::new(),
            agentic_chat: None,
            runtime_cmd: None,
            webbuilder: None,
            agent_skills: HashMap::new(),
            debug_logging: false,
            uniweb_session_id: None,
            uniweb_use_instruction_llm: false,
            agent_aggregation_targets: HashMap::new(),
        }
    }
}

// ── Config (root) ────────────────────────────────────────────────────────────

/// Fully-resolved supervisor configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bot_name: String,
    /// Working directory for all persistent data (already expanded, no `~`).
    pub work_dir: PathBuf,
    /// Optional explicit identity directory (absolute path or relative to `work_dir`).
    pub identity_dir: Option<PathBuf>,
    pub log_level: String,
    pub comms: CommsConfig,
    pub agents: AgentsConfig,
    pub llm: LlmConfig,
    pub ui: UiConfig,
    pub tools: ToolsConfig,
    pub runtimes: RuntimesConfig,
    /// API key from `LLM_API_KEY` env var — never sourced from TOML.
    pub llm_api_key: Option<String>,
    /// Memory subsystem caps (from `[memory.basic_session]`).
    pub memory_kv_cap: Option<usize>,
    pub memory_transcript_cap: Option<usize>,
}

impl Config {
    /// Returns `true` if the PTY channel should be loaded.
    pub fn comms_pty_should_load(&self) -> bool {
        self.comms.pty.enabled
    }

    /// Returns `true` if the Telegram channel should be loaded.
    pub fn comms_telegram_should_load(&self) -> bool {
        self.comms.telegram.enabled
    }

    /// Returns `true` if the HTTP channel should be loaded.
    pub fn comms_http_should_load(&self) -> bool {
        self.comms.http.enabled
    }

    /// Returns `true` if the axum channel should be loaded.
    pub fn comms_axum_should_load(&self) -> bool {
        self.comms.axum_channel.enabled
    }

    /// Returns `true` if the svui UI backend should be loaded.
    pub fn ui_svui_should_load(&self) -> bool {
        self.ui.svui.enabled
    }
}
