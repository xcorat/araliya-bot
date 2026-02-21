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

mod config;
mod error;
mod identity;
mod llm;
mod logger;
mod supervisor;
mod subsystems;

use tokio_util::sync::CancellationToken;
use tracing::info;

use supervisor::bus::SupervisorBus;
use supervisor::control::SupervisorControl;
use supervisor::dispatch::BusHandler;

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

    let mut config = config::load()?;

    let args = parse_cli_args();

    // Without -i, no stdio channels are active (daemon-safe default).
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

    info!(bot_id = %identity.bot_id, "identity ready — starting subsystems");

    // Optionally build the memory system.
    #[cfg(feature = "subsystem-memory")]
    let memory = {
        let mem_config = MemoryConfig {
            kv_cap: config.memory_kv_cap,
            transcript_cap: config.memory_transcript_cap,
        };
        let mem = MemorySystem::new(&identity.identity_dir, mem_config)
            .map_err(|e| error::AppError::Memory(e.to_string()))?;
        std::sync::Arc::new(mem)
    };

    // Shared shutdown token — Ctrl-C cancels it, all tasks watch it.
    let shutdown = CancellationToken::new();

    // Build the supervisor bus (buffer = 64 messages).
    let bus = SupervisorBus::new(64);
    // Build the supervisor-internal control plane (buffer = 32 messages).
    let control = SupervisorControl::new(32);

    // Clone the handle before moving bus into the supervisor task.
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
    #[allow(unused_mut)]
    let mut handlers: Vec<Box<dyn BusHandler>> = vec![];
    
    handlers.push(Box::new(ManagementSubsystem::new(
        control_handle.clone(),
        bus_handle.clone(),
        ManagementInfo {
            bot_id: identity.bot_id.clone(),
            llm_provider: config.llm.provider.clone(),
            llm_model: config.llm.openai.model.clone(),
            llm_timeout_seconds: config.llm.openai.timeout_seconds,
        },
    )));

    #[cfg(feature = "subsystem-llm")]
    {
        let llm = LlmSubsystem::new(&config.llm, config.llm_api_key.clone())
            .map_err(|e| error::AppError::Config(e.to_string()))?;
        handlers.push(Box::new(llm));
    }

    #[cfg(feature = "subsystem-tools")]
    {
        handlers.push(Box::new(ToolsSubsystem::new()));
    }

    #[cfg(feature = "subsystem-agents")]
    {
        #[cfg(feature = "subsystem-memory")]
        let agents = AgentsSubsystem::new(config.agents.clone(), bus_handle.clone(), memory.clone());
        #[cfg(not(feature = "subsystem-memory"))]
        let agents = AgentsSubsystem::new(config.agents.clone(), bus_handle.clone());
        handlers.push(Box::new(agents));
    }

    #[cfg(feature = "subsystem-cron")]
    {
        let cron = CronSubsystem::new(bus_handle.clone(), shutdown.clone());
        handlers.push(Box::new(cron));
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
        );
        comms.join().await?;
    }

    #[cfg(not(feature = "subsystem-comms"))]
    {
        // If no comms, we might want to wait for shutdown token or just exit
        // For now, let's keep it running if the supervisor is active.
        info!("no comms subsystem enabled — waiting for shutdown signal");
        shutdown.cancelled().await;
    }

    // If comms exited due to EOF (not Ctrl-C), still signal everything to stop.
    shutdown.cancel();

    sup_handle.await.ok();

    // In interactive mode, print a clean exit line so the shell prompt
    // appears below the tracing output.  In daemon mode, exit silently.
    if args.interactive {
        use std::io::Write as _;
        println!("\nBye :)");
        let _ = std::io::stdout().flush();
    }
    let _ = { use std::io::Write as _; std::io::stderr().flush() };

    Ok(())
}

struct CliArgs {
    log_level: Option<&'static str>,
    interactive: bool,
}

fn parse_cli_args() -> CliArgs {
    let mut verbosity = 0u8;
    let mut interactive = false;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }

        match arg.as_str() {
            "-i" | "--interactive" => interactive = true,
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

    CliArgs { log_level, interactive }
}

