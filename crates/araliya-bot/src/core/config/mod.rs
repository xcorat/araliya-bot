//! Configuration loading with env-var overrides.
//!
//! Reads `config/default.toml` relative to the current working directory,
//! then applies `ARALIYA_WORK_DIR` and `ARALIYA_LOG_LEVEL` env overrides.
//!
//! # Module layout
//!
//! - **types** — Public configuration structs consumed by subsystems
//!   (`Config`, `LlmConfig`, `AgentsConfig`, etc.).
//! - **raw** — Raw TOML deserialization types (`RawConfig`, `RawLlm`, …).
//!   These mirror the file shape and use serde defaults; kept private.
//! - **load** — Loading logic: `merge_toml`, `load_raw_merged`, `load`,
//!   `load_from`, `expand_home`.

mod load;
mod raw;
mod types;

pub use load::{expand_home, load, load_from};
pub use types::*;

#[cfg(test)]
impl Config {
    /// Safe `Config` for unit tests — dummy LLM, no API keys, no external calls.
    pub fn test_default(work_dir: &std::path::Path) -> Self {
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
                    bind: raw::default_http_bind(),
                },
                axum_channel: AxumChannelConfig {
                    enabled: false,
                    bind: raw::default_http_bind(),
                },
            },
            agents: AgentsConfig {
                default_agent: "echo".into(),
                enabled: std::collections::HashSet::from(["echo".to_string()]),
                channel_map: std::collections::HashMap::new(),
                agent_memory: std::collections::HashMap::new(),
                news_query: None,
                docs: None,
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
                qwen: QwenConfig {
                    api_base_url: "http://127.0.0.1:8081/v1/chat/completions".into(),
                    model: "qwen2.5-instruct".into(),
                    temperature: 0.2,
                    timeout_seconds: 60,
                    max_tokens: 8192,
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
                    label_ids: raw::default_newsmail_label_ids(),
                    n_last: raw::default_newsmail_n_last(),
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
    fn parse_docs_agent_path() {
        let toml = r#"
[supervisor]
bot_name = "foo"
work_dir = "/tmp"
log_level = "info"

[agents.docs]
enabled = true
docsdir = "docs/"
index = "index.md"
"#;
        let f = write_toml(toml);
        let cfg = load_from(f.path(), None, None).unwrap();
        let docs = cfg.agents.docs.as_ref().unwrap();
        assert_eq!(docs.docsdir.as_deref(), Some("docs/"));
        assert_eq!(docs.index.as_deref(), Some("index.md"));
    }

    #[test]
    fn parse_docs_agent_config() {
        let toml = r#"
[supervisor]
bot_name = "test"
work_dir = "~/work"
log_level = "debug"

[agents]
default = "docs"

[agents.docs]
enabled = true
docsdir = "docs/guide/"
index = "index.md"
"#;
        let f = write_toml(toml);
        let cfg = load_from(f.path(), None, None).unwrap();
        assert_eq!(cfg.agents.default_agent, "docs");
        let docs = cfg.agents.docs.as_ref().unwrap();
        assert_eq!(docs.docsdir.as_deref(), Some("docs/guide/"));
        assert_eq!(docs.index.as_deref(), Some("index.md"));
    }

    #[test]
    fn absolute_path_unchanged() {
        let p = expand_home("/absolute/path");
        assert_eq!(p, std::path::PathBuf::from("/absolute/path"));
    }

    #[test]
    fn relative_path_unchanged() {
        let p = expand_home("relative/path");
        assert_eq!(p, std::path::PathBuf::from("relative/path"));
    }

    #[test]
    fn missing_file_errors() {
        let result = load_from(std::path::Path::new("/nonexistent/config.toml"), None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("config error"));
    }

    #[test]
    fn env_work_dir_override() {
        let f = write_toml(MINIMAL_TOML);
        let cfg = load_from(f.path(), Some("/tmp/test-override"), None).unwrap();
        assert_eq!(cfg.work_dir, std::path::PathBuf::from("/tmp/test-override"));
    }

    #[test]
    fn env_log_level_override() {
        let f = write_toml(MINIMAL_TOML);
        let cfg = load_from(f.path(), None, Some("debug")).unwrap();
        assert_eq!(cfg.log_level, "debug");
    }

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

    fn write_named(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
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
        assert_eq!(cfg.bot_name, "base-bot");
        assert_eq!(cfg.log_level, "debug");
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
        assert_eq!(cfg.llm.openai.model, "gpt-overlay");
        assert_eq!(cfg.llm.openai.temperature, 0.1);
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
        assert_eq!(cfg.bot_name, "middle-bot");
        assert_eq!(cfg.log_level, "warn");
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
