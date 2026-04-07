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
fn load_raw_merged(path: &Path, visited: &mut HashSet<PathBuf>) -> Result<toml::Value, AppError> {
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
                agent_skills: HashMap::new(),
                agent_aggregation_targets: HashMap::new(),
                news_query: None,
                gdelt_query: None,
                newsroom_query: None,
                agent_docs: HashMap::new(),
                agentic_chat: None,
                runtime_cmd: None,
                webbuilder: None,
                homebuilder: None,
                debug_logging: false,
                uniweb_session_id: None,
                uniweb_use_instruction_llm: false,
            },
            llm: LlmConfig {
                default: "dummy".to_string(),
                providers: HashMap::new(),
                instruction: None,
                routes: HashMap::new(),
            },
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
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
            runtimes: RuntimesConfig {
                enabled: true,
                default_timeout_secs: 30,
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

    let parsed: RawConfig =
        Deserialize::deserialize(merged_val).map_err(|e: toml::de::Error| {
            AppError::Config(format!("config error in {}: {e}", path.display()))
        })?;

    let s = parsed.supervisor;

    let work_dir_str = work_dir_override.unwrap_or(&s.work_dir).to_string();
    let work_dir = expand_home(&work_dir_str);
    let log_level = log_level_override.unwrap_or(&s.log_level).to_string();
    let identity_dir = s.identity_dir.map(|identity_dir| {
        let p = PathBuf::from(identity_dir);
        if p.is_absolute() { p } else { work_dir.join(p) }
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

    let gdelt_query = parsed
        .agents
        .entries
        .get("gdelt_news")
        .and_then(|entry| entry.gdelt_query.as_ref())
        .map(|q| GdeltAgentQueryConfig {
            lookback_minutes: q.lookback_minutes,
            limit: q.limit,
            min_articles: q.min_articles,
            min_importance: q.min_importance,
            sort_by_importance: q.sort_by_importance,
            english_only: q.english_only,
        });

    let newsroom_query = parsed
        .agents
        .entries
        .get("newsroom")
        .and_then(|entry| entry.gdelt_query.as_ref())
        .map(|q| GdeltAgentQueryConfig {
            lookback_minutes: q.lookback_minutes,
            limit: q.limit,
            min_articles: q.min_articles,
            min_importance: q.min_importance,
            sort_by_importance: q.sort_by_importance,
            english_only: q.english_only,
        });

    let agent_docs: HashMap<String, DocsAgentConfig> = parsed
        .agents
        .entries
        .iter()
        .filter(|(_, e)| e.docsdir.is_some())
        .map(|(id, entry)| {
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
            (
                id.clone(),
                DocsAgentConfig {
                    docsdir: entry.docsdir.clone(),
                    index: entry.index.clone(),
                    use_kg: entry.use_kg,
                    kg,
                },
            )
        })
        .collect();

    let agentic_chat_cfg =
        parsed
            .agents
            .entries
            .get("agentic-chat")
            .map(|entry| AgenticChatConfig {
                use_instruction_llm: entry.use_instruction_llm,
            });

    let runtime_cmd_cfg = parsed.agents.entries.get("runtime_cmd").map(|entry| {
        let defaults = RuntimeCmdAgentConfig::default();
        RuntimeCmdAgentConfig {
            runtime: entry.runtime.clone().unwrap_or(defaults.runtime),
            command: entry.command.clone().unwrap_or(defaults.command),
            setup_script: entry.setup_script.clone(),
        }
    });

    let webbuilder_cfg = parsed.agents.entries.get("webbuilder").map(|entry| {
        let defaults = WebBuilderAgentConfig::default();
        WebBuilderAgentConfig {
            max_iterations: entry.max_iterations.unwrap_or(defaults.max_iterations),
            scaffold: entry.scaffold.clone().unwrap_or(defaults.scaffold),
            theme_guides_dir: entry.theme_guides_dir.as_ref().map(PathBuf::from),
        }
    });

    let homebuilder_cfg = parsed.agents.entries.get("homebuilder").map(|entry| {
        let defaults = HomebuildAgentConfig::default();
        HomebuildAgentConfig {
            max_iterations: entry.max_iterations.unwrap_or(defaults.max_iterations),
            user_name: entry.user_name.clone().unwrap_or(defaults.user_name),
            notes_dir: entry
                .notes_dir
                .as_ref()
                .map(|s| PathBuf::from(s))
                .or(defaults.notes_dir),
            theme_guides_dir: entry.theme_guides_dir.as_ref().map(PathBuf::from),
        }
    });

    // Instruction LLM: just a reference to another provider name in the same map.
    let instruction_llm = parsed.llm.instruction.clone();

    let providers: HashMap<String, ProviderConfig> = parsed
        .llm
        .providers
        .into_iter()
        .map(|(name, raw)| {
            let api_type = match raw.api_type.as_str() {
                "chat_completions" | "openai_compatible" => ApiType::ChatCompletions,
                "openai_responses" | "responses" => ApiType::OpenAiResponses,
                "dummy" => ApiType::Dummy,
                // Catch-all: unknown api_type strings default to ChatCompletions.
                // This lets users add new OpenAI-compatible providers via config
                // without recompiling. Known wire protocols above stay strongly typed.
                other => {
                    tracing::warn!(
                        api_type = other,
                        provider = %name,
                        "unknown api_type, defaulting to chat_completions"
                    );
                    ApiType::ChatCompletions
                }
            };
            let api_base_url = raw.api_base_url.unwrap_or_else(|| match api_type {
                ApiType::ChatCompletions => {
                    "https://api.openai.com/v1/chat/completions".to_string()
                }
                ApiType::OpenAiResponses => "https://api.openai.com/v1/responses".to_string(),
                ApiType::Dummy => String::new(),
            });
            Ok((
                name,
                ProviderConfig {
                    api_type,
                    api_base_url,
                    model: raw.model,
                    temperature: raw.temperature,
                    api_key: resolve_api_key(raw.api_key, raw.api_key_file),
                    reasoning_effort: raw.reasoning_effort,
                    timeout_seconds: raw.timeout_seconds,
                    max_tokens: raw.max_tokens,
                    input_per_million_usd: raw.input_per_million_usd,
                    output_per_million_usd: raw.output_per_million_usd,
                    cached_input_per_million_usd: raw.cached_input_per_million_usd,
                },
            ))
        })
        .collect::<Result<HashMap<String, ProviderConfig>, AppError>>()?;

    // Symbolic route hints → (provider, optional model) pairs.
    let routes: HashMap<String, RouteConfig> = parsed
        .llm
        .routes
        .into_iter()
        .map(|(hint, raw)| {
            (
                hint,
                RouteConfig {
                    provider: raw.provider,
                    model: raw.model,
                },
            )
        })
        .collect();

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
                .iter()
                .filter(|(_, e)| !e.memory.is_empty())
                .map(|(id, e)| (id.clone(), e.memory.clone()))
                .collect(),
            agent_skills: parsed
                .agents
                .entries
                .iter()
                .filter(|(_, e)| !e.skills.is_empty())
                .map(|(id, e)| (id.clone(), e.skills.clone()))
                .collect(),
            agent_aggregation_targets: parsed
                .agents
                .entries
                .iter()
                .filter_map(|(id, e)| e.target_agent.as_ref().map(|t| (id.clone(), t.clone())))
                .collect(),
            news_query,
            gdelt_query,
            newsroom_query,
            agent_docs,
            agentic_chat: agentic_chat_cfg,
            runtime_cmd: runtime_cmd_cfg,
            webbuilder: webbuilder_cfg,
            homebuilder: homebuilder_cfg,
            debug_logging: parsed.agents.debug_logging,
            uniweb_session_id: parsed
                .agents
                .entries
                .get("uniweb")
                .and_then(|e| e.session_id.clone()),
            uniweb_use_instruction_llm: parsed
                .agents
                .entries
                .get("uniweb")
                .map(|e| e.use_instruction_llm)
                .unwrap_or(false),
        },
        llm: LlmConfig {
            default: parsed.llm.provider,
            providers,
            instruction: instruction_llm,
            routes,
        },
        openai_api_key: env::var("OPENAI_API_KEY").ok(),
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
        runtimes: RuntimesConfig {
            enabled: parsed.runtimes.enabled,
            default_timeout_secs: parsed.runtimes.default_timeout_secs,
        },
        memory_kv_cap: parsed.memory.basic_session.kv_cap,
        memory_transcript_cap: parsed.memory.basic_session.transcript_cap,
    })
}

