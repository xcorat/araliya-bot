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

#[cfg(feature = "subsystem-memory")]
use subsystems::memory::{MemoryConfig, MemorySystem};

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

    let config = config::load()?;

    let cli_log_level = cli_log_level_override_from_args();
    let effective_log_level = cli_log_level.unwrap_or(config.log_level.as_str());
    let force_cli_level = cli_log_level.is_some();

    logger::init(effective_log_level, force_cli_level)?;

    info!(
        bot_name = %config.bot_name,
        work_dir = %config.work_dir.display(),
        configured_log_level = %config.log_level,
        effective_log_level = %effective_log_level,
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

    #[cfg(feature = "subsystem-llm")]
    {
        let llm = LlmSubsystem::new(&config.llm, config.llm_api_key.clone())
            .map_err(|e| error::AppError::Config(e.to_string()))?;
        handlers.push(Box::new(llm));
    }

    #[cfg(feature = "subsystem-agents")]
    {
        #[cfg(feature = "subsystem-memory")]
        let agents = AgentsSubsystem::new(config.agents.clone(), bus_handle.clone(), Some(memory.clone()));
        #[cfg(not(feature = "subsystem-memory"))]
        let agents = AgentsSubsystem::new(config.agents.clone(), bus_handle.clone(), None);
        handlers.push(Box::new(agents));
    }

    // Spawn supervisor run-loop (owns the bus receiver).
    let sup_token = shutdown.clone();
    let sup_handle = tokio::spawn(async move {
        supervisor::run(bus, control, sup_token, handlers).await;
    });

    // Start transport adapter boundary for supervisor control plane.
    subsystems::management::start(control_handle, shutdown.clone());

    // Start comms channels as independent concurrent tasks.
    #[cfg(feature = "subsystem-comms")]
    {
        let comms = subsystems::comms::start(&config, bus_handle, shutdown.clone());
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

    // Final newline + flush so the shell prompt appears on a clean line
    // after all tracing output (stderr) has been written.
    {
        use std::io::Write as _;
        println!("\nBye :)");
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
    }

    Ok(())
}

fn cli_log_level_override_from_args() -> Option<&'static str> {
    let mut verbosity = 0u8;

    for arg in std::env::args().skip(1) {
        if arg == "--" {
            break;
        }

        if arg == "--verbose" {
            verbosity = verbosity.saturating_add(1);
            continue;
        }

        if arg.starts_with('-') && arg.len() > 1 && arg.chars().skip(1).all(|c| c == 'v') {
            verbosity = verbosity.saturating_add((arg.len() - 1) as u8);
        }
    }

    // Each -v raises verbosity one tier from the config default:
    //   -v      → warn   (suppress info noise, show warnings+errors only)
    //   -vv     → info   (normal operational output — the typical default)
    //   -vvv    → debug  (flow-level diagnostics: routing, handler registration)
    //   -vvvv+  → trace  (full payload dumps, very verbose)
    match verbosity {
        0 => None,
        1 => Some("warn"),
        2 => Some("info"),
        3 => Some("debug"),
        _ => Some("trace"),
    }
}

