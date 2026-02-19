//! Araliya Bot — supervisor entry point.
//!
//! Startup sequence:
//!   1. Load .env (if present)
//!   2. Init logger at default level
//!   3. Load config
//!   4. Re-init logger at configured level
//!   5. Setup bot identity
//!   6. Print status and exit

mod config;
mod error;
mod identity;
mod logger;

use tracing::info;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), error::AppError> {
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

    info!(bot_id = %identity.bot_id, "identity ready");
    println!("✓ Bot initialized: bot_id={}", identity.bot_id);

    Ok(())
}
