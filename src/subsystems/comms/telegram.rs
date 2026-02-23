//! Telegram comms channel — receives messages via Telegram API, sends to supervisor,
//! and replies back to the user.

use std::sync::Arc;
use std::env;

use teloxide::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::subsystems::runtime::{Component, ComponentFuture};
use super::state::CommsState;

// ── Constants ────────────────────────────────────────────────────────────────

/// Telegram has a 4096 character limit per message.
/// We chunk at 4000 to be safe.
const MAX_MESSAGE_LENGTH: usize = 4000;

// ── TelegramChannel ──────────────────────────────────────────────────────────

/// A Telegram channel instance.
pub struct TelegramChannel {
    channel_id: String,
    state: Arc<CommsState>,
}

impl TelegramChannel {
    pub fn new(channel_id: impl Into<String>, state: Arc<CommsState>) -> Self {
        Self { channel_id: channel_id.into(), state }
    }
}

impl Component for TelegramChannel {
    fn id(&self) -> &str {
        &self.channel_id
    }

    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture {
        Box::pin(run_telegram(self.channel_id, self.state, shutdown))
    }
}

// ── run_telegram ─────────────────────────────────────────────────────────────

async fn run_telegram(
    channel_id: String,
    state: Arc<CommsState>,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let token = match env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            warn!(%channel_id, "TELEGRAM_BOT_TOKEN not set, telegram channel exiting");
            return Ok(());
        }
    };

    info!(%channel_id, "telegram channel starting");

    let bot = Bot::new(token);
    
    let state_clone = state.clone();
    let channel_id_clone = channel_id.clone();
    
    let handler = Update::filter_message().endpoint(
        move |bot: Bot, msg: Message| {
            let state = state_clone.clone();
            let channel_id = channel_id_clone.clone();
            async move {
                if let Some(text) = msg.text() {
                    debug!(%channel_id, from = ?msg.from.as_ref().and_then(|u| u.username.as_ref()), "telegram received message");
                    
                    match state.send_message(&channel_id, text.to_string(), None).await {
                        Ok(reply) => {
                            let mut text = reply.reply;
                            if text.is_empty() {
                                text = "(empty response)".to_string();
                            }
                            
                            // Telegram has a 4096 character limit per message.
                            // We chunk at MAX_MESSAGE_LENGTH to be safe.
                            let chars: Vec<char> = text.chars().collect();
                            
                            for chunk in chars.chunks(MAX_MESSAGE_LENGTH) {
                                let chunk_str: String = chunk.iter().collect();
                                if let Err(e) = bot.send_message(msg.chat.id, chunk_str).await {
                                    warn!("failed to send telegram reply: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            warn!("send_message error: {e}");
                            let _ = bot.send_message(msg.chat.id, "Internal error processing message.").await;
                        }
                    }
                }
                respond(())
            }
        }
    );

    let mut dispatcher = Dispatcher::builder(bot, handler).build();

    tokio::select! {
        biased;

        _ = shutdown.cancelled() => {
            info!(%channel_id, "shutdown signal received — closing telegram channel");
        }
        _ = dispatcher.dispatch() => {
            warn!(%channel_id, "telegram dispatcher exited unexpectedly");
        }
    }

    Ok(())
}
