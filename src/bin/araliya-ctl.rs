//! `araliya-ctl` — management CLI for the Araliya bot daemon.
//!
//! Connects to the bot's Unix domain socket and sends a single management
//! command, printing the response to stdout.
//!
//! # Usage
//!
//! ```text
//! araliya-ctl [--socket <path>] <command>
//!
//! Commands:
//!   health       show bot uptime
//!   status       show uptime + active subsystem handlers
//!   subsystems   list all registered subsystem handlers
//!   shutdown     request a graceful daemon shutdown
//!
//! Flags:
//!   --socket <path>   override socket path (default: {work_dir}/araliya.sock)
//!   --help, -h        print this help
//! ```
//!
//! Socket path resolution order:
//!   1. `--socket <path>` flag
//!   2. `$ARALIYA_WORK_DIR/araliya.sock`
//!   3. `~/.araliya/araliya.sock`

use std::path::PathBuf;
use std::process;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ── Wire types (mirrored from supervisor::control) ────────────────────────
// Kept minimal and inline so this binary has no dependency on internal crate
// modules. Must match the server-side serde representation exactly.

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum ControlCommand {
    Health,
    Status,
    SubsystemsList,
    SubsystemEnable { id: String },
    SubsystemDisable { id: String },
    Shutdown,
}

#[derive(Debug, serde::Deserialize)]
enum ControlResponse {
    Health { uptime_ms: u64 },
    Status { uptime_ms: u64, handlers: Vec<String> },
    Subsystems { handlers: Vec<String> },
    Ack { message: String },
}

#[derive(Debug, serde::Deserialize)]
enum ControlError {
    NotImplemented { message: String },
    Invalid { message: String },
}

#[derive(Debug, serde::Deserialize)]
enum WireResponse {
    #[serde(rename = "ok")]
    Ok(ControlResponse),
    #[serde(rename = "err")]
    Err(ControlError),
}

// ── CLI arg parsing ────────────────────────────────────────────────────────

struct Args {
    socket: Option<String>,
    command: Option<String>,
    rest: Vec<String>,
}

fn parse_args() -> Args {
    let mut socket = None;
    let mut command = None;
    let mut rest = Vec::new();
    let mut iter = std::env::args().skip(1).peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--socket" | "-s" => {
                socket = iter.next();
            }
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            "--" => {
                rest.extend(iter);
                break;
            }
            _ if command.is_none() => command = Some(arg),
            _ => rest.push(arg),
        }
    }

    Args { socket, command, rest }
}

fn print_help() {
    eprintln!("usage: araliya-ctl [--socket <path>] <command>");
    eprintln!();
    eprintln!("commands:");
    eprintln!("  health              show bot uptime");
    eprintln!("  status              show uptime + active subsystem handlers");
    eprintln!("  subsystems          list registered subsystem handlers");
    eprintln!("  shutdown            request graceful daemon shutdown");
    eprintln!();
    eprintln!("flags:");
    eprintln!("  --socket, -s <path>   override default socket path");
    eprintln!("  --help,   -h          print this help");
    eprintln!();
    eprintln!("socket path resolution:");
    eprintln!("  1. --socket flag");
    eprintln!("  2. $ARALIYA_WORK_DIR/araliya.sock");
    eprintln!("  3. ~/.araliya/araliya.sock");
}

fn resolve_socket_path(override_path: Option<String>) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }
    if let Ok(work_dir) = std::env::var("ARALIYA_WORK_DIR") {
        let expanded = if work_dir.starts_with("~/") {
            if let Some(home) = home_dir() {
                home.join(&work_dir[2..])
            } else {
                PathBuf::from(&work_dir)
            }
        } else {
            PathBuf::from(&work_dir)
        };
        return expanded.join("araliya.sock");
    }
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".araliya")
        .join("araliya.sock")
}

fn home_dir() -> Option<PathBuf> {
    // Use $HOME env var first (works in most cases incl. sudo -u).
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    // fallback: dirs crate
    dirs::home_dir()
}

fn build_command(cmd: &str, rest: &[String]) -> Result<ControlCommand, String> {
    match cmd {
        "health" => Ok(ControlCommand::Health),
        "status" => Ok(ControlCommand::Status),
        "subsystems" | "subsys" => Ok(ControlCommand::SubsystemsList),
        "shutdown" => Ok(ControlCommand::Shutdown),
        "enable" => {
            let id = rest.first().ok_or("usage: araliya-ctl enable <id>")?;
            Ok(ControlCommand::SubsystemEnable { id: id.clone() })
        }
        "disable" => {
            let id = rest.first().ok_or("usage: araliya-ctl disable <id>")?;
            Ok(ControlCommand::SubsystemDisable { id: id.clone() })
        }
        other => Err(format!("unknown command: {other}\n  run 'araliya-ctl --help' for usage")),
    }
}

fn print_response(resp: WireResponse) {
    match resp {
        WireResponse::Ok(r) => match r {
            ControlResponse::Health { uptime_ms } => {
                let secs = uptime_ms / 1000;
                let ms = uptime_ms % 1000;
                println!("ok  uptime {secs}.{ms:03}s");
            }
            ControlResponse::Status { uptime_ms, handlers } => {
                let secs = uptime_ms / 1000;
                let ms = uptime_ms % 1000;
                println!("ok  uptime {secs}.{ms:03}s");
                println!("    handlers ({}):", handlers.len());
                for h in &handlers {
                    println!("      {h}");
                }
            }
            ControlResponse::Subsystems { handlers } => {
                println!("ok  subsystems ({}):", handlers.len());
                for h in &handlers {
                    println!("      {h}");
                }
            }
            ControlResponse::Ack { message } => {
                println!("ok  {message}");
            }
        },
        WireResponse::Err(e) => {
            let msg = match e {
                ControlError::NotImplemented { message } => format!("not implemented: {message}"),
                ControlError::Invalid { message } => message,
            };
            eprintln!("error: {msg}");
            process::exit(1);
        }
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

fn main() {
    let args = parse_args();

    let cmd_str = match args.command {
        Some(ref c) => c.as_str().to_string(),
        None => {
            eprintln!("error: no command given");
            eprintln!("  run 'araliya-ctl --help' for usage");
            process::exit(1);
        }
    };

    let cmd = match build_command(&cmd_str, &args.rest) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    let socket_path = resolve_socket_path(args.socket);

    #[cfg(not(unix))]
    {
        eprintln!("error: araliya-ctl requires a Unix system (Unix domain sockets)");
        process::exit(1);
    }

    #[cfg(unix)]
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        if let Err(e) = rt.block_on(run(socket_path, cmd)) {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }
}

#[cfg(unix)]
async fn run(socket_path: PathBuf, cmd: ControlCommand) -> Result<(), String> {
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(&socket_path).await.map_err(|e| {
        format!(
            "cannot connect to {}: {e}\n  is the daemon running?",
            socket_path.display()
        )
    })?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let mut request = serde_json::to_string(&cmd)
        .map_err(|e| format!("serialise error: {e}"))?;
    request.push('\n');

    writer
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("send error: {e}"))?;

    let line = lines
        .next_line()
        .await
        .map_err(|e| format!("recv error: {e}"))?
        .ok_or_else(|| "daemon closed connection without responding".to_string())?;

    let resp: WireResponse =
        serde_json::from_str(&line).map_err(|e| format!("parse response error: {e}\n  raw: {line}"))?;

    print_response(resp);
    Ok(())
}
