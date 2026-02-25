//! Configuration loading with env-var overrides.
//!
//! Reads TOML files, supports `[meta] base = "..."` inheritance chains,
//! and applies `ARALIYA_WORK_DIR` and `ARALIYA_LOG_LEVEL` env overrides.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::AppError;

use super::raw::{self, RawConfig};
use super::types::*;

/// Deep-merge two TOML values.
/// Tables are merged recursively — the overlay only needs to specify keys that
/// differ from the base. For every other type (string, integer, array, …)
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
/// fully merged `toml::Value`. `visited` carries canonicalized paths already
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
        let work_dir_str = work_dir_override.unwrap_or_else(|| "~/.araliya".to_string());
        let work_dir = expand_home(&work_dir_str);
        let log_level = log_level_override.unwrap_or_else(|| "info".to_string());

        Ok(Config {
            bot_name: "araliya".to_string(),
            work_dir,
            identity_dir: None,
            log_level,
            comms: CommsConfig {
                pty: PtyConfig { enabled: true },
                telegram: TelegramConfig { enabled: false },
                http: HttpConfig {
                    enabled: false,
                    bind: "127.0.0.1:8080".to_string(),
                },
                axum_channel: AxumChannelConfig {
                    enabled: false,
                    bind: "127.0.0.1:8080".to_string(),
                },
            },
            agents: AgentsConfig {
                default_agent: "basic_chat".to_string(),
                channel_map: HashMap::new(),
                enabled: HashSet::from(["basic_chat".to_string()]),
                agent_memory: HashMap::new(),
                news_query: None,
                docs: None,
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
                qwen: QwenConfig {
                    api_base_url: "http://127.0.0.1:8081/v1/chat/completions".to_string(),
                    model: "qwen2.5-instruct".to_string(),
                    temperature: 0.2,
                    timeout_seconds: 60,
                    max_tokens: 8192,
                    input_per_million_usd: 0.0,
                    output_per_million_usd: 0.0,
                    cached_input_per_million_usd: 0.0,
                },
            },
            llm_api_key: env::var("LLM_API_KEY").ok(),
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

    let parsed: RawConfig = Deserialize::deserialize(merged_val)
        .map_err(|e: toml::de::Error| {
            AppError::Config(format!("config error in {}: {e}", path.display()))
        })?;

    let s = parsed.supervisor;

    let work_dir_str = work_dir_override.unwrap_or(&s.work_dir).to_string();
    let work_dir = expand_home(&work_dir_str);
    let log_level = log_level_override.unwrap_or(&s.log_level).to_string();
    let identity_dir = s.identity_dir.map(|identity_dir| {
        let p = PathBuf::from(identity_dir);
        if p.is_absolute() {
            p
        } else {
            work_dir.join(p)
        }
    });

    let news_query = parsed
        .agents
        .entries
        .get("news")
        .and_then(|entry| entry.query.as_ref())
        .map(|q| NewsAgentQueryConfig {
            label: q.label.clone(),
            n_last: q.n_last,
            t_interval: q.t_interval.clone(),
            tsec_last: q.tsec_last,
            q: q.q.clone(),
        });

    let docs_cfg = parsed.agents.entries.get("docs").map(|entry| {
        let defaults = DocsKgConfig::default();
        let kg = DocsKgConfig {
            min_entity_mentions: entry
                .kg
                .min_entity_mentions
                .unwrap_or(defaults.min_entity_mentions),
            bfs_max_depth: entry.kg.bfs_max_depth.unwrap_or(defaults.bfs_max_depth),
            edge_weight_threshold: entry
                .kg
                .edge_weight_threshold
                .unwrap_or(defaults.edge_weight_threshold),
            max_chunks: entry.kg.max_chunks.unwrap_or(defaults.max_chunks),
            fts_share: entry.kg.fts_share.unwrap_or(defaults.fts_share),
            max_seeds: entry.kg.max_seeds.unwrap_or(defaults.max_seeds),
        };
        DocsAgentConfig {
            docsdir: entry.docsdir.clone(),
            index: entry.index.clone(),
            use_kg: entry.use_kg,
            kg,
        }
    });
    let docs_cfg = docs_cfg.filter(|d| d.docsdir.is_some());

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
            enabled: parsed
                .agents
                .entries
                .iter()
                .filter(|(_, e)| e.enabled)
                .map(|(id, _)| id.clone())
                .collect(),
            agent_memory: parsed
                .agents
                .entries
                .into_iter()
                .filter(|(_, e)| !e.memory.is_empty())
                .map(|(id, e)| (id, e.memory))
                .collect(),
            news_query,
            docs: docs_cfg,
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
            qwen: QwenConfig {
                api_base_url: parsed.llm.qwen.api_base_url,
                model: parsed.llm.qwen.model,
                temperature: parsed.llm.qwen.temperature,
                timeout_seconds: parsed.llm.qwen.timeout_seconds,
                max_tokens: parsed.llm.qwen.max_tokens,
                input_per_million_usd: parsed.llm.qwen.input_per_million_usd,
                output_per_million_usd: parsed.llm.qwen.output_per_million_usd,
                cached_input_per_million_usd: parsed.llm.qwen.cached_input_per_million_usd,
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
