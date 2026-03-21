//! Agent definition — parsed from `agent.toml` manifests.
//!
//! Each agent has a definition directory containing an `agent.toml` manifest
//! and optional prompt files. System agents live in `config/agents/`, user
//! agents in `{work_dir}/agents/`. User definitions override system ones
//! by agent ID.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Parsed agent definition from an `agent.toml` manifest.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    /// Agent identifier (derived from directory name).
    pub id: String,
    /// Optional parent agent to inherit from.
    pub extends: Option<String>,
    /// Whether the agent is enabled.
    pub enabled: bool,
    /// Bus tools this agent may invoke.
    pub skills: Vec<String>,
    /// Memory store types required by this agent.
    pub memory_stores: Vec<String>,
    /// Whether to route instruction pass through `llm/instruct`.
    pub use_instruction_llm: bool,
    /// Catch-all for agent-specific config sections (docs, query, etc.).
    pub extra: toml::Value,
    /// Resolved directory containing this definition's files.
    pub dir: PathBuf,
}

impl AgentDefinition {
    /// Load an agent definition from a directory containing `agent.toml`.
    pub fn load(dir: &Path) -> Result<Self, AppError> {
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::Config("agent definition directory has no name".into()))?
            .to_string();

        let manifest_path = dir.join("agent.toml");
        if !manifest_path.exists() {
            return Err(AppError::Config(format!(
                "agent '{}': missing agent.toml in {}",
                id,
                dir.display()
            )));
        }

