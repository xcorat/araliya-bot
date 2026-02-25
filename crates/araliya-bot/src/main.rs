// TODO: move the core functionality to a `core` crate/folder
//! Araliya Bot — supervisor entry point.
//!
//! Startup sequence:
//!   1. Load .env (if present)
//!   2. Load config
//!   3. Resolve effective log level (CLI `-v` flags > env > config)
//!   4. Init logger once
//!   5. Setup bot identity
//!   6. Start supervisor bus
//!   7. Spawn Ctrl-C → shutdown signal watcher
//!   8. Spawn supervisor run-loop
//!   9. Run comms subsystem (drives console until shutdown)
//!  10. Cancel token + join supervisor

mod core;
mod bootstrap;
mod llm;
mod supervisor;
mod subsystems;

pub use core::{config, error};
pub use bootstrap::{identity, logger};

use tokio_util::sync::CancellationToken;
use tracing::info;

use std::sync::{Arc, OnceLock};

use supervisor::bus::SupervisorBus;
use supervisor::component_info::ComponentInfo;
use supervisor::control::SupervisorControl;
use supervisor::dispatch::BusHandler;
// CHECK: again! sub-agents should imply sub-memory, why do we need to have both?
#[cfg(feature = "subsystem-agents")]
use subsystems::agents::AgentsSubsystem;

#[cfg(feature = "subsystem-llm")]
use subsystems::llm::LlmSubsystem;

#[cfg(feature = "subsystem-tools")]
use subsystems::tools::ToolsSubsystem;

#[cfg(feature = "subsystem-cron")]
use subsystems::cron::CronSubsystem;

#[cfg(feature = "subsystem-memory")]
use subsystems::memory::{MemoryConfig, MemorySystem};

