//! PTY (console) comms channel — reads lines from stdin, sends to supervisor,
//! prints the reply to stdout.
//!
//! [`PtyChannel`] implements [`runtime::Component`] directly.  State
//! (`Arc<CommsState>`) is captured at construction time so the generic
//! `Component::run(self, shutdown)` signature applies without modification.
//! All supervisor communication goes through [`CommsState::send_message`]
//! — this module has no direct bus access.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::subsystems::runtime::{Component, ComponentFuture};
use super::state::{CommsEvent, CommsState};

// ── PtyChannel ───────────────────────────────────────────────────────────────

/// A PTY channel instance.  Multiple instances would each get a unique id.
/// State is captured at construction; the generic `Component` interface
/// is used for all lifecycle management.
pub struct PtyChannel {
    channel_id: String,
    state: Arc<CommsState>,
}

impl PtyChannel {
    pub fn new(channel_id: impl Into<String>, state: Arc<CommsState>) -> Self {
        Self { channel_id: channel_id.into(), state }
    }
}

impl Component for PtyChannel {
    fn id(&self) -> &str {
        &self.channel_id
    }

    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture {
        Box::pin(run_pty(self.channel_id, self.state, shutdown))
    }
}

// ── run_pty ──────────────────────────────────────────────────────────────────

async fn run_pty(
    channel_id: String,
    state: Arc<CommsState>,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    info!(%channel_id, "pty channel started — type a message and press Enter. Ctrl-C to quit.");
    println!("─────────────────────────────────");
    println!(" Araliya console  (Ctrl-C to quit)");
    println!("─────────────────────────────────");

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        print!("> ");
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
                        println!();
                        break;
                    }
                    Ok(None) => {
                        // Ctrl-D / EOF — move past the "> " prompt.
                        println!();
                        info!("pty stdin closed");
                        break;
                    }
                    Ok(Some(input)) => {
                        let input = input.trim().to_string();
                        if input.is_empty() { continue; }

                        debug!(input = %input, "pty received line");

                        match state.send_message(&channel_id, input).await {
                            Err(e) => {
                                warn!("send_message error: {e}, pty exiting");
                                break;
                            }
                            Ok(reply) => println!("{reply}"),
                        }
                    }
                }
            }
        }
    }

    state.report_event(CommsEvent::ChannelShutdown { channel_id });
    Ok(())
}
