#![cfg_attr(test, allow(dead_code))]
//! Configuration loading with env-var overrides.
//!
//! Reads `config/default.toml` relative to the current working directory,
//! then applies `ARALIYA_WORK_DIR` and `ARALIYA_LOG_LEVEL` env overrides.

use std::{
    env,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    fs,
};

use serde::Deserialize;

use crate::error::AppError;

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

/// Comms subsystem configuration.
#[derive(Debug, Clone)]
pub struct CommsConfig {
    pub pty: PtyConfig,
    pub telegram: TelegramConfig,
    pub http: HttpConfig,
    pub axum_channel: AxumChannelConfig,
}

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

/// LLM subsystem configuration.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Which provider is active (e.g. `"dummy"`, `"openai"`).
    /// Maps to `default` in `[llm]` TOML — named `default` there to signal
    /// that other provider sections can coexist without being loaded.
    pub provider: String,
    /// Config for the OpenAI / OpenAI-compatible provider (`[llm.openai]`).
    pub openai: OpenAiConfig,
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
    /// Optional default query args for the `news` agent -> `newsmail_aggregator/get`.
    pub news_query: Option<NewsAgentQueryConfig>,
}

/// Optional query defaults for the `news` agent.
#[derive(Debug, Clone)]
pub struct NewsAgentQueryConfig {
    pub label: Option<String>,
    pub n_last: Option<usize>,
    pub t_interval: Option<String>,
    pub tsec_last: Option<u64>,
    pub q: Option<String>,
}

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
    /// API key from `LLM_API_KEY` env var — `None` for keyless local models.
    /// Never sourced from TOML.
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

/// Raw TOML shape — `serde` target before resolution.
#[derive(Deserialize)]
struct RawConfig {
    supervisor: RawSupervisor,
    #[serde(default)]
    comms: RawComms,
    #[serde(default)]
    agents: RawAgents,
    #[serde(default)]
    llm: RawLlm,
    #[serde(default)]
    memory: RawMemory,
    #[serde(default)]
    ui: RawUi,
    #[serde(default)]
    tools: RawTools,
}

#[derive(Deserialize)]
struct RawSupervisor {
    bot_name: String,
    work_dir: String,
    #[serde(default)]
    identity_dir: Option<String>,
    log_level: String,
}

#[derive(Deserialize, Default)]
struct RawComms {
    #[serde(default)]
    pty: RawPty,
    #[serde(default)]
    telegram: RawTelegram,
    #[serde(default)]
    http: RawHttp,
    #[serde(default)]
    axum_channel: RawAxumChannel,
}

#[derive(Deserialize)]
struct RawPty {
    /// Defaults to `true`: PTY auto-enables when no other channel is present.
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Deserialize)]
struct RawTelegram {
    /// Defaults to `false`: Telegram must be explicitly enabled.
    #[serde(default = "default_false")]
    enabled: bool,
}

#[derive(Deserialize)]
struct RawHttp {
    /// Defaults to `false`: HTTP must be explicitly enabled.
    #[serde(default = "default_false")]
    enabled: bool,
    /// Bind address for the HTTP listener.
    #[serde(default = "default_http_bind")]
    bind: String,
}

#[derive(Deserialize)]
struct RawLlm {
    /// Maps to `default = "..."` in `[llm]`.
    #[serde(rename = "default", default = "default_llm_provider")]
    provider: String,
    #[serde(default)]
    openai: RawOpenAiConfig,
}

impl Default for RawLlm {
    fn default() -> Self {
        Self { provider: default_llm_provider(), openai: RawOpenAiConfig::default() }
    }
}

