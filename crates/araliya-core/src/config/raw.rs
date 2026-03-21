//! Raw TOML deserialization types.
//!
//! These structs mirror the TOML file shape and use `serde` defaults.
//! The `load` module converts them into the public `types` structs.

use std::collections::HashMap;

use serde::Deserialize;

// ── Top-level ────────────────────────────────────────────────────────────────

/// Raw TOML shape — serde target before resolution.
#[derive(Deserialize)]
pub(super) struct RawConfig {
    pub supervisor: RawSupervisor,
    #[serde(default)]
    pub comms: RawComms,
    #[serde(default)]
    pub agents: RawAgents,
    #[serde(default)]
    pub llm: RawLlm,
    #[serde(default)]
    pub memory: RawMemory,
    #[serde(default)]
    pub ui: RawUi,
    #[serde(default)]
    pub tools: RawTools,
    #[serde(default)]
    pub runtimes: RawRuntimes,
}

#[derive(Deserialize)]
pub(super) struct RawSupervisor {
    pub bot_name: String,
    pub work_dir: String,
    #[serde(default)]
    pub identity_dir: Option<String>,
    pub log_level: String,
}

// ── Comms ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(super) struct RawComms {
    #[serde(default)]
    pub pty: RawPty,
    #[serde(default)]
    pub telegram: RawTelegram,
    #[serde(default)]
    pub http: RawHttp,
    #[serde(default)]
    pub axum_channel: RawAxumChannel,
}

#[derive(Deserialize)]
pub(super) struct RawPty {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Default)]
pub(super) struct RawTelegram {
    #[serde(default = "default_false")]
    pub enabled: bool,
}

#[derive(Deserialize)]
pub(super) struct RawHttp {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_http_bind")]
    pub bind: String,
}

#[derive(Deserialize)]
pub(super) struct RawAxumChannel {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_http_bind")]
    pub bind: String,
}

// ── LLM ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(super) struct RawLlm {
    #[serde(rename = "default", default = "default_llm_provider")]
    pub provider: String,
    #[serde(default)]
    pub openai: RawOpenAiConfig,
    #[serde(default)]
    pub qwen: RawQwenConfig,
    /// Optional separate small model for the instruction pass (`[llm.instruction]`).
    #[serde(default)]
    pub instruction: Option<RawLlmInstruction>,
}

impl Default for RawLlm {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            openai: RawOpenAiConfig::default(),
            qwen: RawQwenConfig::default(),
            instruction: None,
        }
    }
}

/// Configuration for the optional instruction-pass LLM (`[llm.instruction]`).
///
/// When present, `llm/instruct` bus requests are routed to this provider
/// instead of the main LLM.  Provider sub-configs (`openai`, `qwen`) are
/// optional — when absent, the corresponding values from `[llm.openai]` /
/// `[llm.qwen]` are inherited.  Only specify the sub-section when you need
/// to override a specific field (e.g. a different model or temperature).
#[derive(Deserialize, Default)]
pub(super) struct RawLlmInstruction {
    /// Provider name ("openai", "qwen", "dummy").  Defaults to main provider if empty.
    #[serde(default)]
    pub provider: String,
    /// Override fields for the OpenAI-compatible provider.
    /// Absent → inherit all values from `[llm.openai]`.
    #[serde(default)]
    pub openai: Option<RawOpenAiConfig>,
    /// Override fields for the Qwen provider.
    /// Absent → inherit all values from `[llm.qwen]`.
    #[serde(default)]
    pub qwen: Option<RawQwenConfig>,
}

#[derive(Deserialize)]
pub(super) struct RawOpenAiConfig {
    #[serde(default = "default_openai_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default = "default_openai_temperature")]
    pub temperature: f32,
    #[serde(default = "default_openai_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub max_tokens: usize,
    #[serde(default)]
    pub input_per_million_usd: f64,
    #[serde(default)]
    pub output_per_million_usd: f64,
    #[serde(default)]
    pub cached_input_per_million_usd: f64,
}

