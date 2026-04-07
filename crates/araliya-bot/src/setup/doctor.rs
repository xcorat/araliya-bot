//! `araliya-bot doctor` — config health check.
//!
//! Validates that all required files exist, keys are set, and config sections
//! are present. Prints ✓ / ✗ per check and exits non-zero if any fail.

use std::path::Path;

use console::style;

pub fn run(config_path: &Path, env_path: &Path) -> anyhow::Result<bool> {
    println!(
        "\n{} {}",
        style("──").cyan(),
        style("Araliya Bot — Doctor").bold()
    );
    println!();

    let mut all_ok = true;

    // ── Files ──────────────────────────────────────────────────────────
    all_ok &= check(
        "Config file exists",
        config_path.exists(),
        &format!(
            "Missing: {} — run 'araliya-bot setup'",
            config_path.display()
        ),
    );

    all_ok &= check(
        ".env file exists",
        env_path.exists(),
        &format!("Missing: {} — run 'araliya-bot setup'", env_path.display()),
    );

    // ── TOML structure ────────────────────────────────────────────────
    if config_path.exists() {
        match std::fs::read_to_string(config_path) {
            Ok(content) => {
                match toml::from_str::<toml::Value>(&content) {
                    Ok(parsed) => {
                        all_ok &= check("Config is valid TOML", true, "");
                        all_ok &= check(
                            "[supervisor] section present",
                            parsed.get("supervisor").is_some(),
                            "Config missing [supervisor] — regenerate with 'araliya-bot setup'",
                        );
                        all_ok &= check(
                            "[llm] section present",
                            parsed.get("llm").is_some(),
                            "Config missing [llm] — regenerate with 'araliya-bot setup'",
                        );
                        all_ok &= check(
                            "[agents] section present",
                            parsed.get("agents").is_some(),
                            "Config missing [agents] — regenerate with 'araliya-bot setup'",
                        );
                        all_ok &= check(
                            "[comms] section present",
                            parsed.get("comms").is_some(),
                            "Config missing [comms] — regenerate with 'araliya-bot setup'",
                        );

                        // Check that default agent is not the bare echo placeholder
                        if let Some(agents) = parsed.get("agents")
                            && let Some(_default) = agents.get("default").and_then(|v| v.as_str())
                        {
                            all_ok &= check(
                                "Default agent is configured",
                                true,
                                "Default agent is 'echo' — consider running 'araliya-bot setup' to choose a profile",
                            );
                        }
                    }
                    Err(e) => {
                        all_ok &=
                            check("Config is valid TOML", false, &format!("Parse error: {e}"));
                    }
                }
            }
            Err(e) => {
                all_ok &= check("Config is readable", false, &format!("Read error: {e}"));
            }
        }
    }

    // ── Environment variables ─────────────────────────────────────────
    let llm_key_set = std::env::var("OPENAI_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    // Also check the .env file directly in case it hasn't been sourced.
    let llm_key_in_env_file = if env_path.exists() {
        std::fs::read_to_string(env_path)
            .map(|c| {
                c.lines()
                    .any(|l| l.starts_with("OPENAI_API_KEY=") && l.len() > "OPENAI_API_KEY=".len())
            })
            .unwrap_or(false)
    } else {
        false
    };

    all_ok &= check(
        "OPENAI_API_KEY is set",
        llm_key_set || llm_key_in_env_file,
        "OPENAI_API_KEY not found in environment or .env file",
    );

    // Telegram: only check if enabled in config
    let tg_enabled_in_config = if config_path.exists() {
        std::fs::read_to_string(config_path)
            .ok()
            .and_then(|c| toml::from_str::<toml::Value>(&c).ok())
            .and_then(|v| v.get("comms")?.get("telegram")?.get("enabled")?.as_bool())
            .unwrap_or(false)
    } else {
        false
    };

    if tg_enabled_in_config {
        let tg_set = std::env::var("TELEGRAM_BOT_TOKEN")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let tg_in_file = env_path.exists()
            && std::fs::read_to_string(env_path)
                .map(|c| c.lines().any(|l| l.starts_with("TELEGRAM_BOT_TOKEN=")))
                .unwrap_or(false);
        all_ok &= check(
            "TELEGRAM_BOT_TOKEN is set (Telegram is enabled)",
            tg_set || tg_in_file,
            "TELEGRAM_BOT_TOKEN not found — add it to your .env file",
        );
    }

    // ── Summary ───────────────────────────────────────────────────────
    println!();
    if all_ok {
        println!("  {} All checks passed.", style("✓").green().bold());
    } else {
        println!(
            "  {} Some checks failed. Run {} to reconfigure.",
            style("✗").red().bold(),
            style("araliya-bot setup").bold()
        );
    }
    println!();

    Ok(all_ok)
}

fn check(label: &str, ok: bool, hint: &str) -> bool {
    if ok {
        println!("  {}  {}", style("✓").green(), label);
    } else {
        println!("  {}  {}", style("✗").red(), label);
        if !hint.is_empty() {
            println!("     {}", style(hint).yellow());
        }
    }
    ok
}