#[derive(Deserialize)]
struct RawOpenAiConfig {
    #[serde(default = "default_openai_api_base_url")]
    api_base_url: String,
    #[serde(default = "default_openai_model")]
    model: String,
    #[serde(default = "default_openai_temperature")]
    temperature: f32,
    #[serde(default = "default_openai_timeout_seconds")]
    timeout_seconds: u64,
    #[serde(default)]
    input_per_million_usd: f64,
    #[serde(default)]
    output_per_million_usd: f64,
    #[serde(default)]
    cached_input_per_million_usd: f64,
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

fn default_llm_provider() -> String { "dummy".to_string() }
fn default_openai_api_base_url() -> String { "https://api.openai.com/v1/chat/completions".to_string() }
fn default_openai_model() -> String { "gpt-4o-mini".to_string() }
fn default_openai_temperature() -> f32 { 0.2 }
fn default_openai_timeout_seconds() -> u64 { 60 }

#[derive(Deserialize, Default)]
struct RawAgents {
    /// `default = "..."` in `[agents]` — which agent handles unrouted messages.
    #[serde(rename = "default", default = "default_agent_name")]
    default_agent: String,
    /// `[agents.routing]` — channel_id -> agent_id overrides.
    #[serde(default)]
    routing: HashMap<String, String>,
    /// All other `[agents.<id>]` subsections — one entry per configured agent.
    #[serde(flatten)]
    entries: HashMap<String, RawAgentEntry>,
}

#[derive(Deserialize)]
struct RawAgentEntry {
    /// Defaults to `true`; set to `false` to disable without removing the section.
    #[serde(default = "default_true")]
    enabled: bool,
    /// Memory store types this agent requires (e.g. `["basic_session"]`).
    #[serde(default)]
    memory: Vec<String>,
    /// Optional per-agent query defaults (used by `agents.news.query`).
    #[serde(default)]
    query: Option<RawNewsAgentQuery>,
}

#[derive(Deserialize)]
struct RawNewsAgentQuery {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    n_last: Option<usize>,
    #[serde(default)]
    t_interval: Option<String>,
    #[serde(default)]
    tsec_last: Option<u64>,
    #[serde(default)]
    q: Option<String>,
}

fn default_agent_name() -> String { "basic_chat".to_string() }

#[derive(Deserialize, Default)]
struct RawMemory {
    #[serde(default)]
    basic_session: RawBasicSessionConfig,
}

#[derive(Deserialize, Default)]
struct RawBasicSessionConfig {
    kv_cap: Option<usize>,
    transcript_cap: Option<usize>,
}

#[derive(Deserialize, Default)]
struct RawUi {
    #[serde(default)]
    svui: RawSvui,
}

#[derive(Deserialize, Default)]
struct RawTools {
    #[serde(default)]
    newsmail_aggregator: RawNewsmailAggregator,
}

#[derive(Deserialize)]
struct RawNewsmailAggregator {
    #[serde(default = "default_newsmail_label_ids")]
    label_ids: Vec<String>,
    #[serde(default = "default_newsmail_n_last")]
    n_last: usize,
    #[serde(default)]
    tsec_last: Option<u64>,
    #[serde(default)]
    q: Option<String>,
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

fn default_newsmail_label_ids() -> Vec<String> { vec!["INBOX".to_string()] }
fn default_newsmail_n_last() -> usize { 10 }

#[derive(Deserialize)]
struct RawSvui {
    #[serde(default = "default_false")]
    enabled: bool,
    #[serde(default)]
    static_dir: Option<String>,
}

impl Default for RawSvui {
    fn default() -> Self {
        Self {
            enabled: false,
            static_dir: None,
        }
    }
}

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

#[derive(Deserialize)]
struct RawAxumChannel {
    #[serde(default = "default_false")]
    enabled: bool,
    #[serde(default = "default_http_bind")]
    bind: String,
}

impl Default for RawAxumChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_http_bind(),
        }
    }
}

fn default_http_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

/// Deep-merge two TOML values.
/// Tables are merged recursively — the overlay only needs to specify keys that
/// differ from the base.  For every other type (string, integer, array, …)
/// the overlay value replaces the base value wholesale.
fn merge_toml(base: toml::Value, overlay: toml::Value) -> toml::Value {
    match (base, overlay) {
        (toml::Value::Table(mut base_tbl), toml::Value::Table(overlay_tbl)) => {
            for (key, ov_val) in overlay_tbl {
                let merged = match base_tbl.remove(&key) {
                    Some(base_val) => merge_toml(base_val, ov_val),
                    None => ov_val,
                };
                base_tbl.insert(key, merged);
            }
            toml::Value::Table(base_tbl)
        }
        (_, overlay) => overlay,
    }
}

/// Read a config file, follow any `[meta] base = "..."` chain, and return the
/// fully merged `toml::Value`.  `visited` carries canonicalized paths already
/// seen in this chain so circular references are caught early.
fn load_raw_merged(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<toml::Value, AppError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical) {
        return Err(AppError::Config(format!(
            "circular base reference detected at: {}",
            path.display()
        )));
    }

    let raw = fs::read_to_string(path)
        .map_err(|e| AppError::Config(format!("cannot read {}: {e}", path.display())))?;

    let overlay_val: toml::Value = toml::from_str(&raw)
        .map_err(|e| AppError::Config(format!("parse error in {}: {e}", path.display())))?;

    if let Some(base_str) = overlay_val
        .get("meta")
        .and_then(|m| m.get("base"))
        .and_then(|b| b.as_str())
    {
        let base_path = if Path::new(base_str).is_absolute() {
            PathBuf::from(base_str)
        } else {
            path.parent().unwrap_or(Path::new(".")).join(base_str)
        };
        let base_val = load_raw_merged(&base_path, visited)?;
        Ok(merge_toml(base_val, overlay_val))
    } else {
        Ok(overlay_val)
    }
}

