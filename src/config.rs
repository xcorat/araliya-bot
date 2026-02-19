//! Configuration loading with env-var overrides.
//!
//! Reads `config/default.toml` relative to the current working directory,
//! then applies `ARALIYA_WORK_DIR` and `ARALIYA_LOG_LEVEL` env overrides.

use std::{
    env,
    path::{Path, PathBuf},
    fs,
};

use serde::Deserialize;

use crate::error::AppError;

/// Fully-resolved supervisor configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bot_name: String,
    /// Working directory for all persistent data (already expanded, no `~`).
    pub work_dir: PathBuf,
    pub log_level: String,
}

/// Raw TOML shape — `serde` target before resolution.
#[derive(Deserialize)]
struct RawConfig {
    supervisor: RawSupervisor,
}

#[derive(Deserialize)]
struct RawSupervisor {
    bot_name: String,
    work_dir: String,
    log_level: String,
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
    let log_level = log_level_override.unwrap_or(&s.log_level).to_string();

    Ok(Config {
        bot_name: s.bot_name,
        work_dir: expand_home(&work_dir_str),
        log_level,
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