use subsystems::management::ManagementSubsystem;
use subsystems::management::ManagementInfo;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), error::AppError> {
    // Load .env if present — ignore errors (file is optional).
    let _ = dotenvy::dotenv();

    let args = parse_cli_args();

    let mut config = config::load(args.config_path.as_deref())?;

    // Without -i, no stdio channels are active (daemon-safe default).
    // TODO: warn in this case that pty is available only when interactive.
    if !args.interactive {
        config.comms.pty.enabled = false;
    }

    let effective_log_level = args.log_level.unwrap_or(config.log_level.as_str());
    let force_cli_level = args.log_level.is_some();

    logger::init(effective_log_level, force_cli_level)?;

    info!(
        bot_name = %config.bot_name,
        work_dir = %config.work_dir.display(),
        configured_log_level = %config.log_level,
        effective_log_level = %effective_log_level,
        interactive = %args.interactive,
        "config loaded"
    );

    let identity = identity::setup(&config)?;

    info!(public_id = %identity.public_id, "identity ready — starting subsystems");

    // Shared shutdown token — Ctrl-C cancels it, all tasks watch it.
    // Created before the memory system so the docstore manager can receive it.
    let shutdown = CancellationToken::new();

    // Optionally build the memory system.
    #[cfg(feature = "subsystem-memory")]
    let memory = {
        let mem_config = MemoryConfig {
            kv_cap: config.memory_kv_cap,
            transcript_cap: config.memory_transcript_cap,
        };
        let mut mem = MemorySystem::new(&identity.identity_dir, mem_config)
            .map_err(|e| error::AppError::Memory(e.to_string()))?;
        // CHECK: We are only starting the docstore manager, so this might be ok.
        #[cfg(feature = "idocstore")]
        mem.start_docstore_manager(shutdown.clone());
        std::sync::Arc::new(mem)
    };

    // Build the supervisor bus (buffer = 64 messages).
    // TODO: Add config, and a refuse collector.
    let bus = SupervisorBus::new(64);
    // Build the supervisor-internal control plane (buffer = 32 messages).
    let control = SupervisorControl::new(32);

    // Clone the handle before moving bus into the supervisor task.
    // CHECK: why clone? does all systems that uses the bus need a clone?
    let bus_handle = bus.handle.clone();
    let control_handle = control.handle.clone();

    // Ctrl-C handler — cancels the token so all tasks shut down.
    let ctrlc_token = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("ctrl-c received — initiating shutdown");
            ctrlc_token.cancel();
        }
    });

    // Build subsystem handlers and register with supervisor.
    // TODO:
    let mut handlers: Vec<Box<dyn BusHandler>> = vec![];
    let mut configured_handlers: Vec<String> = vec!["management".to_string()];
    
    // OnceLock bridge: comms::start() will populate this once channel list is known.
    // ManagementSubsystem reads it when building the component tree.
    let comms_info: Arc<OnceLock<ComponentInfo>> = Arc::new(OnceLock::new());

    handlers.push(Box::new(ManagementSubsystem::new(
        control_handle.clone(),
        bus_handle.clone(),
        // TODO: IMPORTANT: we shouldn't add the llm provider here. let the subsystem handle it.
        ManagementInfo {
            bot_id: identity.public_id.clone(),
            llm_provider: config.llm.provider.clone(),
            llm_model: config.llm.openai.model.clone(),
            llm_timeout_seconds: config.llm.openai.timeout_seconds,
        },
        comms_info.clone(),
    )));

    #[cfg(feature = "subsystem-llm")]
    {
        let llm = LlmSubsystem::new(&config.llm, config.llm_api_key.clone())
            .map_err(|e| error::AppError::Config(e.to_string()))?;
        handlers.push(Box::new(llm));
        configured_handlers.push("llm".to_string());
    }

    #[cfg(feature = "subsystem-tools")]
    {
        handlers.push(Box::new(ToolsSubsystem::new(
            config.tools.newsmail_aggregator.clone(),
        )));
        configured_handlers.push("tools".to_string());
    }

    #[cfg(all(feature = "subsystem-agents", feature = "subsystem-memory"))]
    {
        let rates = crate::llm::ModelRates {
            input_per_million_usd: config.llm.openai.input_per_million_usd,
            output_per_million_usd: config.llm.openai.output_per_million_usd,
            cached_input_per_million_usd: config.llm.openai.cached_input_per_million_usd,
        };
        let agents = AgentsSubsystem::new(config.agents.clone(), bus_handle.clone(), memory.clone())?
            .with_llm_rates(rates);
        #[cfg(feature = "plugin-docs")]
        agents.init_docs().await?;
        handlers.push(Box::new(agents));
        configured_handlers.push("agents".to_string());
    }

    #[cfg(feature = "subsystem-cron")]
    {
        let cron = CronSubsystem::new(bus_handle.clone(), shutdown.clone());
        handlers.push(Box::new(cron));
        configured_handlers.push("cron".to_string());
    }

    // Spawn supervisor run-loop (owns the bus receiver).
    let sup_token = shutdown.clone();
    let sup_handle = tokio::spawn(async move {
        supervisor::run(bus, control, sup_token, handlers).await;
    });

    // Start supervisor-internal transport adapters for control/chat over stdio.
    // The management adapter is only active when the user passes -i / --interactive.
    // The Unix domain socket adapter always starts (daemon management).
    let socket_path = config.work_dir.join("araliya.sock");
    supervisor::adapters::start(
        control_handle,
        bus_handle.clone(),
        shutdown.clone(),
        args.interactive,
        socket_path,
    );

    print_startup_summary(
        &config,
        &identity,
        args.interactive,
        &configured_handlers,
    );

    // Start comms channels as independent concurrent tasks.
    #[cfg(feature = "subsystem-comms")]
    {
        // Build optional UI serve handle for the HTTP channel.
        #[cfg(feature = "subsystem-ui")]
        let ui_handle = subsystems::ui::start(&config);

        let comms = subsystems::comms::start(
            &config,
            bus_handle,
            shutdown.clone(),
            #[cfg(feature = "subsystem-ui")]
            ui_handle,
            comms_info,
        );
        comms.join().await?;
    }

    // CHECK: We had exited after this if there are no comms active, removed that. 

    // If comms exited due to EOF (not Ctrl-C), still signal everything to stop.
    shutdown.cancel();

    sup_handle.await.ok();

    // In interactive mode, print a clean exit line so the shell prompt
    // appears below the tracing output.  In daemon mode, exit silently.
    // CHECK: what the correct way to do this.
    if args.interactive {
        use std::io::Write as _;
        println!("\nBye :) ...");
        let _ = std::io::stdout().flush();
    }
    let _ = { use std::io::Write as _; std::io::stderr().flush() };

    Ok(())
}

