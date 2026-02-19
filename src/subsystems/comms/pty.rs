//! PTY (console) comms channel — reads lines from stdin, sends to supervisor,
//! prints the reply to stdout.
//!
//! Implements [`Channel`] so the comms subsystem can spawn it as an
//! independent task.  All supervisor communication goes through
//! [`CommsState::send_message`] — this module has no direct bus access.
//!
//! Auto-loaded when no other comms channel is configured. Runs until the
//! `shutdown` token is cancelled (Ctrl-C) or stdin is closed.

use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;
use super::state::{CommsEvent, CommsState};
use super::Channel;

// ── PtyChannel ───────────────────────────────────────────────────────────────

/// A PTY channel instance.  Multiple instances would each get a unique id.
pub struct PtyChannel {
    channel_id: String,
}

impl PtyChannel {
    pub fn new(channel_id: impl Into<String>) -> Self {
        Self { channel_id: channel_id.into() }
    }
}

impl Channel for PtyChannel {
    fn id(&self) -> &str {
        &self.channel_id
    }

    fn run(
        self: Box<Self>,
        state: Arc<CommsState>,
        shutdown: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>> {
        Box::pin(run_pty(self.channel_id, state, shutdown))
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
                        break;
                    }
                    Ok(None) => {
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
