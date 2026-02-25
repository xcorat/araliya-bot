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

/// Agents subsystem configuration.
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
    /// Optional configuration for the `docs` agent.
    pub docs: Option<DocsAgentConfig>,
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
