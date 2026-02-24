
//! Unix-domain-socket client — mirrors the wire protocol in `araliya-ctl`.
//!
//! All types are inlined so this binary has no dependency on internal crate
//! modules. Must stay in sync with `supervisor::control` serde representations.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ── Wire types ─────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub enum Command {
    Health,
    Status,
    SubsystemsList,
}

#[derive(Debug, serde::Deserialize)]
pub enum ControlResponse {
    Health { uptime_ms: u64 },
    Status { uptime_ms: u64, handlers: Vec<String> },
    Subsystems { handlers: Vec<String> },
    Ack { message: String },
}

#[derive(Debug, serde::Deserialize)]
pub enum ControlError {
    NotImplemented { message: String },
    Invalid { message: String },
}

#[derive(Debug, serde::Deserialize)]
pub enum WireResponse {
    #[serde(rename = "ok")]
    Ok(ControlResponse),
    #[serde(rename = "err")]
    Err(ControlError),
}

// ── Socket path resolution ─────────────────────────────────────────────────

pub fn socket_path() -> PathBuf {
    if let Ok(work_dir) = std::env::var("ARALIYA_WORK_DIR") {
        let path = if work_dir.starts_with("~/") {
            dirs::home_dir()
                .map(|h| h.join(&work_dir[2..]))
                .unwrap_or_else(|| PathBuf::from(&work_dir))
        } else {
            PathBuf::from(&work_dir)
        };
        return path.join("araliya.sock");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".araliya")
        .join("araliya.sock")
}

// ── Client ─────────────────────────────────────────────────────────────────

/// Send a command to the daemon and return the formatted response string.
/// Returns an error string if the daemon is unreachable or responds with an error.
pub async fn send_command(cmd: Command) -> Result<String, String> {
    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let path = socket_path();
        let stream = UnixStream::connect(&path).await.map_err(|e| {
            format!("connect {}: {e}\n  is the daemon running?", path.display())
        })?;

        let (reader, mut writer) = stream.into_split();

        let mut request =
            serde_json::to_string(&cmd).map_err(|e| format!("serialise: {e}"))?;
        request.push('\n');
        writer
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("send: {e}"))?;

        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await
            .map_err(|e| format!("recv: {e}"))?
            .ok_or_else(|| "daemon closed without responding".to_string())?;

        let resp: WireResponse =
            serde_json::from_str(&line).map_err(|e| format!("parse: {e}\n  raw: {line}"))?;

        Ok(format_response(resp))
    }

    #[cfg(not(unix))]
    {
        let _ = cmd;
        Err("IPC requires Unix (Unix domain sockets not available on this platform)".to_string())
    }
}

fn format_response(resp: WireResponse) -> String {
    match resp {
        WireResponse::Ok(r) => match r {
            ControlResponse::Health { uptime_ms } => {
                let s = uptime_ms / 1000;
                let ms = uptime_ms % 1000;
                format!("ok  uptime {s}.{ms:03}s")
            }
            ControlResponse::Status { uptime_ms, handlers } => {
                let s = uptime_ms / 1000;
                let ms = uptime_ms % 1000;
                format!("ok  uptime {s}.{ms:03}s  handlers: {}", handlers.join(", "))
            }
            ControlResponse::Subsystems { handlers } => {
                format!("ok  subsystems: {}", handlers.join(", "))
            }
            ControlResponse::Ack { message } => format!("ok  {message}"),
        },
        WireResponse::Err(e) => match e {
            ControlError::NotImplemented { message } => format!("err  not implemented: {message}"),
            ControlError::Invalid { message } => format!("err  {message}"),
        },
    }
}
