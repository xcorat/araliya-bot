use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::supervisor::bus::{BusHandle, BusPayload};
use crate::supervisor::control::{ControlCommand, ControlHandle, ControlResponse};

const VIRTUAL_PTY_CHANNEL_ID: &str = "pty0";

static STDIO_CONTROL_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Returns true when the supervisor stdio adapter owns stdio.
pub fn stdio_control_active() -> bool {
    STDIO_CONTROL_ACTIVE.load(Ordering::Relaxed)
}

fn set_stdio_control_active(active: bool) {
    STDIO_CONTROL_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn start(
    control: ControlHandle,
    bus: BusHandle,
    shutdown: CancellationToken,
    interactive_enabled: bool,
) {
    let stdin_is_terminal = std::io::stdin().is_terminal();
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let interactive_tty = stdin_is_terminal && stdout_is_terminal;

    if interactive_tty && !interactive_enabled {
        set_stdio_control_active(false);
        info!("supervisor stdio adapter: interactive tty detected; adapter disabled by config, PTY remains active");
        return;
    }

    if interactive_tty {
        info!("supervisor stdio adapter: interactive management forced by config");
    } else {
        info!("supervisor stdio adapter: non-interactive stdio detected; enabling management adapter");
    }

    set_stdio_control_active(true);
    info!("supervisor stdio adapter: connected; PTY channel disabled, virtual /chat enabled");

    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();

        loop {
            if interactive_tty {
                print!("# ");
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
            }

            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    info!("supervisor stdio adapter shutting down");
                    break;
                }

                line = lines.next_line() => {
                    let input = match line {
                        Ok(Some(l)) => l,
                        Ok(None) => {
                            info!("supervisor stdio adapter stdin closed");
                            break;
                        }
                        Err(e) => {
                            warn!("supervisor stdio adapter read error: {e}");
                            break;
                        }
                    };

                    match parse_tty_protocol(&input) {
                        Ok(None) => {}
                        Ok(Some(StdioFrame::Help)) => print_usage(),
                        Ok(Some(StdioFrame::Chat { content })) => {
                            match bus
                                .request(
                                    "agents",
                                    BusPayload::CommsMessage {
                                        channel_id: VIRTUAL_PTY_CHANNEL_ID.to_string(),
                                        content,
                                        session_id: None,
                                        usage: None,
                                    },
                                )
                                .await
                            {
                                Ok(Ok(BusPayload::CommsMessage { content, .. })) => println!("{content}"),
                                Ok(Ok(other)) => println!("{other:?}"),
                                Ok(Err(err)) => eprintln!("chat error: {} ({})", err.message, err.code),
                                Err(err) => eprintln!("chat transport error: {err}"),
                            }
                        }
                        Ok(Some(StdioFrame::AgentHealth { agent_id })) => {
                            match bus
                                .request(
                                    format!("agents/{agent_id}/health"),
                                    BusPayload::CommsMessage {
                                        channel_id: VIRTUAL_PTY_CHANNEL_ID.to_string(),
                                        content: "health".to_string(),
                                        session_id: None,
                                        usage: None,
                                    },
                                )
                                .await
                            {
                                Ok(Ok(BusPayload::CommsMessage { content, .. })) => println!("{content}"),
                                Ok(Ok(other)) => println!("{other:?}"),
                                Ok(Err(err)) => eprintln!("agent health error: {} ({})", err.message, err.code),
                                Err(err) => eprintln!("agent health transport error: {err}"),
                            }
                        }
                        Ok(Some(StdioFrame::Control(command))) => {
                            match control.request(command).await {
                                Ok(Ok(response)) => print_control_response(response),
                                Ok(Err(err)) => eprintln!("control error: {err:?}"),
                                Err(err) => eprintln!("control transport error: {err}"),
                            }
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            print_usage();
                        }
                    }
                }
            }
        }

        set_stdio_control_active(false);
    });
}

#[derive(Debug)]
enum StdioFrame {
    Chat { content: String },
    AgentHealth { agent_id: String },
    Control(ControlCommand),
    Help,
}

fn parse_tty_protocol(line: &str) -> Result<Option<StdioFrame>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let first_non_ws = line
        .char_indices()
        .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some((idx, ch)));

    let Some((start, ch)) = first_non_ws else {
        return Ok(None);
    };

    if ch != '/' {
        return Err("expected slash command (first non-whitespace character must be '/')".to_string());
    }

    let cmdline = &line[start + ch.len_utf8()..];
    let mut parts = cmdline.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default().trim();
    let rest = parts.next().unwrap_or_default().trim();

    match command {
        "chat" => {
            if rest.is_empty() {
                Err("usage: /chat <message>".to_string())
            } else {
                Ok(Some(StdioFrame::Chat {
                    content: rest.to_string(),
                }))
            }
        }
        "health" => {
            if rest.is_empty() {
                Ok(Some(StdioFrame::Control(ControlCommand::Health)))
            } else {
                Ok(Some(StdioFrame::AgentHealth {
                    agent_id: rest.to_string(),
                }))
            }
        }
        "status" => ensure_no_args(rest, StdioFrame::Control(ControlCommand::Status)),
        "subsys" => ensure_no_args(rest, StdioFrame::Control(ControlCommand::SubsystemsList)),
        "exit" => ensure_no_args(rest, StdioFrame::Control(ControlCommand::Shutdown)),
        "help" => ensure_no_args(rest, StdioFrame::Help),
        "" => Err("usage: /<command> [args]".to_string()),
        other => Err(format!("unknown command: /{other}")),
    }
}

fn ensure_no_args(rest: &str, frame: StdioFrame) -> Result<Option<StdioFrame>, String> {
    if rest.is_empty() {
        Ok(Some(frame))
    } else {
        Err("unexpected arguments".to_string())
    }
}

fn print_usage() {
    eprintln!("commands:");
    eprintln!("  /chat <message>");
    eprintln!("  /health [agent]");
    eprintln!("  /status");
    eprintln!("  /subsys");
    eprintln!("  /exit");
    eprintln!("  /help");
}

fn print_control_response(response: ControlResponse) {
    match response {
        ControlResponse::Health { uptime_ms } => {
            println!("health: ok (uptime_ms={uptime_ms})");
        }
        ControlResponse::Status {
            uptime_ms,
            handlers,
        } => {
            println!("status: uptime_ms={uptime_ms} handlers={handlers:?}");
        }
        ControlResponse::Subsystems { handlers } => {
            println!("subsystems: {handlers:?}");
        }
        ControlResponse::Ack { message } => {
            println!("ok: {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StdioFrame, parse_tty_protocol};

    #[test]
    fn parse_chat_command() {
        match parse_tty_protocol("/chat hello world") {
            Ok(Some(StdioFrame::Chat { content })) => assert_eq!(content, "hello world"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_requires_slash_prefix() {
        let err = parse_tty_protocol("hello").expect_err("non-command input should fail");
        assert!(err.contains("first non-whitespace"));
    }

    #[test]
    fn parse_allows_leading_whitespace_before_slash() {
        match parse_tty_protocol("   /chat hi") {
            Ok(Some(StdioFrame::Chat { content })) => assert_eq!(content, "hi"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_health_news_command() {
        match parse_tty_protocol("/health news") {
            Ok(Some(StdioFrame::AgentHealth { agent_id })) => assert_eq!(agent_id, "news"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_plain_health_command() {
        match parse_tty_protocol("/health") {
            Ok(Some(StdioFrame::Control(super::ControlCommand::Health))) => {}
            other => panic!("unexpected parse result: {other:?}"),
        }
    }
}
