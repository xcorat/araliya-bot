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
mod logger;
mod supervisor;
mod subsystems;

use tokio_util::sync::CancellationToken;
use tracing::info;

use supervisor::bus::SupervisorBus;

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

    // Shared shutdown token — Ctrl-C cancels it, all tasks watch it.
    let shutdown = CancellationToken::new();

    // Build the supervisor bus (buffer = 64 messages).
    let bus = SupervisorBus::new(64);

    // Clone the handle before moving bus into the supervisor task.
    let bus_handle = bus.handle.clone();

    // Ctrl-C handler — cancels the token so all tasks shut down.
    let ctrlc_token = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("ctrl-c received — initiating shutdown");
            ctrlc_token.cancel();
        }
    });

    // Spawn supervisor run-loop (owns the bus receiver).
    let sup_token = shutdown.clone();
    let sup_handle = tokio::spawn(async move {
        supervisor::run(bus, sup_token).await;
    });

    // Run comms subsystem — drives the console on this task until shutdown.
    // When this returns (Ctrl-C or stdin EOF), we ensure shutdown is cancelled
    // so the supervisor and any other tasks also stop.
    subsystems::comms::run(&config, bus_handle, shutdown.clone()).await?;

    // If comms exited due to EOF (not Ctrl-C), still signal everything to stop.
    shutdown.cancel();

    sup_handle.await.ok();

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

    match verbosity {
        0 => None,
        1 => Some("debug"),
        _ => Some("trace"),
    }
}