impl Default for RawOpenAiConfig {
    fn default() -> Self {
        Self {
            api_base_url: default_openai_api_base_url(),
            model: default_openai_model(),
            temperature: default_openai_temperature(),
            timeout_seconds: default_openai_timeout_seconds(),
            max_tokens: 0,
            input_per_million_usd: 0.0,
            output_per_million_usd: 0.0,
            cached_input_per_million_usd: 0.0,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RawQwenConfig {
    #[serde(default = "default_qwen_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_qwen_model")]
    pub model: String,
    #[serde(default = "default_qwen_temperature")]
    pub temperature: f32,
    #[serde(default = "default_qwen_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_qwen_max_tokens")]
    pub max_tokens: usize,
    #[serde(default)]
    pub input_per_million_usd: f64,
    #[serde(default)]
    pub output_per_million_usd: f64,
    #[serde(default)]
    pub cached_input_per_million_usd: f64,
}

impl Default for RawQwenConfig {
    fn default() -> Self {
        Self {
            api_base_url: default_qwen_api_base_url(),
            model: default_qwen_model(),
            temperature: default_qwen_temperature(),
            timeout_seconds: default_qwen_timeout_seconds(),
            max_tokens: default_qwen_max_tokens(),
            input_per_million_usd: 0.0,
            output_per_million_usd: 0.0,
            cached_input_per_million_usd: 0.0,
        }
    }
}

// ── Agents ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(super) struct RawAgents {
    #[serde(rename = "default", default = "default_agent_name")]
    pub default_agent: String,
    #[serde(default)]
    pub routing: HashMap<String, String>,
    /// Enable per-turn debug logging to the session KV store for agentic plugins.
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(flatten)]
    pub entries: HashMap<String, RawAgentEntry>,
}

#[derive(Deserialize)]
pub(super) struct RawAgentEntry {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub memory: Vec<String>,
    #[serde(default)]
    pub query: Option<RawNewsAgentQuery>,
    #[serde(default)]
    pub docsdir: Option<String>,
    #[serde(default)]
    pub index: Option<String>,
    #[serde(default)]
    pub use_kg: bool,
    #[serde(default)]
    pub kg: RawKgConfig,
    /// Whether the `agentic-chat` plugin should route the instruction pass
    /// through `llm/instruct` (requires `[llm.instruction]` to be configured).
    #[serde(default)]
    pub use_instruction_llm: bool,
    /// Bus tools this agent is allowed to invoke (e.g. `["gmail", "newsmail_aggregator"]`).
    /// Defaults to empty — the agent can only use its own local tools.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Runtime name for the `runtime_cmd` agent (e.g. `"node"`, `"bash"`).
    #[serde(default)]
    pub runtime: Option<String>,
    /// Interpreter command for the `runtime_cmd` agent (e.g. `"node"`, `"python3"`).
    #[serde(default)]
    pub command: Option<String>,
    /// Optional setup script run on first interaction.
    #[serde(default)]
    pub setup_script: Option<String>,
    /// Optional explicit global session ID for the `uniweb` agent.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Maximum LLM-tool iteration cycles for the `webbuilder` agent.
    #[serde(default)]
    pub max_iterations: Option<usize>,
    /// Scaffold type for the `webbuilder` agent (e.g. `"vite-svelte"`).
    #[serde(default)]
    pub scaffold: Option<String>,
    /// Query params for the `gdelt_news` agent.
    #[serde(default)]
    pub gdelt_query: Option<RawGdeltAgentQuery>,
    /// Target aggregator agent for dispatching article URLs (e.g. "news_aggregator").
    /// Used by agents like newsroom to feed URLs to an aggregator for KG processing.
    /// Defaults to "news_aggregator" if not specified.
    #[serde(default)]
    pub target_agent: Option<String>,
}

#[derive(Deserialize, Default)]
pub(super) struct RawKgConfig {
    #[serde(default)]
    pub min_entity_mentions: Option<usize>,
    #[serde(default)]
    pub bfs_max_depth: Option<usize>,
    #[serde(default)]
    pub edge_weight_threshold: Option<f32>,
    #[serde(default)]
    pub max_chunks: Option<usize>,
    #[serde(default)]
    pub fts_share: Option<f32>,
    #[serde(default)]
    pub max_seeds: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct RawNewsAgentQuery {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub n_last: Option<usize>,
    #[serde(default)]
    pub t_interval: Option<String>,
    #[serde(default)]
    pub tsec_last: Option<u64>,
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Deserialize, Default)]
pub(super) struct RawGdeltAgentQuery {
    /// How many minutes back to include (default 60).
    #[serde(default)]
    pub lookback_minutes: Option<u32>,
    /// Maximum rows to return (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Only include events with at least this many articles.
    #[serde(default)]
    pub min_articles: Option<u32>,
    /// Only include events with ABS(GoldsteinScale) >= this value (0–10).
    #[serde(default)]
    pub min_importance: Option<f32>,
    /// Sort by importance (ABS(GoldsteinScale)) rather than article count.
    #[serde(default)]
    pub sort_by_importance: Option<bool>,
    /// Restrict to English-language source mentions.
    #[serde(default)]
    pub english_only: Option<bool>,
}

// ── Memory ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(super) struct RawMemory {
    #[serde(default)]
    pub basic_session: RawBasicSessionConfig,
}

