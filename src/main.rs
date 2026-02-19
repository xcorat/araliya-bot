//! Araliya Bot — supervisor entry point.
//!
//! Startup sequence:
//!   1. Load .env (if present)
//!   2. Init logger at default level
//!   3. Load config
//!   4. Re-init logger at configured level
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

    // Bootstrap logger at "info" before config is available.
    logger::init("info")?;

    let config = config::load()?;

    info!(
        bot_name = %config.bot_name,
        work_dir = %config.work_dir.display(),
        log_level = %config.log_level,
        "config loaded"
    );

    let identity = identity::setup(&config)?;

    info!(bot_id = %identity.bot_id, "identity ready — starting subsystems");

    // Shared shutdown token — Ctrl-C cancels it, all tasks watch it.
    let shutdown = CancellationToken::new();

    // Build the supervisor bus (buffer = 64 messages).
    let bus = SupervisorBus::new(64);

    // Clone the sender before moving bus into the supervisor task.
    let comms_tx = bus.comms_tx.clone();

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
    subsystems::comms::run(&config, comms_tx, shutdown.clone()).await?;

    // If comms exited due to EOF (not Ctrl-C), still signal everything to stop.
    shutdown.cancel();

    sup_handle.await.ok();

    Ok(())
}