/// Load config from the given path, or `config/default.toml`, then apply env-var overrides.
/// If no path is given and `config/default.toml` does not exist, returns a hardcoded minimal default.
pub fn load(config_path: Option<&str>) -> Result<Config, AppError> {
    let work_dir_override = env::var("ARALIYA_WORK_DIR").ok();
    let log_level_override = env::var("ARALIYA_LOG_LEVEL").ok();

    if let Some(path) = config_path {
        // If explicitly provided, it must exist and be valid.
        return load_from(
            Path::new(path),
            work_dir_override.as_deref(),
            log_level_override.as_deref(),
        );
    }

    let default_path = Path::new("config/default.toml");
    if default_path.exists() {
        load_from(
            default_path,
            work_dir_override.as_deref(),
            log_level_override.as_deref(),
        )
    } else {
        // Hardcoded minimal default
        let work_dir_str = work_dir_override.unwrap_or("~/.araliya".to_string());
        let work_dir = expand_home(&work_dir_str);
        let log_level = log_level_override.unwrap_or("info".to_string());

        Ok(Config {
            bot_name: "araliya".to_string(),
            work_dir,
            identity_dir: None,
            log_level,
            comms: CommsConfig {
                pty: PtyConfig { enabled: true },
                telegram: TelegramConfig { enabled: false },
                http: HttpConfig { enabled: false, bind: "127.0.0.1:8080".to_string() },
                axum_channel: AxumChannelConfig { enabled: false, bind: "127.0.0.1:8080".to_string() },
            },
            agents: AgentsConfig {
                default_agent: "basic_chat".to_string(),
                channel_map: HashMap::new(),
                enabled: HashSet::from(["basic_chat".to_string()]),
                agent_memory: HashMap::new(),
                news_query: None,
            },
            llm: LlmConfig {
                provider: "dummy".to_string(),
                openai: OpenAiConfig {
                    api_base_url: "https://api.openai.com/v1/chat/completions".to_string(),
                    model: "gpt-4o-mini".to_string(),
                    temperature: 0.2,
                    timeout_seconds: 60,
                    input_per_million_usd: 0.0,
                    output_per_million_usd: 0.0,
                    cached_input_per_million_usd: 0.0,
                },
            },
            llm_api_key: env::var("LLM_API_KEY").ok(),
            ui: UiConfig {
                svui: SvuiConfig { enabled: false, static_dir: None },
            },
            tools: ToolsConfig {
                newsmail_aggregator: NewsmailAggregatorConfig {
                    label_ids: default_newsmail_label_ids(),
                    n_last: default_newsmail_n_last(),
                    tsec_last: None,
                    q: None,
                },
            },
            memory_kv_cap: Some(200),
            memory_transcript_cap: Some(500),
        })
    }
}

