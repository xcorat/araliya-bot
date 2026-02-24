//! Unix-domain socket management adapter.
//!
//! Listens on `{work_dir}/araliya.sock`, accepts multiple concurrent
//! connections, and processes newline-delimited JSON [`ControlCommand`]
//! requests, responding with [`WireResponse`] JSON lines.
//!
//! Protocol (per connection):
//!   → `<ControlCommand JSON>\n`
//!   ← `<WireResponse JSON>\n`
//!
//! The connection stays open and can process multiple request/response pairs.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::supervisor::control::{ControlCallError, ControlCommand, ControlError, ControlHandle, WireResponse};

pub fn start(control: ControlHandle, socket_path: PathBuf, shutdown: CancellationToken) {
    // Remove stale socket file left by a previous run.
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            error!(
                socket = %socket_path.display(),
                error = %e,
                "management socket bind failed — araliya-ctl will not work"
            );
            return;
        }
    };

    info!(socket = %socket_path.display(), "management socket listening");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    info!("management socket shutting down");
                    let _ = std::fs::remove_file(&socket_path);
                    break;
                }

                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let ctl = control.clone();
                            let tok = shutdown.clone();
                            tokio::spawn(handle_connection(stream, ctl, tok));
                        }
                        Err(e) => {
                            warn!(error = %e, "management socket accept error");
                        }
                    }
                }
            }
        }
    });
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    control: ControlHandle,
    shutdown: CancellationToken,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => break,

            line = lines.next_line() => {
                match line {
                    Ok(None) => break, // client closed connection
                    Ok(Some(l)) if l.trim().is_empty() => continue,
                    Ok(Some(l)) => {
                        let wire = dispatch(&l, &control).await;
                        let mut json = match serde_json::to_string(&wire) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!(error = %e, "management socket serialise error");
                                continue;
                            }
                        };
                        json.push('\n');
                        if writer.write_all(json.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "management socket connection read error");
                        break;
                    }
                }
            }
        }
    }
}

async fn dispatch(line: &str, control: &ControlHandle) -> WireResponse {
    let cmd: ControlCommand = match serde_json::from_str(line) {
        Ok(c) => c,
        Err(e) => {
            return WireResponse::Err(ControlError::Invalid {
                message: format!("parse error: {e}"),
            });
        }
    };

    debug!(cmd = ?cmd, "management socket dispatching command");

    match control.request(cmd).await {
        Ok(result) => WireResponse::from(result),
        Err(ControlCallError::Send) => WireResponse::Err(ControlError::Invalid {
            message: "supervisor not running".into(),
        }),
        Err(e) => WireResponse::Err(ControlError::Invalid {
            message: e.to_string(),
        }),
    }
}