/// Expand a leading `~` to the user's home directory.
/// Absolute or relative paths without `~` are returned unchanged.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    if path == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(path)
}

/// Resolves an API key from either a direct string, a `secret:<name>` prefix,
/// or a dedicated `api_key_file` path.
/// Secrets are looked up in `~/.local/share/araliya/secrets/` (or platform equivalent).
pub fn resolve_api_key(api_key: Option<String>, api_key_file: Option<String>) -> Option<String> {
    if let Some(file_path) = api_key_file {
        let path = expand_home(&file_path);
        match fs::read_to_string(&path) {
            Ok(content) => return Some(content.trim().to_string()),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to read api_key_file");
                return None;
            }
        }
    }

    if let Some(key) = api_key {
        if let Some(secret_name) = key.strip_prefix("secret:") {
            let data_dir = dirs::data_local_dir().unwrap_or_else(|| {
                dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(".local/share")
            });
            let secret_path = data_dir.join("araliya/secrets").join(secret_name);
            match fs::read_to_string(&secret_path) {
                Ok(content) => return Some(content.trim().to_string()),
                Err(e) => {
                    tracing::warn!(secret = %secret_name, path = %secret_path.display(), error = %e, "failed to read secret file");
                }
            }
        } else if key.starts_with("${") && key.ends_with('}') {
            let env_var = &key[2..key.len()-1];
            if let Ok(val) = env::var(env_var) {
                return Some(val);
            } else {
                tracing::warn!(env_var = %env_var, "env var for api_key not found");
            }
        } else {
            return Some(key);
        }
    }

    None
}
