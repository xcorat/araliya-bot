//! Tests for agent prompt and manifest presence in config/agents/.
//!
//! Prompts are co-located with agent manifests under config/agents/{agent}/
//! and shared layers live in config/agents/_shared/.
//! The legacy config/prompts/ directory was removed in Phase 6 of modularization.

use std::fs;
use std::path::Path;

/// Workspace root: crates/araliya-bot/../../ → repo root.
fn agents_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/agents")
}

// ── Shared prompt layer tests ─────────────────────────────────────────────

#[test]
fn test_shared_prompts_exist_in_agents_dir() {
    let shared = agents_dir().join("_shared");
    assert!(shared.join("id.md").exists(), "_shared/id.md missing");
    assert!(shared.join("agent.md").exists(), "_shared/agent.md missing");
    assert!(
        shared.join("memory_and_tools.md").exists(),
        "_shared/memory_and_tools.md missing"
    );
    assert!(
        shared.join("subagent.md").exists(),
        "_shared/subagent.md missing"
    );
    assert!(shared.join("tools.ms").exists(), "_shared/tools.ms missing");
}

#[test]
fn test_memory_and_tools_prompt_exists() {
    assert!(
        agents_dir().join("_shared/memory_and_tools.md").exists(),
        "memory_and_tools.md prompt file missing"
    );
}

#[test]
fn test_subagent_prompt_exists() {
    assert!(
        agents_dir().join("_shared/subagent.md").exists(),
        "subagent.md prompt file missing"
    );
}

#[test]
fn test_memory_and_tools_template_vars() {
    let text = fs::read_to_string(agents_dir().join("_shared/memory_and_tools.md")).unwrap();
    assert!(
        text.contains("{{tools}}"),
        "memory_and_tools.md should contain {{{{tools}}}} variable"
    );
}

#[test]
fn test_subagent_template_vars() {
    let text = fs::read_to_string(agents_dir().join("_shared/subagent.md")).unwrap();
    assert!(
        text.contains("{{subagent_role}}"),
        "subagent.md should contain {{{{subagent_role}}}} variable"
    );
}

// ── Agent manifest tests ──────────────────────────────────────────────────

#[test]
fn test_agent_manifests_exist() {
    let ad = agents_dir();
    for agent in &[
        "echo",
        "basic-chat",
        "chat",
        "agentic-chat",
        "docs",
        "docs_agent",
        "uniweb",
        "gmail",
        "news",
        "gdelt_news",
        "newsroom",
        "news_aggregator",
        "test_rssnews",
        "runtime_cmd",
        "webbuilder",
    ] {
        assert!(
            ad.join(agent).join("agent.toml").exists(),
            "agent.toml missing for {}",
            agent
        );
    }
}

#[test]
fn test_agent_prompts_co_located() {
    let ad = agents_dir();
    // Agents with co-located prompts
    assert!(ad.join("agentic-chat").join("instruct.md").exists());
    assert!(ad.join("agentic-chat").join("context.md").exists());
    assert!(ad.join("docs").join("instruct.md").exists());
    assert!(ad.join("docs").join("context.md").exists());
    assert!(ad.join("chat").join("context.md").exists());
    assert!(ad.join("news").join("summary.md").exists());
    assert!(ad.join("gdelt_news").join("summary.md").exists());
    assert!(ad.join("newsroom").join("summary.md").exists());
}

#[test]
fn test_docs_agent_extends() {
    let manifest =
        fs::read_to_string(agents_dir().join("docs_agent").join("agent.toml")).unwrap();
    assert!(
        manifest.contains(r#"extends = "docs""#),
        "docs_agent should extend docs"
    );
}

#[test]
fn test_chat_context_prompt_exists() {
    assert!(
        agents_dir().join("chat/context.md").exists(),
        "chat/context.md missing"
    );
}

#[test]
fn test_news_summary_prompt_exists() {
    assert!(
        agents_dir().join("news/summary.md").exists(),
        "news/summary.md missing"
    );
}
