//! Comms subsystem — manages all external I/O channels.
//!
//! Each channel (PTY, HTTP, Telegram…) implements [`araliya_core::runtime::Component`]
//! and is spawned as an independent concurrent task by [`start`].

#[cfg(feature = "channel-axum")]
pub mod axum_channel;
pub mod http;
pub mod pty;
pub mod state;
#[cfg(feature = "channel-telegram")]
pub mod telegram;

pub use state::{CommsEvent, CommsState};

use std::sync::{Arc, OnceLock};

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[cfg(feature = "subsystem-ui")]
use araliya_core::ui::UiServeHandle;

use araliya_core::bus::{
    BusError, BusHandle, BusHandler, BusPayload, BusResult, ComponentInfo, ComponentStatusResponse,
    ERR_METHOD_NOT_FOUND,
};
use araliya_core::config::Config;
use araliya_core::runtime::{spawn_components, Component, SubsystemHandle};

// ── CommsStatusHandler ───────────────────────────────────────────────────────

/// Minimal [`BusHandler`] that exposes `comms/status` and `comms/{channel_id}/status`
/// for the comms subsystem and its channel children.
pub struct CommsStatusHandler {
    comms_info: Arc<OnceLock<ComponentInfo>>,
}

impl CommsStatusHandler {
    pub fn new(comms_info: Arc<OnceLock<ComponentInfo>>) -> Self {
        Self { comms_info }
    }
}

impl BusHandler for CommsStatusHandler {
    fn prefix(&self) -> &str {
        "comms"
    }

    fn component_info(&self) -> ComponentInfo {
        self.comms_info
            .get()
            .cloned()
            .unwrap_or_else(|| ComponentInfo::running("comms", "Comms", vec![]))
    }

    fn handle_request(
        &self,
        method: &str,
        _payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        if method == "comms/status" {
            let resp = ComponentStatusResponse::running("comms");
            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: resp.to_json(),
            }));
            return;
        }

        if method == "comms/detailed_status" {
            let channel_ids: Vec<String> = self
                .comms_info
                .get()
                .map(|info| info.children.iter().map(|c| c.id.clone()).collect())
                .unwrap_or_default();
            let data = serde_json::json!({
                "id": "comms",
                "status": "running",
                "state": "on",
                "channels": channel_ids,
            });
            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: data.to_string(),
            }));
            return;
        }

        // comms/{channel_id}/status
        if let Some(channel_id) = method
            .strip_prefix("comms/")
            .and_then(|rest| rest.strip_suffix("/status"))
        {
            let exists = self
                .comms_info
                .get()
                .map(|info| info.children.iter().any(|c| c.id == channel_id))
                .unwrap_or(false);
            let resp = if exists {
                ComponentStatusResponse::running(channel_id)
            } else {
                ComponentStatusResponse::error(channel_id, "not found")
            };
            let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                data: resp.to_json(),
            }));
            return;
        }

        let _ = reply_tx.send(Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("comms method not found: {method}"),
        )));
    }
}

// ── start ───────────────────────────────────────────────────────────────────

/// Spawn all configured comms channels and return a [`SubsystemHandle`].
pub fn start(
    config: &Config,
    bus: BusHandle,
    shutdown: CancellationToken,
    #[cfg(feature = "subsystem-ui")] ui_handle: Option<UiServeHandle>,
    #[cfg(any(feature = "plugin-homebuilder", feature = "plugin-webbuilder"))] preview_root: Option<std::path::PathBuf>,
    comms_info: Arc<OnceLock<ComponentInfo>>,
    stdio_control_active: bool,
) -> SubsystemHandle {
    let (event_tx, event_rx) = mpsc::channel::<CommsEvent>(32);
    let state = Arc::new(CommsState::new(bus, event_tx));

    let mut components: Vec<Box<dyn Component>> = Vec::new();

    #[cfg(feature = "channel-pty")]
    {
        let pty_requested = config.comms_pty_should_load();

        if pty_requested && !stdio_control_active {
            info!("loading pty channel");
            components.push(Box::new(pty::PtyChannel::new("pty0", state.clone())));
        } else if pty_requested && stdio_control_active {
            info!(
                "pty channel disabled: stdio management adapter is active (virtual /chat route enabled)"
            );
        }
    }

    #[cfg(feature = "channel-telegram")]
    {
        if config.comms_telegram_should_load() {
            info!("loading telegram channel");
            components.push(Box::new(telegram::TelegramChannel::new(
                "telegram0",
                state.clone(),
            )));
        }
    }

    #[cfg(feature = "channel-http")]
    {
        if config.comms_http_should_load() {
            info!(bind = %config.comms.http.bind, "loading http channel");
            #[cfg(feature = "subsystem-ui")]
            let ui = ui_handle.clone();
            #[cfg(not(feature = "subsystem-ui"))]
            let ui: Option<()> = None;
            components.push(Box::new(http::HttpChannel::new(
                "http0",
                config.comms.http.bind.clone(),
                state.clone(),
                ui,
            )));
        }
    }
    #[cfg(not(feature = "channel-http"))]
    if config.comms_http_should_load() {
        tracing::warn!(
            "config has [comms.http] enabled = true but this binary was compiled \
             without the `channel-http` feature — channel will not start. \
             Rebuild with `--features channel-http` or set enabled = false."
        );
    }

    #[cfg(feature = "channel-axum")]
    {
        if config.comms_axum_should_load() {
            info!(bind = %config.comms.axum_channel.bind, "loading axum channel");
            #[cfg(feature = "subsystem-ui")]
            let ui = ui_handle.clone();
            #[cfg(not(feature = "subsystem-ui"))]
            let ui: Option<()> = None;
            components.push(Box::new(axum_channel::AxumChannel::new(
                "axum0",
                config.comms.axum_channel.bind.clone(),
                state.clone(),
                ui,
                #[cfg(any(feature = "plugin-homebuilder", feature = "plugin-webbuilder"))]
                preview_root.clone(),
            )));
        }
    }
    #[cfg(not(feature = "channel-axum"))]
    if config.comms_axum_should_load() {
        tracing::warn!(
            "config has [comms.axum_channel] enabled = true but this binary was compiled \
             without the `channel-axum` feature — channel will not start. \
             Rebuild with `--features channel-axum` or set enabled = false."
        );
    }

    if components.is_empty() {
        info!("no comms channels configured — waiting for shutdown");
    }

    {
        let channel_children: Vec<ComponentInfo> = components
            .iter()
            .map(|c| ComponentInfo::leaf(c.id(), &ComponentInfo::capitalise(c.id())))
            .collect();
        let _ = comms_info.set(ComponentInfo::running("comms", "Comms", channel_children));
    }

    tokio::spawn(async move {
        let mut rx = event_rx;
        while let Some(event) = rx.recv().await {
            match event {
                CommsEvent::ChannelShutdown { ref channel_id } => {
                    debug!(channel_id, "channel reported shutdown");
                }
                CommsEvent::SessionStarted { ref channel_id } => {
                    debug!(channel_id, "channel session started");
                }
            }
        }
    });

    spawn_components(components, shutdown)
}
