//! Tests for agent prompt loading from config/prompts

use std::fs;

#[test]
fn test_news_prompt_file_exists() {
    let path = "config/prompts/news_summary.txt";
    assert!(fs::metadata(path).is_ok(), "news_summary.txt prompt file missing");
}

#[test]
fn test_docs_prompt_file_exists() {
    let path = "config/prompts/docs_qa.txt";
    assert!(fs::metadata(path).is_ok(), "docs_qa.txt prompt file missing");
}

#[test]
fn test_chat_prompt_file_exists() {
    let path = "config/prompts/chat_context.txt";
    assert!(fs::metadata(path).is_ok(), "chat_context.txt prompt file missing");
}

#[test]
fn test_news_prompt_template_vars() {
    let text = fs::read_to_string("config/prompts/news_summary.txt").unwrap();
    assert!(text.contains("{{items}}"), "news_summary.txt should contain {{items}} variable");
}

#[test]
fn test_docs_prompt_template_vars() {
    let text = fs::read_to_string("config/prompts/docs_qa.txt").unwrap();
    assert!(text.contains("{{docs}}"), "docs_qa.txt should contain {{docs}} variable");
    assert!(text.contains("{{question}}"), "docs_qa.txt should contain {{question}} variable");
}

#[test]
fn test_chat_prompt_template_vars() {
    let text = fs::read_to_string("config/prompts/chat_context.txt").unwrap();
    assert!(text.contains("{{history}}"), "chat_context.txt should contain {{history}} variable");
    assert!(text.contains("{{user_input}}"), "chat_context.txt should contain {{user_input}} variable");
}
