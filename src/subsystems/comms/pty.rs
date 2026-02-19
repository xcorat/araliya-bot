//! PTY (console) comms channel — reads lines from stdin, sends to supervisor,
//! prints the reply to stdout.
//!
//! Auto-loaded when no other comms channel is configured. Runs until the
//! `shutdown` token is cancelled (Ctrl-C) or stdin is closed.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::state::CommsState;
use crate::error::AppError;

/// Run the PTY channel until `shutdown` is cancelled or stdin is closed.
pub async fn run(state: Arc<CommsState>, shutdown: CancellationToken) -> Result<(), AppError> {
    info!("pty channel started — type a message and press Enter. Ctrl-C to quit.");
    println!("─────────────────────────────────");
    println!(" Araliya console  (Ctrl-C to quit)");
    println!("─────────────────────────────────");

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        print!("> ");
        // flush the prompt — stdout is line-buffered by default
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                println!("\n[pty] shutdown signal received — closing console channel");
                info!("pty channel shutting down");
                break;
            }

            line = lines.next_line() => {
                match line {
                    Err(e) => {
                        warn!("pty read error: {e}");
                        break;
                    }
                    Ok(None) => {
                        // stdin closed (EOF)
                        info!("pty stdin closed");
                        break;
                    }
                    Ok(Some(input)) => {
                        let input = input.trim().to_string();
                        if input.is_empty() {
                            continue;
                        }

                        debug!(input = %input, "pty received line");

                        let (reply_tx, reply_rx) = oneshot::channel();
                        if state.comms_tx.send(crate::supervisor::bus::CommsMessage {
                            content: input,
                            reply_tx,
                        }).await.is_err() {
                            warn!("supervisor bus closed, pty exiting");
                            break;
                        }

                        match reply_rx.await {
                            Ok(reply) => println!("{reply}"),
                            Err(_) => warn!("supervisor dropped reply sender"),
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