// TODO: Move this to a separate file.
fn print_startup_summary(
    config: &config::Config,
    identity: &identity::Identity,
    interactive: bool,
    configured_handlers: &[String],
) {
    let ansi_enabled = {
        use std::io::IsTerminal as _;
        std::io::stdout().is_terminal()
    };

    let pid = std::process::id();
    let mode_text = if interactive { "interactive" } else { "daemon" };
    let stdio_status = if interactive { "enabled" } else { "disabled" };

    let style_default_agent = |agent: &str| {
        if ansi_enabled {
            format!("\x1b[1m{agent}\x1b[0m")
        } else {
            agent.to_string()
        }
    };

    let fit = |text: String| -> String {
        const WIDTH: usize = 58;
        let char_count = text.chars().count();
        if char_count >= WIDTH {
            let mut out = text.chars().take(WIDTH - 1).collect::<String>();
            out.push('…');
            out
        } else {
            format!("{text:<WIDTH$}")
        }
    };

    let mut subsystem_names = configured_handlers.to_vec();
    subsystem_names.push("identity".to_string());
    subsystem_names.sort();
    subsystem_names.dedup();
    let subsystem_summary = if subsystem_names.is_empty() {
        "none".to_string()
    } else {
        subsystem_names.join(", ")
    };

    #[cfg(feature = "subsystem-llm")]
    let llm_line = format!(
        "provider={} model={} temp={} timeout={}s",
        config.llm.provider,
        config.llm.openai.model,
        config.llm.openai.temperature,
        config.llm.openai.timeout_seconds
    );
    #[cfg(not(feature = "subsystem-llm"))]
    let llm_line = "disabled (not compiled)".to_string();

    let mut agent_lines: Vec<String> = Vec::new();
    #[cfg(all(feature = "subsystem-agents", feature = "subsystem-memory"))]
    {
        let mut enabled_agents = config.agents.enabled.iter().cloned().collect::<Vec<_>>();
        enabled_agents.sort();

        if enabled_agents.is_empty() {
            agent_lines.push("none enabled".to_string());
        } else {
            for agent in enabled_agents {
                let display_name = if agent == config.agents.default_agent {
                    format!("{} (default)", style_default_agent(&agent))
                } else {
                    agent.clone()
                };

                let desc = match agent.as_str() {
                    "echo" => "echoes back messages",
                    "basic_chat" => "routes chat to llm",
                    "chat" => "llm chat with session memory",
                    "gmail" => "reads latest email via tool:gmail",
                    "news" => "reads mailbox digest via tool:newsmail_aggregator",
                    "dummy" => "placeholder agent",
                    _ => "enabled custom agent",
                };

                agent_lines.push(format!("{}: {}", display_name, desc));
            }
        }
    }
    #[cfg(not(all(feature = "subsystem-agents", feature = "subsystem-memory")))]
    {
        agent_lines.push("disabled (not compiled)".to_string());
    }

    let mut enabled_tools: Vec<String> = Vec::new();
    #[cfg(feature = "subsystem-tools")]
    {
        #[cfg(feature = "plugin-gmail-tool")]
        {
            enabled_tools.push("gmail".to_string());
            enabled_tools.push("newsmail_aggregator".to_string());
        }
    }
    let tools_line = if enabled_tools.is_empty() {
        "none".to_string()
    } else {
        enabled_tools.join(", ")
    };

    let mut comms_lines = Vec::new();
    comms_lines.push(format!("🖥️  stdio-control: {}", stdio_status));

    #[cfg(feature = "channel-pty")]
    {
        let pty_status = if config.comms.pty.enabled {
            if interactive {
                "disabled (interactive uses stdio control)"
            } else {
                "enabled"
            }
        } else {
            "disabled"
        };
        comms_lines.push(format!("⌨️  pty: {}", pty_status));
    }

    #[cfg(feature = "channel-telegram")]
    {
        let status = if config.comms.telegram.enabled {
            "enabled"
        } else {
            "disabled"
        };
        comms_lines.push(format!("✈️  telegram: {}", status));
    }

    #[cfg(feature = "channel-http")]
    {
        if config.comms.http.enabled {
            comms_lines.push(format!("🌐 http: {}", config.comms.http.bind));
        } else {
            comms_lines.push("🌐 http: disabled".to_string());
        }
    }

    #[cfg(feature = "channel-axum")]
    {
        if config.comms.axum_channel.enabled {
            comms_lines.push(format!("🧩 axum: {}", config.comms.axum_channel.bind));
        } else {
            comms_lines.push("🧩 axum: disabled".to_string());
        }
    }

    #[cfg(not(feature = "channel-http"))]
    if config.comms.http.enabled {
        comms_lines.push("🌐 http: configured but not compiled in".to_string());
    }

    #[cfg(not(feature = "channel-axum"))]
    if config.comms.axum_channel.enabled {
        // CHECK: whats the use of this?
        comms_lines.push("🧩 axum: configured but not compiled in".to_string());
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║ 🤖 Araliya Supervisor Status                                 ║");
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ 🧾 Bot: {:<52}║", config.bot_name);
    println!("║ 🆔 Public ID: {:<46}║", identity.public_id);
    println!("║ 🧠 PID: {:<52}║", pid);
    println!("║ 🛰️  Mode: {:<51}║", mode_text);
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ ⚙️  Subsystems                                               ║");
    println!("║   {}║", fit(format!("✅ {}", subsystem_summary)));
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ 📡 Comms                                                     ║");
    for line in comms_lines {
        println!("║   {}║", fit(line));
    }
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ 🧠 LLM                                                       ║");
    println!("║   {}║", fit(llm_line));
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ 🤝 Agents                                                    ║");
    for line in agent_lines {
        println!("║   {}║", fit(line));
    }
    println!("╟──────────────────────────────────────────────────────────────╢");
    println!("║ 🧰 Tools                                                     ║");
    println!("║   {}║", fit(tools_line));
    #[cfg(feature = "subsystem-tools")]
    {
        println!(
            "║   {}║",
            fit(format!(
                "defaults: newsmail_aggregator(label_ids={:?}, n_last={})",
                config.tools.newsmail_aggregator.label_ids, config.tools.newsmail_aggregator.n_last
            ))
        );
    }
    println!("╚══════════════════════════════════════════════════════════════╝");

    if interactive {
        println!("💡 Type /help for help");
    }
}

// TODO: We used to use clap, but for lean core, we use basic parsing. Check later.
struct CliArgs {
    log_level: Option<&'static str>,
    interactive: bool,
    config_path: Option<String>,
}

fn parse_cli_args() -> CliArgs {
    let mut verbosity = 0u8;
    let mut interactive = false;
    let mut config_path = None;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }

        match arg.as_str() {
            "-h" | "--help" => {
                println!("Usage: araliya-bot [OPTIONS]");
                println!("");
                println!("Options:");
                println!("  -h, --help                 Print help");
                println!("  -i, --interactive          Run in interactive mode (enables PTY console)");
                println!("  -f, --config <PATH>        Path to configuration file (default: config/default.toml)");
                println!("  -v, -vv, -vvv, -vvvv       Increase logging verbosity");
                std::process::exit(0);
            }
            "-i" | "--interactive" => interactive = true,
            "-f" | "--config" => {
                if let Some(path) = iter.next() {
                    config_path = Some(path);
                } else {
                    eprintln!("error: -f/--config requires a path argument");
                    std::process::exit(1);
                }
            }
            "--verbose" => verbosity = verbosity.saturating_add(1),
            a if a.starts_with('-') && a.len() > 1 && a.chars().skip(1).all(|c| c == 'v') => {
                verbosity = verbosity.saturating_add((a.len() - 1) as u8);
            }
            _ => {}
        }
    }

    // Each -v raises verbosity one tier from the config default:
    //   -v      → warn   (suppress info noise, show warnings+errors only)
    //   -vv     → info   (normal operational output — the typical default)
    //   -vvv    → debug  (flow-level diagnostics: routing, handler registration)
    //   -vvvv+  → trace  (full payload dumps, very verbose)
    let log_level = match verbosity {
        0 => None,
        1 => Some("warn"),
        2 => Some("info"),
        3 => Some("debug"),
        _ => Some("trace"),
    };

    CliArgs { log_level, interactive, config_path }
}