/// Internal loader — accepts an explicit path and optional overrides.
/// Tests pass overrides directly instead of mutating env vars.
/// Follows `[meta] base = "..."` inheritance chains before resolving.
pub fn load_from(
    path: &Path,
    work_dir_override: Option<&str>,
    log_level_override: Option<&str>,
) -> Result<Config, AppError> {
    let merged_val = load_raw_merged(path, &mut HashSet::new())?;

    let parsed: RawConfig = serde::Deserialize::deserialize(merged_val)
        .map_err(|e: toml::de::Error| AppError::Config(format!("config error in {}: {e}", path.display())))?;

    let s = parsed.supervisor;

    let work_dir_str = work_dir_override.unwrap_or(&s.work_dir).to_string();
    let work_dir = expand_home(&work_dir_str);
    let log_level = log_level_override.unwrap_or(&s.log_level).to_string();
    let identity_dir = s.identity_dir.map(|identity_dir| {
        let path = PathBuf::from(identity_dir);
        if path.is_absolute() {
            path
        } else {
            work_dir.join(path)
        }
    });

    let news_query = parsed.agents.entries
        .get("news")
        .and_then(|entry| entry.query.as_ref())
        .map(|q| NewsAgentQueryConfig {
            label: q.label.clone(),
            n_last: q.n_last,
            t_interval: q.t_interval.clone(),
            tsec_last: q.tsec_last,
            q: q.q.clone(),
        });

    Ok(Config {
        bot_name: s.bot_name,
        work_dir,
        identity_dir,
        log_level,
        comms: CommsConfig {
            pty: PtyConfig {
                enabled: parsed.comms.pty.enabled,
            },
            telegram: TelegramConfig {
                enabled: parsed.comms.telegram.enabled,
            },
            http: HttpConfig {
                enabled: parsed.comms.http.enabled,
                bind: parsed.comms.http.bind,
            },
            axum_channel: AxumChannelConfig {
                enabled: parsed.comms.axum_channel.enabled,
                bind: parsed.comms.axum_channel.bind,
            },
        },
        agents: AgentsConfig {
            default_agent: parsed.agents.default_agent,
            channel_map: parsed.agents.routing,
            enabled: parsed.agents.entries
                .iter()
                .filter(|(_, e)| e.enabled)
                .map(|(id, _)| id.clone())
                .collect(),
            agent_memory: parsed.agents.entries
                .into_iter()
                .filter(|(_, e)| !e.memory.is_empty())
                .map(|(id, e)| (id, e.memory))
                .collect(),
            news_query,
        },
        llm: LlmConfig {
            provider: parsed.llm.provider,
            openai: OpenAiConfig {
                api_base_url: parsed.llm.openai.api_base_url,
                model: parsed.llm.openai.model,
                temperature: parsed.llm.openai.temperature,
                timeout_seconds: parsed.llm.openai.timeout_seconds,
                input_per_million_usd: parsed.llm.openai.input_per_million_usd,
                output_per_million_usd: parsed.llm.openai.output_per_million_usd,
                cached_input_per_million_usd: parsed.llm.openai.cached_input_per_million_usd,
            },
        },
        llm_api_key: env::var("LLM_API_KEY").ok(),
        ui: UiConfig {
            svui: SvuiConfig {
                enabled: parsed.ui.svui.enabled,
                static_dir: parsed.ui.svui.static_dir,
            },
        },
        tools: ToolsConfig {
            newsmail_aggregator: NewsmailAggregatorConfig {
                label_ids: parsed.tools.newsmail_aggregator.label_ids,
                n_last: parsed.tools.newsmail_aggregator.n_last.max(1),
                tsec_last: parsed.tools.newsmail_aggregator.tsec_last,
                q: parsed.tools.newsmail_aggregator.q,
            },
        },
        memory_kv_cap: parsed.memory.basic_session.kv_cap,
        memory_transcript_cap: parsed.memory.basic_session.transcript_cap,
    })
}

/// Expand a leading `~` to the user's home directory.
/// Absolute or relative paths without `~` are returned unchanged.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

// ── test helpers ──────────────────────────────────────────────────────────────

