//! Tests for agent prompt loading from config/agents and config/prompts (legacy).

use std::fs;
use std::path::Path;

/// Workspace root is two levels up from the crate manifest directory
/// (`crates/araliya-bot/../../` → workspace root).
fn prompts_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/prompts")
}

fn agents_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/agents")
}

fn prompt_path(name: &str) -> std::path::PathBuf {
    prompts_dir().join(name)
}

#[test]
fn test_news_prompt_file_exists() {
    assert!(
        prompt_path("news_summary.txt").exists(),
        "news_summary.txt prompt file missing"
    );
}

#[test]
fn test_docs_prompt_file_exists() {
    assert!(
        prompt_path("docs_qa.txt").exists(),
        "docs_qa.txt prompt file missing"
    );
}

#[test]
fn test_chat_prompt_file_exists() {
    assert!(
        prompt_path("chat_context.txt").exists(),
        "chat_context.txt prompt file missing"
    );
}

#[test]
fn test_news_prompt_template_vars() {
    let text = fs::read_to_string(prompt_path("news_summary.txt")).unwrap();
    assert!(
        text.contains("{{items}}"),
        "news_summary.txt should contain {{items}} variable"
    );
}

#[test]
fn test_docs_prompt_template_vars() {
    let text = fs::read_to_string(prompt_path("docs_qa.txt")).unwrap();
    assert!(
        text.contains("{{docs}}"),
        "docs_qa.txt should contain {{docs}} variable"
    );
    assert!(
        text.contains("{{question}}"),
        "docs_qa.txt should contain {{question}} variable"
    );
}

#[test]
fn test_chat_prompt_template_vars() {
    let text = fs::read_to_string(prompt_path("chat_context.txt")).unwrap();
    assert!(
        text.contains("{{history}}"),
        "chat_context.txt should contain {{history}} variable"
    );
    assert!(
        text.contains("{{user_input}}"),
        "chat_context.txt should contain {{user_input}} variable"
    );
}

#[test]
fn test_memory_and_tools_prompt_exists() {
    assert!(
        prompt_path("memory_and_tools.md").exists(),
        "memory_and_tools.md prompt file missing"
    );
}

#[test]
fn test_subagent_prompt_exists() {
    assert!(
        prompt_path("subagent.md").exists(),
        "subagent.md prompt file missing"
    );
}

#[test]
fn test_memory_and_tools_template_vars() {
    let text = fs::read_to_string(prompt_path("memory_and_tools.md")).unwrap();
    assert!(
        text.contains("{{tools}}"),
        "memory_and_tools.md should contain {{tools}} variable"
    );
}

#[test]
fn test_subagent_template_vars() {
    let text = fs::read_to_string(prompt_path("subagent.md")).unwrap();
    assert!(
        text.contains("{{subagent_role}}"),
        "subagent.md should contain {{subagent_role}} variable"
    );
}

// ── Agent definition directory tests ─────────────────────────────────────

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
    let manifest = fs::read_to_string(agents_dir().join("docs_agent").join("agent.toml")).unwrap();
    assert!(
        manifest.contains(r#"extends = "docs""#),
        "docs_agent should extend docs"
    );
}