#[derive(Deserialize, Default)]
pub(super) struct RawBasicSessionConfig {
    pub kv_cap: Option<usize>,
    pub transcript_cap: Option<usize>,
}

// ── UI ───────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(super) struct RawUi {
    #[serde(default)]
    pub svui: RawSvui,
}

#[derive(Deserialize, Default)]
pub(super) struct RawSvui {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default)]
    pub static_dir: Option<String>,
}

// ── Tools ────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(super) struct RawTools {
    #[serde(default)]
    pub newsmail_aggregator: RawNewsmailAggregator,
}

#[derive(Deserialize)]
pub(super) struct RawNewsmailAggregator {
    #[serde(default = "default_newsmail_label_ids")]
    pub label_ids: Vec<String>,
    #[serde(default = "default_newsmail_n_last")]
    pub n_last: usize,
    #[serde(default)]
    pub tsec_last: Option<u64>,
    #[serde(default)]
    pub q: Option<String>,
}

impl Default for RawNewsmailAggregator {
    fn default() -> Self {
        Self {
            label_ids: default_newsmail_label_ids(),
            n_last: default_newsmail_n_last(),
            tsec_last: None,
            q: None,
        }
    }
}

// ── Runtimes ─────────────────────────────────────────────────────────────────

/// Raw config for the runtimes subsystem (`[runtimes]`).
#[derive(Deserialize)]
pub(super) struct RawRuntimes {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_runtimes_timeout")]
    pub default_timeout_secs: u64,
}

impl Default for RawRuntimes {
    fn default() -> Self {
        Self {
            enabled: true,
            default_timeout_secs: default_runtimes_timeout(),
        }
    }
}

// ── Default impls for serde ──────────────────────────────────────────────────

impl Default for RawPty {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for RawHttp {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_http_bind(),
        }
    }
}

impl Default for RawAxumChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_http_bind(),
        }
    }
}

// ── Default functions (used by serde) ────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

pub(super) fn default_http_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_llm_provider() -> String {
    "dummy".to_string()
}
fn default_openai_api_base_url() -> String {
    "https://api.openai.com/v1/chat/completions".to_string()
}
fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_openai_temperature() -> f32 {
    0.2
}
fn default_openai_timeout_seconds() -> u64 {
    60
}
fn default_qwen_api_base_url() -> String {
    "http://127.0.0.1:8081/v1/chat/completions".to_string()
}
fn default_qwen_model() -> String {
    "qwen2.5-instruct".to_string()
}
fn default_qwen_temperature() -> f32 {
    0.2
}
fn default_qwen_timeout_seconds() -> u64 {
    60
}
fn default_qwen_max_tokens() -> usize {
    8192
}

fn default_runtimes_timeout() -> u64 {
    30
}

fn default_agent_name() -> String {
    "basic_chat".to_string()
}

pub(super) fn default_newsmail_label_ids() -> Vec<String> {
    vec!["INBOX".to_string()]
}
pub(super) fn default_newsmail_n_last() -> usize {
    10
}
