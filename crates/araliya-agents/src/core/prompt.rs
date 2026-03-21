//! Layered prompt builder for agent plugins.
//!
//! Prompts are assembled from template files in two locations:
//!
//! 1. **Agent definition directory** (`config/agents/<agent-id>/`) — agent-specific
//!    prompts co-located with the agent's `agent.toml` manifest.
//! 2. **Shared layers** (`config/agents/_shared/`) — identity and common prompts
//!    shared across all agents.
//!
//! ## Layer ordering convention
//!
//! ```text
//! 0. _shared/id.md               — bot identity / persona (who it is)
//! 1. _shared/agent.md            — agent-level instructions (what it does)
//! 2. _shared/memory_and_tools.md — memory access & tool guidance; {{tools}} placeholder
//! 3. _shared/subagent.md         — (optional) subagent delegation constraints
//! 4. <agent-id>/instruct.md      — agent-specific instruction template
//! 5. <agent-id>/context.md       — agent-specific context/response template
//! ```
//!
//! Variable substitution uses `{{key}}` syntax and is applied once at
//! [`build()`](PromptBuilder::build) time, after all layers are joined.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const SEPARATOR: &str = "\n\n";

/// Fluent builder that assembles a layered prompt from template files.
pub struct PromptBuilder {
    prompts_dir: PathBuf,
    parts: Vec<String>,
    vars: HashMap<String, String>,
}

impl PromptBuilder {
    /// Create a builder rooted at `prompts_dir`.
    ///
    /// For shared layers this is `config/agents/_shared/`.
    /// For agent-specific layers, use [`agent_layer()`](Self::agent_layer).
    pub fn new(prompts_dir: impl Into<PathBuf>) -> Self {
        Self {
            prompts_dir: prompts_dir.into(),
            parts: Vec::new(),
            vars: HashMap::new(),
        }
    }

    /// Append a layer by loading `filename` from the prompts directory.
    /// Silently skips the layer when the file does not exist.
    pub fn layer(mut self, filename: &str) -> Self {
        let path = self.prompts_dir.join(filename);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    self.parts.push(trimmed);
                }
            }
            Err(_) => {
                tracing::debug!("prompt: layer '{}' not found — skipped", path.display());
            }
        }
        self
    }

    /// Load a prompt from an agent's definition directory.
    ///
    /// Resolution order:
    /// 1. `{user_agents_dir}/{agent_id}/{filename}` — user override (if provided)
    /// 2. `{agents_dir}/{agent_id}/{filename}` — system agent-specific prompt
    /// 3. `{agents_dir}/_shared/{filename}` — shared fallback
    ///
    /// Silently skips if no location has the file.
    pub fn agent_layer(self, agents_dir: &Path, agent_id: &str, filename: &str) -> Self {
        self.agent_layer_with_user(agents_dir, agent_id, filename, None)
    }

    /// Like [`agent_layer`](Self::agent_layer) but checks an optional user
    /// agents directory first for overrides.
    pub fn agent_layer_with_user(
        mut self,
        agents_dir: &Path,
        agent_id: &str,
        filename: &str,
        user_agents_dir: Option<&Path>,
    ) -> Self {
        // 1. Check user override
        if let Some(uad) = user_agents_dir {
            let user_path = uad.join(agent_id).join(filename);
            if let Ok(text) = fs::read_to_string(&user_path) {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    self.parts.push(trimmed);
                }
                return self;
            }
        }
        // 2. Check system agent dir
        let agent_path = agents_dir.join(agent_id).join(filename);
        if let Ok(text) = fs::read_to_string(&agent_path) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                self.parts.push(trimmed);
            }
            return self;
        }
        // 3. Fall back to _shared/
        let shared_path = agents_dir.join("_shared").join(filename);
        match fs::read_to_string(&shared_path) {
            Ok(text) => {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    self.parts.push(trimmed);
                }
            }
            Err(_) => {
                tracing::debug!(
                    "prompt: agent_layer '{}/{}' not found in user, agent, or _shared — skipped",
                    agent_id,
                    filename
                );
            }
        }
        self
    }

    /// Load `tools.ms` and substitute `{{tools}}` with a comma-separated
    /// list of tool names.  Falls back to an inline sentence if the file
    /// is missing.
    ///
    /// Also registers `"tools"` as a variable so `{{tools}}` in any other
    /// layer (e.g. `memory_and_tools.md`) is substituted at build time.
    pub fn with_tools(mut self, tools: &[String]) -> Self {
        let tools_str = if tools.is_empty() {
            "none".to_string()
        } else {
            tools.join(", ")
        };

        // Register as a variable so {{tools}} in other layers is substituted too.
        self.vars.insert("tools".to_string(), tools_str.clone());

        let path = self.prompts_dir.join("tools.ms");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|_| "You have access to the following tools: {{tools}}".to_string());
        let rendered = text.trim().replace("{{tools}}", &tools_str);
        if !rendered.is_empty() {
            self.parts.push(rendered);
        }
        self
    }

    /// Directly append a text fragment (e.g. an already-loaded template body).
    pub fn append(mut self, text: impl Into<String>) -> Self {
        let s = text.into();
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            self.parts.push(trimmed);
        }
        self
    }

    /// Register `{{key}}` → `value` substitution pairs applied at build time.
    pub fn with_vars<'a, I>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        for (k, v) in vars {
            self.vars.insert(k.to_string(), v.to_string());
        }
        self
    }

    /// Register a single variable.  Convenience wrapper around [`with_vars`](Self::with_vars).
    pub fn var(mut self, key: &str, value: impl Into<String>) -> Self {
        self.vars.insert(key.to_string(), value.into());
        self
    }

    /// Assemble all layers, join with blank lines, and apply variable substitution.
    pub fn build(self) -> String {
        let mut prompt = self.parts.join(SEPARATOR);
        for (k, v) in &self.vars {
            let placeholder = format!("{{{{{}}}}}", k);
            prompt = prompt.replace(&placeholder, v);
        }
        prompt
    }
}