        let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
            AppError::Config(format!(
                "agent '{}': cannot read {}: {}",
                id,
                manifest_path.display(),
                e
            ))
        })?;

        let table: toml::Value = content.parse().map_err(|e: toml::de::Error| {
            AppError::Config(format!("agent '{}': invalid TOML: {}", id, e))
        })?;

        let agent_section = table.get("agent");

        let extends = agent_section
            .and_then(|a| a.get("extends"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let enabled = agent_section
            .and_then(|a| a.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let skills = table
            .get("tools")
            .and_then(|t| t.get("skills"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let memory_stores = table
            .get("memory")
            .and_then(|m| m.get("stores"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let use_instruction_llm = table
            .get("llm")
            .and_then(|l| l.get("use_instruction_llm"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Self {
            id,
            extends,
            enabled,
            skills,
            memory_stores,
            use_instruction_llm,
            extra: table,
            dir: dir.to_path_buf(),
        })
    }

    /// Resolve a prompt file path within this agent's definition directory.
    /// Returns `Some(path)` if the file exists, `None` otherwise.
    pub fn prompt_path(&self, filename: &str) -> Option<PathBuf> {
        let path = self.dir.join(filename);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }
}

/// Scan an agents directory and load all agent definitions found.
///
/// Each subdirectory (except `_shared`) containing an `agent.toml` is loaded.
/// Returns a map of agent ID → definition.
pub fn scan_agent_definitions(dir: &Path) -> HashMap<String, AgentDefinition> {
    let mut defs = HashMap::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return defs,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Skip _shared — that's for shared prompt layers, not agents.
        if name.starts_with('_') {
            continue;
        }
        // Only load directories that have agent.toml.
        if !path.join("agent.toml").exists() {
            continue;
        }
        match AgentDefinition::load(&path) {
            Ok(def) => {
                defs.insert(def.id.clone(), def);
            }
            Err(e) => {
                tracing::warn!("skipping agent definition at {}: {}", path.display(), e);
            }
        }
    }

    defs
}

/// Resolve agent definitions from system and user directories.
///
/// System agents (`config/agents/`) are loaded first, then user agents
/// (`{work_dir}/agents/`) overlay by ID — user definitions win on conflict.
pub fn resolve_agent_definitions(
    system_dir: &Path,
    user_dir: &Path,
) -> HashMap<String, AgentDefinition> {
    let mut defs = scan_agent_definitions(system_dir);
    let user_defs = scan_agent_definitions(user_dir);

    for (id, def) in user_defs {
        if defs.contains_key(&id) {
            tracing::info!("user agent '{}' overrides system definition", id);
        } else {
            tracing::info!("user agent '{}' loaded from {}", id, def.dir.display());
        }
        defs.insert(id, def);
    }

    defs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, agent_id: &str, content: &str) -> PathBuf {
        let agent_dir = dir.join(agent_id);
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("agent.toml"), content).unwrap();
        agent_dir
    }

    #[test]
    fn load_minimal_definition() {
        let tmp = TempDir::new().unwrap();
        let agent_dir = write_manifest(
            tmp.path(),
            "echo",
            r#"
[agent]
enabled = true
"#,
        );
        let def = AgentDefinition::load(&agent_dir).unwrap();
        assert_eq!(def.id, "echo");
        assert!(def.enabled);
        assert!(def.extends.is_none());
        assert!(def.skills.is_empty());
    }

    #[test]
    fn load_full_definition() {
        let tmp = TempDir::new().unwrap();
        let agent_dir = write_manifest(
            tmp.path(),
            "docs",
            r#"
[agent]
extends = ""
enabled = true

[tools]
skills = ["web_search", "gmail"]

[memory]
stores = ["basic_session", "docstore"]

[llm]
use_instruction_llm = false

[docs]
docsdir = "/path/to/docs"
index = "index.md"
"#,
        );
        let def = AgentDefinition::load(&agent_dir).unwrap();
        assert_eq!(def.id, "docs");
        assert!(def.enabled);
        assert!(def.extends.is_none()); // empty string → None
        assert_eq!(def.skills, vec!["web_search", "gmail"]);
        assert_eq!(def.memory_stores, vec!["basic_session", "docstore"]);
        assert!(!def.use_instruction_llm);
        // Extra sections preserved
        assert!(def.extra.get("docs").is_some());
    }

    #[test]
    fn load_extends_definition() {
        let tmp = TempDir::new().unwrap();
        let agent_dir = write_manifest(
            tmp.path(),
            "docs_agent",
            r#"
[agent]
extends = "docs"
"#,
        );
        let def = AgentDefinition::load(&agent_dir).unwrap();
        assert_eq!(def.id, "docs_agent");
        assert_eq!(def.extends.as_deref(), Some("docs"));
    }

    #[test]
    fn scan_skips_shared_and_nondir() {
        let tmp = TempDir::new().unwrap();
        // _shared should be skipped
        let shared = tmp.path().join("_shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("agent.toml"), "[agent]\nenabled = true").unwrap();

        // Regular file should be skipped
        std::fs::write(tmp.path().join("not-a-dir.toml"), "").unwrap();

        // Valid agent
        write_manifest(tmp.path(), "echo", "[agent]\nenabled = true");

        let defs = scan_agent_definitions(tmp.path());
        assert_eq!(defs.len(), 1);
        assert!(defs.contains_key("echo"));
    }

    #[test]
    fn user_overrides_system() {
        let sys = TempDir::new().unwrap();
        let usr = TempDir::new().unwrap();

        write_manifest(sys.path(), "echo", "[agent]\nenabled = true");
        write_manifest(sys.path(), "chat", "[agent]\nenabled = true");
        write_manifest(usr.path(), "echo", "[agent]\nenabled = false");
        write_manifest(usr.path(), "custom", "[agent]\nenabled = true");

        let defs = resolve_agent_definitions(sys.path(), usr.path());
        assert_eq!(defs.len(), 3); // echo (overridden), chat, custom
        assert!(!defs["echo"].enabled); // user override wins
        assert!(defs["chat"].enabled);
        assert!(defs["custom"].enabled);
    }

    #[test]
    fn prompt_path_resolution() {
        let tmp = TempDir::new().unwrap();
        let agent_dir = write_manifest(tmp.path(), "docs", "[agent]\nenabled = true");
        std::fs::write(agent_dir.join("instruct.md"), "test prompt").unwrap();

        let def = AgentDefinition::load(&agent_dir).unwrap();
        assert!(def.prompt_path("instruct.md").is_some());
        assert!(def.prompt_path("nonexistent.md").is_none());
    }

    #[test]
    fn missing_manifest_errors() {
        let tmp = TempDir::new().unwrap();
        let empty_dir = tmp.path().join("no-manifest");
        std::fs::create_dir_all(&empty_dir).unwrap();

        let result = AgentDefinition::load(&empty_dir);
        assert!(result.is_err());
    }
}
