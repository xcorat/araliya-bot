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

/// Comms subsystem configuration.
#[derive(Debug, Clone)]
pub struct CommsConfig {
    pub pty: PtyConfig,
    pub telegram: TelegramConfig,
    pub http: HttpConfig,
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
    /// Force supervisor stdio management adapter active in interactive TTY runs.
    pub stdio_management_interactive: bool,
    pub comms: CommsConfig,
    pub agents: AgentsConfig,
    pub llm: LlmConfig,
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
}

#[derive(Deserialize)]
struct RawSupervisor {
    bot_name: String,
    work_dir: String,
    #[serde(default)]
    identity_dir: Option<String>,
    log_level: String,
    #[serde(default = "default_false")]
    stdio_management_interactive: bool,
}

#[derive(Deserialize, Default)]
struct RawComms {
    #[serde(default)]
    pty: RawPty,
    #[serde(default)]
    telegram: RawTelegram,
    #[serde(default)]
    http: RawHttp,
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
}

impl Default for RawOpenAiConfig {
    fn default() -> Self {
        Self {
            api_base_url: default_openai_api_base_url(),
            model: default_openai_model(),
            temperature: default_openai_temperature(),
            timeout_seconds: default_openai_timeout_seconds(),
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

fn default_http_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

/// Load config from `config/default.toml`, then apply env-var overrides.
pub fn load() -> Result<Config, AppError> {
    let work_dir_override = env::var("ARALIYA_WORK_DIR").ok();
    let log_level_override = env::var("ARALIYA_LOG_LEVEL").ok();
    load_from(
        Path::new("config/default.toml"),
        work_dir_override.as_deref(),
        log_level_override.as_deref(),
    )
}

/// Internal loader — accepts an explicit path and optional overrides.
/// Tests pass overrides directly instead of mutating env vars.
pub fn load_from(
    path: &Path,
    work_dir_override: Option<&str>,
    log_level_override: Option<&str>,
) -> Result<Config, AppError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| AppError::Config(format!("cannot read {}: {e}", path.display())))?;

    let parsed: RawConfig = toml::from_str(&raw)
        .map_err(|e| AppError::Config(format!("parse error in {}: {e}", path.display())))?;

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

    Ok(Config {
        bot_name: s.bot_name,
        work_dir,
        identity_dir,
        log_level,
        stdio_management_interactive: s.stdio_management_interactive,
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
        },
        llm: LlmConfig {
            provider: parsed.llm.provider,
            openai: OpenAiConfig {
                api_base_url: parsed.llm.openai.api_base_url,
                model: parsed.llm.openai.model,
                temperature: parsed.llm.openai.temperature,
                timeout_seconds: parsed.llm.openai.timeout_seconds,
            },
        },
        llm_api_key: env::var("LLM_API_KEY").ok(),
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
            stdio_management_interactive: false,
            comms: CommsConfig {
                pty: PtyConfig { enabled: true },
                telegram: TelegramConfig { enabled: false },
                http: HttpConfig {
                    enabled: false,
                    bind: default_http_bind(),
                },
            },
            agents: AgentsConfig {
                default_agent: "echo".into(),
                enabled: HashSet::from(["echo".to_string()]),
                channel_map: HashMap::new(),
                agent_memory: HashMap::new(),
            },
            llm: LlmConfig {
                provider: "dummy".into(),
                openai: OpenAiConfig {
                    api_base_url: "http://localhost:0/v1/chat/completions".into(),
                    model: "test-model".into(),
                    temperature: 0.0,
                    timeout_seconds: 1,
                },
            },
            llm_api_key: None,
            memory_kv_cap: None,
            memory_transcript_cap: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
}
