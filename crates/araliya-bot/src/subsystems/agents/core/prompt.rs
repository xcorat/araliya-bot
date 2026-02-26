//! Layered prompt builder for agent plugins.
//!
//! Prompts for agents are assembled from a stack of plain-text template
//! fragments stored under `config/prompts/`.  Each layer is appended in
//! order; missing files are silently skipped so layers can be optional.
//!
//! ## Layer ordering convention
//!
//! ```text
//! 0. id.md               — bot identity / persona (who it is)
//! 1. agent.md            — agent-level instructions (what it does)
//! 2. memory_and_tools.md — memory access & tool guidance; {{tools}} placeholder
//! 3. subagent.md         — (optional) subagent delegation constraints
//! 4. <agent body>        — agent-specific template with task variables
//! ```
//!
//! Variable substitution uses `{{key}}` syntax and is applied once at
//! [`build()`](PromptBuilder::build) time, after all layers are joined.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const SEPARATOR: &str = "\n\n";

/// Fluent builder that assembles a layered prompt from template files.
///
/// ```rust
/// use std::collections::HashMap;
/// // (in production code, call inside an agent handler)
/// // let prompt = PromptBuilder::new("config/prompts")
/// //     .layer("id.md")
/// //     .layer("agent.md")
/// //     .layer("memory_and_tools.md")
/// //     .with_tools(&["newsmail_aggregator".to_string()])
/// //     .append("Summarize: {{items}}")
/// //     .with_vars([("items", "item 1\nitem 2")])
/// //     .build();
/// ```
pub struct PromptBuilder {
    prompts_dir: PathBuf,
    parts: Vec<String>,
    vars: HashMap<String, String>,
}

impl PromptBuilder {
    /// Create a builder rooted at `prompts_dir` (e.g. `"config/prompts"`).
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

    /// Load `tools.ms` and substitute `{{tools}}` with a comma-separated
    /// list of tool names.  Falls back to an inline sentence if the file
    /// is missing.
    pub fn with_tools(mut self, tools: &[String]) -> Self {
        let tools_str = if tools.is_empty() {
            "none".to_string()
        } else {
            tools.join(", ")
        };

        let path = self.prompts_dir.join("tools.ms");
        let text = fs::read_to_string(&path).unwrap_or_else(|_| {
            "You have access to the following tools: {{tools}}".to_string()
        });
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

/// Convenience: build the standard identity + agent + memory/tools preamble.
///
/// This is the common prefix shared by every agent.  Call `.append()` and
/// `.with_vars()` on `PromptBuilder::preamble()` to add the agent-specific
/// body before calling `.build()`.
pub fn preamble(prompts_dir: impl AsRef<Path>, tools: &[String]) -> PromptBuilder {
    PromptBuilder::new(prompts_dir.as_ref())
        .layer("id.md")
        .layer("agent.md")
        .layer("memory_and_tools.md")
        .with_tools(tools)
}

/// Convenience: build the standard preamble and append a subagent layer.
pub fn subagent_preamble(prompts_dir: impl AsRef<Path>, tools: &[String]) -> PromptBuilder {
    preamble(prompts_dir, tools).layer("subagent.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompts_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../config/prompts")
    }

    #[test]
    fn builder_assembles_layers_in_order() {
        let result = PromptBuilder::new(prompts_dir())
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
        let result = PromptBuilder::new(prompts_dir())
            .with_tools(&tools)
            .build();
        assert!(result.contains("newsmail_aggregator"));
        assert!(result.contains("gmail"));
        assert!(!result.contains("{{tools}}"));
    }

    #[test]
    fn builder_with_empty_tools_renders_none() {
        let result = PromptBuilder::new(prompts_dir())
            .with_tools(&[])
            .build();
        assert!(result.contains("none"));
    }

    #[test]
    fn preamble_contains_standard_layers() {
        let tools = vec!["some_tool".to_string()];
        let result = preamble(prompts_dir(), &tools).build();
        assert!(result.contains("some_tool"));
    }

    #[test]
    fn subagent_preamble_contains_subagent_layer() {
        let result = subagent_preamble(prompts_dir(), &[])
            .var("subagent_role", "fetch and summarise news")
            .build();
        assert!(result.contains("fetch and summarise news"));
    }
}