/// Convenience: build the standard identity + agent + memory/tools preamble
/// from the `_shared/` directory inside `agents_dir`.
pub fn preamble(agents_dir: impl AsRef<Path>, tools: &[String]) -> PromptBuilder {
    let shared = agents_dir.as_ref().join("_shared");
    PromptBuilder::new(&shared)
        .layer("id.md")
        .layer("agent.md")
        .layer("memory_and_tools.md")
        .with_tools(tools)
}

/// Convenience: build the standard preamble and append a subagent layer.
pub fn subagent_preamble(agents_dir: impl AsRef<Path>, tools: &[String]) -> PromptBuilder {
    preamble(agents_dir, tools).layer("subagent.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/agents")
    }

    // Backward compat — tests also work with old prompts_dir
    fn prompts_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/prompts")
    }

    #[test]
    fn builder_assembles_layers_in_order() {
        let shared = agents_dir().join("_shared");
        let result = PromptBuilder::new(&shared)
            .layer("id.md")
            .layer("agent.md")
            .build();
        let id_pos = result.find("evolve").or_else(|| result.find('\u{2764}'));
        let agent_pos = result.find("intelligent agent");
        if let (Some(i), Some(a)) = (id_pos, agent_pos) {
            assert!(i < a, "id.md content should appear before agent.md content");
        }
        assert!(!result.trim().is_empty());
    }

    #[test]
    fn builder_skips_missing_file() {
        let result = PromptBuilder::new(prompts_dir())
            .layer("nonexistent_file_xyz.md")
            .append("hello")
            .build();
        assert_eq!(result.trim(), "hello");
    }

    #[test]
    fn builder_substitutes_variable() {
        let result = PromptBuilder::new(prompts_dir())
            .append("Items: {{items}}")
            .var("items", "item1\nitem2")
            .build();
        assert!(result.contains("item1\nitem2"));
        assert!(!result.contains("{{items}}"));
    }

    #[test]
    fn builder_with_tools_rendered() {
        let tools = vec!["newsmail_aggregator".to_string(), "gmail".to_string()];
        let shared = agents_dir().join("_shared");
        let result = PromptBuilder::new(&shared).with_tools(&tools).build();
        assert!(result.contains("newsmail_aggregator"));
        assert!(result.contains("gmail"));
        assert!(!result.contains("{{tools}}"));
    }

    #[test]
    fn builder_with_empty_tools_renders_none() {
        let shared = agents_dir().join("_shared");
        let result = PromptBuilder::new(&shared).with_tools(&[]).build();
        assert!(result.contains("none"));
    }

    #[test]
    fn preamble_contains_standard_layers() {
        let tools = vec!["some_tool".to_string()];
        let result = preamble(agents_dir(), &tools).build();
        assert!(result.contains("some_tool"));
    }

    #[test]
    fn subagent_preamble_contains_subagent_layer() {
        let result = subagent_preamble(agents_dir(), &[])
            .var("subagent_role", "fetch and summarise news")
            .build();
        assert!(result.contains("fetch and summarise news"));
    }

    #[test]
    fn agent_layer_loads_from_agent_dir() {
        let ad = agents_dir();
        let result = PromptBuilder::new(ad.join("_shared"))
            .agent_layer(&ad, "docs", "instruct.md")
            .build();
        // docs/instruct.md should exist and have content
        assert!(!result.trim().is_empty());
    }

    #[test]
    fn agent_layer_falls_back_to_shared() {
        let ad = agents_dir();
        // echo has no instruct.md — should fall back to _shared (which also
        // doesn't have instruct.md, so it should be skipped silently)
        let result = PromptBuilder::new(ad.join("_shared"))
            .agent_layer(&ad, "echo", "id.md")
            .build();
        // id.md exists in _shared, so echo gets it via fallback
        assert!(!result.trim().is_empty());
    }

    #[test]
    fn agent_layer_skips_missing() {
        let ad = agents_dir();
        let result = PromptBuilder::new(ad.join("_shared"))
            .agent_layer(&ad, "echo", "nonexistent_xyz.md")
            .append("fallback")
            .build();
        assert_eq!(result.trim(), "fallback");
    }
}