/// Safe `Config` for unit tests — dummy LLM, no API keys, no external calls.
#[cfg(test)]
impl Config {
    pub fn test_default(work_dir: &Path) -> Self {
        Self {
            bot_name: "test".into(),
            work_dir: work_dir.to_path_buf(),
            identity_dir: None,
            log_level: "info".into(),
            comms: CommsConfig {
                pty: PtyConfig { enabled: true },
                telegram: TelegramConfig { enabled: false },
                http: HttpConfig {
                    enabled: false,
                    bind: default_http_bind(),
                },
                axum_channel: AxumChannelConfig {
                    enabled: false,
                    bind: default_http_bind(),
                },
            },
            agents: AgentsConfig {
                default_agent: "echo".into(),
                enabled: HashSet::from(["echo".to_string()]),
                channel_map: HashMap::new(),
                agent_memory: HashMap::new(),
                news_query: None,
            },
            llm: LlmConfig {
                provider: "dummy".into(),
                openai: OpenAiConfig {
                    api_base_url: "http://localhost:0/v1/chat/completions".into(),
                    model: "test-model".into(),
                    temperature: 0.0,
                    timeout_seconds: 1,
                    input_per_million_usd: 0.0,
                    output_per_million_usd: 0.0,
                    cached_input_per_million_usd: 0.0,
                },
            },
            llm_api_key: None,
            ui: UiConfig {
                svui: SvuiConfig {
                    enabled: false,
                    static_dir: None,
                },
            },
            tools: ToolsConfig {
                newsmail_aggregator: NewsmailAggregatorConfig {
                    label_ids: default_newsmail_label_ids(),
                    n_last: default_newsmail_n_last(),
                    tsec_last: None,
                    q: None,
                },
            },
            memory_kv_cap: None,
            memory_transcript_cap: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    const MINIMAL_TOML: &str = r#"
[supervisor]
bot_name = "test-bot"
work_dir = "~/.araliya"
log_level = "info"
"#;

    fn write_toml(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parse_basic_config() {
        let f = write_toml(MINIMAL_TOML);
        let cfg = load_from(f.path(), None, None).unwrap();
        assert_eq!(cfg.bot_name, "test-bot");
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = dirs::home_dir().expect("home dir must exist in test env");
        let expanded = expand_home("~/.araliya");
        assert!(expanded.starts_with(&home));
        assert!(expanded.ends_with(".araliya"));
    }

    #[test]
    fn absolute_path_unchanged() {
        let p = expand_home("/absolute/path");
        assert_eq!(p, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn relative_path_unchanged() {
        let p = expand_home("relative/path");
        assert_eq!(p, PathBuf::from("relative/path"));
    }

    #[test]
    fn missing_file_errors() {
        let result = load_from(Path::new("/nonexistent/config.toml"), None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("config error"));
    }

    #[test]
    fn env_work_dir_override() {
        let f = write_toml(MINIMAL_TOML);
        let cfg = load_from(f.path(), Some("/tmp/test-override"), None).unwrap();
        assert_eq!(cfg.work_dir, PathBuf::from("/tmp/test-override"));
    }

    #[test]
    fn env_log_level_override() {
        let f = write_toml(MINIMAL_TOML);
        let cfg = load_from(f.path(), None, Some("debug")).unwrap();
        assert_eq!(cfg.log_level, "debug");
    }

    // ── layered config tests ──────────────────────────────────────────────────

    const BASE_TOML: &str = r#"
[supervisor]
bot_name = "base-bot"
work_dir = "~/.araliya"
log_level = "info"

[agents]
default = "echo"

[llm]
default = "dummy"

[llm.openai]
model = "gpt-base"
temperature = 0.1
timeout_seconds = 30
api_base_url = "https://api.openai.com/v1/chat/completions"
"#;

    fn write_named(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn overlay_keeps_base_fields() {
        let dir = TempDir::new().unwrap();
        write_named(&dir, "base.toml", BASE_TOML);
        let overlay = r#"
[meta]
base = "base.toml"

[supervisor]
log_level = "debug"
"#;
        let overlay_path = write_named(&dir, "overlay.toml", overlay);
        let cfg = load_from(&overlay_path, None, None).unwrap();
        assert_eq!(cfg.bot_name, "base-bot");   // from base
        assert_eq!(cfg.log_level, "debug");      // from overlay
    }

    #[test]
    fn overlay_wins_scalar() {
        let dir = TempDir::new().unwrap();
        write_named(&dir, "base.toml", BASE_TOML);
        let overlay = r#"
[meta]
base = "base.toml"

[llm.openai]
model = "gpt-overlay"
"#;
        let overlay_path = write_named(&dir, "overlay.toml", overlay);
        let cfg = load_from(&overlay_path, None, None).unwrap();
        assert_eq!(cfg.llm.openai.model, "gpt-overlay");  // overlay wins
        assert_eq!(cfg.llm.openai.temperature, 0.1);       // base preserved
    }

    #[test]
    fn chained_bases() {
        let dir = TempDir::new().unwrap();
        write_named(&dir, "grandbase.toml", BASE_TOML);
        let middle = r#"
[meta]
base = "grandbase.toml"

[supervisor]
bot_name = "middle-bot"
"#;
        write_named(&dir, "middle.toml", middle);
        let top = r#"
[meta]
base = "middle.toml"

[supervisor]
log_level = "warn"
"#;
        let top_path = write_named(&dir, "top.toml", top);
        let cfg = load_from(&top_path, None, None).unwrap();
        assert_eq!(cfg.bot_name, "middle-bot"); // from middle (beats grandbase)
        assert_eq!(cfg.log_level, "warn");       // from top
    }

    #[test]
    fn missing_base_errors() {
        let dir = TempDir::new().unwrap();
        let overlay = r#"
[meta]
base = "nonexistent.toml"

[supervisor]
bot_name = "x"
work_dir = "~/.araliya"
log_level = "info"
"#;
        let overlay_path = write_named(&dir, "overlay.toml", overlay);
        let result = load_from(&overlay_path, None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cannot read") || msg.contains("config error"));
    }

    #[test]
    fn cycle_detection() {
        let dir = TempDir::new().unwrap();
        let self_path = dir.path().join("self.toml");
        let content = format!(
            "[meta]\nbase = \"{}\"\n\n{BASE_TOML}",
            self_path.display()
        );
        std::fs::write(&self_path, content).unwrap();
        let result = load_from(&self_path, None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("circular"));
    }

}

