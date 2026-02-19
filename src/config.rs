//! Configuration loading with env-var overrides.
//!
//! Reads `config/default.toml` relative to the current working directory,
//! then applies `ARALIYA_WORK_DIR` and `ARALIYA_LOG_LEVEL` env overrides.

use std::{
    env,
    collections::HashMap,
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

/// Comms subsystem configuration.
#[derive(Debug, Clone)]
pub struct CommsConfig {
    pub pty: PtyConfig,
}

/// LLM provider configuration.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Named provider to use (e.g. "dummy", "openai").
    pub provider: String,
}

/// Agents subsystem configuration.
#[derive(Debug, Clone)]
pub struct AgentsConfig {
    /// Ordered list of enabled agent IDs. The first entry is the default agent.
    pub enabled: Vec<String>,
    /// Optional channel-to-agent overrides (channel_id -> agent_id).
    pub channel_map: HashMap<String, String>,
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
}

impl Config {
    /// Returns `true` if the PTY channel should be loaded.
    ///
    /// PTY is auto-enabled when no other comms channels are configured (always
    /// true for now while PTY is the only channel). Explicit `enabled = false`
    /// in config will still suppress it.
    pub fn comms_pty_should_load(&self) -> bool {
        self.comms.pty.enabled
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
}

#[derive(Deserialize)]
struct RawPty {
    /// Defaults to `true`: PTY auto-enables when no other channel is present.
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Deserialize)]
struct RawLlm {
    #[serde(default = "default_llm_provider")]
    provider: String,
}

impl Default for RawLlm {
    fn default() -> Self {
        Self { provider: default_llm_provider() }
    }
}

fn default_llm_provider() -> String {
    "dummy".to_string()
}

#[derive(Deserialize, Default)]
struct RawAgents {
    #[serde(default = "default_agents_enabled")]
    enabled: Vec<String>,
    #[serde(default)]
    channel_map: HashMap<String, String>,
}

impl Default for RawPty {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn default_true() -> bool {
    true
}

fn default_agents_enabled() -> Vec<String> {
    vec!["basic_chat".to_string()]
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
        comms: CommsConfig {
            pty: PtyConfig {
                enabled: parsed.comms.pty.enabled,
            },
        },
        agents: AgentsConfig {
            enabled: parsed.agents.enabled,
            channel_map: parsed.agents.channel_map,
        },
        llm: LlmConfig {
            provider: parsed.llm.provider,
        },
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
