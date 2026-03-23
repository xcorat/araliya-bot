//! Interactive first-run setup wizard + doctor.
//!
//! Invoked via:
//!   araliya-bot setup  [--config PATH] [--env PATH] [--work-dir PATH]
//!   araliya-bot doctor [--config PATH] [--env PATH]

pub mod answers;
pub mod doctor;
pub mod writer;

use std::path::PathBuf;

use anyhow::{Context, Result};
use console::style;
use dialoguer::{Confirm, Input, MultiSelect, Password, Select};
use indicatif::{ProgressBar, ProgressStyle};

use answers::{Answers, BotProfile, LlmProvider};

// ── public API ─────────────────────────────────────────────────────────────

pub struct SetupOpts {
    pub config_path: PathBuf,
    pub env_path: PathBuf,
    pub work_dir: PathBuf,
}

pub struct DoctorOpts {
    pub config_path: PathBuf,
    pub env_path: PathBuf,
}

/// Run the interactive setup wizard.
pub fn run_setup(opts: SetupOpts) -> Result<()> {
    banner();
    println!(
        "  {}",
        style("Press Ctrl-C at any time to cancel without saving.").dim()
    );
    println!();

    // ── Step 1: Bot identity ──────────────────────────────────────────
    section("Bot identity");

    let bot_name: String = Input::new()
        .with_prompt("Bot name")
        .default("araliya".into())
        .interact_text()
        .context("bot name prompt")?;

    let work_dir_default = opts.work_dir.display().to_string();
    let work_dir_str: String = Input::new()
        .with_prompt("Runtime data directory (identity, sessions, memory)")
        .default(work_dir_default)
        .interact_text()
        .context("work dir prompt")?;
    let work_dir = PathBuf::from(&work_dir_str);

    println!();

    // ── Step 2: LLM provider ──────────────────────────────────────────
    section("LLM provider");
    println!(
        "  {}",
        style("API keys are stored in .env — never in config.toml.").dim()
    );
    println!();

    struct ProviderDef {
        label: &'static str,
        hint: &'static str,
        base_url: &'static str,
        default_model: &'static str,
        needs_key: bool,
        variant: LlmProvider,
    }

    let providers = [
        ProviderDef {
            label: "OpenAI",
            hint: "api.openai.com — GPT-4o, o3, etc.",
            base_url: "https://api.openai.com/v1/chat/completions",
            default_model: "gpt-4o-mini",
            needs_key: true,
            variant: LlmProvider::OpenAI,
        },
        ProviderDef {
            label: "OpenRouter",
            hint: "openrouter.ai — 100+ models via one key",
            base_url: "https://openrouter.ai/api/v1/chat/completions",
            default_model: "openai/gpt-4o-mini",
            needs_key: true,
            variant: LlmProvider::OpenRouter,
        },
        ProviderDef {
            label: "Anthropic",
            hint: "api.anthropic.com — Claude models direct",
            base_url: "https://api.anthropic.com/v1/messages",
            default_model: "claude-opus-4-5",
            needs_key: true,
            variant: LlmProvider::Anthropic,
        },
        ProviderDef {
            label: "Local / Ollama / LM Studio",
            hint: "no key required",
            base_url: "http://127.0.0.1:11434/v1/chat/completions",
            default_model: "llama3",
            needs_key: false,
            variant: LlmProvider::LocalOllama,
        },
        ProviderDef {
            label: "Other OpenAI-compatible",
            hint: "custom base URL",
            base_url: "",
            default_model: "",
            needs_key: true,
            variant: LlmProvider::OtherOpenAICompat,
        },
        ProviderDef {
            label: "Dummy (no LLM — for testing)",
            hint: "echoes input back",
            base_url: "",
            default_model: "dummy",
            needs_key: false,
            variant: LlmProvider::Dummy,
        },
    ];

    let provider_labels: Vec<String> = providers
        .iter()
        .map(|p| format!("{:<32} — {}", p.label, style(p.hint).dim()))
        .collect();

    let provider_idx = Select::new()
        .with_prompt("Which LLM provider?")
        .items(&provider_labels)
        .default(0)
        .interact()
        .context("provider select")?;

    let prov = &providers[provider_idx];

    let llm_api_key = if prov.needs_key {
        Password::new()
            .with_prompt(format!("{} API key", prov.label))
            .interact()
            .context("api key prompt")?
    } else {
        String::new()
    };

    let llm_model: String = if prov.default_model.is_empty() {
        Input::new()
            .with_prompt("Model name")
            .interact_text()
            .context("model prompt")?
    } else {
        Input::new()
            .with_prompt("Model name")
            .default(prov.default_model.into())
            .interact_text()
            .context("model prompt")?
    };

    let llm_api_base_url: String = if prov.base_url.is_empty() {
        Input::new()
            .with_prompt("API base URL")
            .interact_text()
            .context("base url prompt")?
    } else {
        Input::new()
            .with_prompt("API base URL")
            .default(prov.base_url.into())
            .interact_text()
            .context("base url prompt")?
    };

    println!();

    // ── Step 3: Agent profile ─────────────────────────────────────────
    section("Agent profile");
    println!(
        "  {}",
        style("This controls which agent handles incoming messages.").dim()
    );
    println!();

    struct ProfileDef {
        label: &'static str,
        hint: &'static str,
        variant: BotProfile,
    }

    let profiles = [
        ProfileDef {
            label: "Basic chat",
            hint: "direct LLM pass-through, no memory",
            variant: BotProfile::BasicChat,
        },
        ProfileDef {
            label: "Session chat",
            hint: "multi-turn with conversation history",
            variant: BotProfile::SessionChat,
        },
        ProfileDef {
            label: "Agentic chat",
            hint: "dual-pass: instruction + tool use + response",
            variant: BotProfile::AgenticChat,
        },
        ProfileDef {
            label: "Docs agent",
            hint: "RAG over a local docs/ directory",
            variant: BotProfile::Docs,
        },
        ProfileDef {
            label: "Newsroom",
            hint: "persistent GDELT news monitoring",
            variant: BotProfile::Newsroom,
        },
        ProfileDef {
            label: "Custom (skip — configure manually)",
            hint: "echo agent as placeholder",
            variant: BotProfile::Custom,
        },
    ];

    let profile_labels: Vec<String> = profiles
        .iter()
        .map(|p| format!("{:<28} — {}", p.label, style(p.hint).dim()))
        .collect();

    let profile_idx = Select::new()
        .with_prompt("Which agent profile?")
        .items(&profile_labels)
        .default(0)
        .interact()
        .context("profile select")?;

    let profile = profiles[profile_idx].variant.clone();
    println!();

    // ── Step 4: Channels ──────────────────────────────────────────────
    section("Communication channels");
    println!(
        "  {}",
        style("PTY = interactive terminal  |  HTTP = web UI + REST API  |  Telegram = bot").dim()
    );
    println!();

    let channel_items = [
        "HTTP / Web UI  (serve frontend + API at localhost:8080)",
        "Telegram bot   (requires bot token from @BotFather)",
    ];
    let channel_defaults = [true, false];

    let selected_channels = MultiSelect::new()
        .with_prompt("Enable channels  (Space to toggle, Enter to confirm)")
        .items(&channel_items)
        .defaults(&channel_defaults)
        .interact()
        .context("channel select")?;

    let enable_http = selected_channels.contains(&0);
    let enable_telegram = selected_channels.contains(&1);

    let http_bind: String = if enable_http {
        Input::new()
            .with_prompt("HTTP bind address")
            .default("127.0.0.1:8080".into())
            .interact_text()
            .context("http bind prompt")?
    } else {
        "127.0.0.1:8080".into()
    };

    let telegram_token = if enable_telegram {
        let token: String = Password::new()
            .with_prompt("Telegram bot token (from @BotFather)")
            .interact()
            .context("telegram token prompt")?;

        // ── Validate token against Telegram getMe API ─────────────────
        println!();
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        spinner.set_message("Validating Telegram token...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let validation_result = validate_telegram_token(&token);

        spinner.finish_and_clear();

        match validation_result {
            Ok(bot_username) => {
                println!(
                    "  {}  Telegram token valid — bot: {}",
                    style("✓").green(),
                    style(format!("@{bot_username}")).bold()
                );
                Some(token)
            }
            Err(e) => {
                println!(
                    "  {}  Token validation failed: {}",
                    style("⚠").yellow(),
                    e
                );
                let save_anyway = Confirm::new()
                    .with_prompt("Save the token anyway?")
                    .default(false)
                    .interact()
                    .context("save token confirm")?;
                if save_anyway {
                    Some(token)
                } else {
                    println!(
                        "  {}",
                        style("Telegram skipped — add TELEGRAM_BOT_TOKEN to .env manually.").dim()
                    );
                    None
                }
            }
        }
    } else {
        None
    };

    let enable_telegram_final = telegram_token.is_some();
    println!();

    // ── Write files ───────────────────────────────────────────────────
    section("Writing configuration");

    let answers = Answers {
        bot_name: bot_name.clone(),
        work_dir,
        config_dir: opts.config_path.parent().unwrap_or(&opts.config_path).to_path_buf(),
        llm_provider: prov.variant.clone(),
        llm_api_key,
        llm_model: llm_model.clone(),
        llm_api_base_url: llm_api_base_url.clone(),
        profile,
        enable_http,
        http_bind: http_bind.clone(),
        enable_telegram: enable_telegram_final,
        telegram_token,
    };

    writer::write_config(&answers, &opts.config_path)
        .with_context(|| format!("writing config to {}", opts.config_path.display()))?;
    println!(
        "  {}  Config → {}",
        style("✓").green(),
        style(opts.config_path.display()).bold()
    );

    writer::write_env(&answers, &opts.env_path)
        .with_context(|| format!("writing .env to {}", opts.env_path.display()))?;
    println!(
        "  {}  Secrets → {}",
        style("✓").green(),
        style(opts.env_path.display()).bold()
    );

    // ── Next steps ────────────────────────────────────────────────────
    println!();
    println!("{}", style("─".repeat(50)).cyan().dim());
    println!("{}", style("  You're all set!").green().bold());
    println!("{}", style("─".repeat(50)).cyan().dim());
    println!();
    println!("  Start (interactive terminal):");
    println!(
        "    {}",
        style(format!(
            "araliya-bot -i -f {}",
            opts.config_path.display()
        ))
        .bold()
    );
    println!();
    if enable_http {
        println!("  Start (headless — serves web UI + API):");
        println!(
            "    {}",
            style(format!("araliya-bot -f {}", opts.config_path.display())).bold()
        );
        println!();
        println!("  Web UI:");
        println!("    {}", style(format!("http://{http_bind}")).bold());
        println!();
    }
    println!("  Validate config:");
    println!(
        "    {}",
        style(format!(
            "araliya-bot doctor -f {}",
            opts.config_path.display()
        ))
        .bold()
    );
    println!();
    println!("  Edit config:");
    println!(
        "    {}",
        style(opts.config_path.display().to_string()).dim()
    );
    println!();

    Ok(())
}

/// Run the doctor health check. Returns Ok(true) if all checks pass.
pub fn run_doctor(opts: DoctorOpts) -> Result<bool> {
    doctor::run(&opts.config_path, &opts.env_path)
}

// ── private helpers ────────────────────────────────────────────────────────

fn banner() {
    println!();
    println!(
        "{}",
        style("╔══════════════════════════════════════════════╗")
            .cyan()
            .bold()
    );
    println!(
        "{}",
        style("║       Araliya Bot  ·  Setup Wizard           ║")
            .cyan()
            .bold()
    );
    println!(
        "{}",
        style("╚══════════════════════════════════════════════╝")
            .cyan()
            .bold()
    );
    println!();
}

fn section(title: &str) {
    println!(
        "{} {}",
        style("──").cyan(),
        style(title).bold()
    );
    println!();
}

/// Call the Telegram getMe API and return the bot username on success.
fn validate_telegram_token(token: &str) -> Result<String> {
    let url = format!("https://api.telegram.org/bot{token}/getMe");
    let resp = reqwest::blocking::get(&url)
        .context("HTTP request to Telegram API failed")?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().context("invalid JSON from Telegram")?;

    if !status.is_success() || !body["ok"].as_bool().unwrap_or(false) {
        let desc = body["description"]
            .as_str()
            .unwrap_or("unknown error");
        anyhow::bail!("{desc}");
    }

    let username = body["result"]["username"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    Ok(username)
}
