//! PTY (console) comms channel — reads lines from stdin, sends to supervisor,
//! prints the reply to stdout.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::state::{CommsEvent, CommsState};
use araliya_core::error::AppError;
use araliya_core::runtime::{Component, ComponentFuture};

// ── PtyChannel ───────────────────────────────────────────────────────────────

pub struct PtyChannel {
    channel_id: String,
    state: Arc<CommsState>,
}

impl PtyChannel {
    pub fn new(channel_id: impl Into<String>, state: Arc<CommsState>) -> Self {
        Self {
            channel_id: channel_id.into(),
            state,
        }
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
                        println!();
                        info!("pty stdin closed");
                        break;
                    }
                    Ok(Some(input)) => {
                        let input = input.trim().to_string();
                        if input.is_empty() { continue; }

                        debug!(input = %input, "pty received line");

                        let result = tokio::select! {
                            biased;
                            _ = shutdown.cancelled() => {
                                println!();
                                break;
                            }
                            r = state.send_message(&channel_id, input, None, None) => r,
                        };
                        match result {
                            Err(e) => {
                                warn!("send_message error: {e}, pty exiting");
                                break;
                            }
                            Ok(reply) => println!("{}", reply.reply),
                        }
                    }
                }
            }
        }
    }

    state.report_event(CommsEvent::ChannelShutdown { channel_id });
    Ok(())
}
