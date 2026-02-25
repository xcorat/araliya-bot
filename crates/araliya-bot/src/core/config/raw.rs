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

#[derive(Deserialize)]
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
}

impl Default for RawLlm {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            openai: RawOpenAiConfig::default(),
            qwen: RawQwenConfig::default(),
        }
    }
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

#[derive(Deserialize)]
pub(super) struct RawSvui {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default)]
    pub static_dir: Option<String>,
}

impl Default for RawSvui {
    fn default() -> Self {
        Self {
            enabled: false,
            static_dir: None,
        }
    }
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

// ── Default impls for serde ──────────────────────────────────────────────────

impl Default for RawPty {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for RawTelegram {
    fn default() -> Self {
        Self { enabled: false }
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

fn default_agent_name() -> String {
    "basic_chat".to_string()
}

pub(super) fn default_newsmail_label_ids() -> Vec<String> {
    vec!["INBOX".to_string()]
}
pub(super) fn default_newsmail_n_last() -> usize {
    10
}
