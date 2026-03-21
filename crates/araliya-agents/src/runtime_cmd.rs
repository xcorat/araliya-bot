//! `runtime_cmd` agent — REPL-style passthrough to an external runtime.
//!
//! All user messages are sent as source code to `runtimes/exec`.  No LLM
//! is involved — this is a pure command passthrough agent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::oneshot;
use tracing::{debug, warn};

use araliya_core::bus::message::{BusPayload, BusResult};
use araliya_core::config::RuntimeCmdAgentConfig;

use super::{Agent, AgentsState};

pub(crate) struct RuntimeCmdPlugin {
    runtime: String,
    command: String,
    setup_script: Option<String>,
    initialized: Arc<AtomicBool>,
}

impl RuntimeCmdPlugin {
    pub fn new(cfg: &RuntimeCmdAgentConfig) -> Self {
        Self {
            runtime: cfg.runtime.clone(),
            command: cfg.command.clone(),
            setup_script: cfg.setup_script.clone(),
            initialized: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Agent for RuntimeCmdPlugin {
    fn id(&self) -> &str {
        "runtime_cmd"
    }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let runtime = self.runtime.clone();
        let command = self.command.clone();
        let setup_script = self.setup_script.clone();
        let initialized = self.initialized.clone();

        tokio::spawn(async move {
            // ── Init on first use ───────────────────────────────────
            if !initialized.load(Ordering::Acquire) {
                debug!(runtime = %runtime, "runtime_cmd: initializing runtime");
                let init_json = serde_json::json!({
                    "runtime": runtime,
                    "setup_script": setup_script,
                })
                .to_string();

                match state.runtime_init(init_json).await {
                    Ok(_) => {
                        initialized.store(true, Ordering::Release);
                        debug!(runtime = %runtime, "runtime_cmd: runtime initialized");
                    }
                    Err(e) => {
                        warn!(runtime = %runtime, error = ?e, "runtime_cmd: init failed");
                        let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                            channel_id,
                            content: format!("Runtime init failed: {e:?}"),
                            session_id,
                            usage: None,
                            timing: None,
                            thinking: None,
                        }));
                        return;
                    }
                }
            }

            // ── Execute user input as source code ───────────────────
            let exec_json = serde_json::json!({
                "runtime": runtime,
                "command": command,
                "source": content,
            })
            .to_string();

            match state.runtime_exec(exec_json).await {
                Ok(BusPayload::JsonResponse { data }) => {
                    let reply = format_exec_result(&data);
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: reply,
                        session_id,
                        usage: None,
                        timing: None,
                        thinking: None,
                    }));
                }
                Ok(other) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("Unexpected response: {other:?}"),
                        session_id,
                        usage: None,
                        timing: None,
                        thinking: None,
                    }));
                }
                Err(e) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: format!("Execution error: {e:?}"),
                        session_id,
                        usage: None,
                        timing: None,
                        thinking: None,
                    }));
                }
            }
        });
    }
}

/// Format a `RuntimeExecResult` JSON string into a human-readable reply.
fn format_exec_result(json: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return json.to_string(),
    };

    let success = v.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let stdout = v.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let stderr = v.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
    let exit_code = v.get("exit_code").and_then(|v| v.as_i64());

    if success {
        let out = stdout.trim();
        if out.is_empty() {
            "(ok, no output)".to_string()
        } else {
            out.to_string()
        }
    } else {
        let code_str = exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        let err = stderr.trim();
        if err.is_empty() {
            format!("Error (exit {code_str})")
        } else {
            format!("Error (exit {code_str}):\n{err}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_success_with_output() {
        let json = serde_json::json!({
            "success": true,
            "exit_code": 0,
            "stdout": "hello world\n",
            "stderr": "",
            "duration_ms": 42,
        })
        .to_string();
        assert_eq!(format_exec_result(&json), "hello world");
    }

    #[test]
    fn format_success_no_output() {
        let json = serde_json::json!({
            "success": true,
            "exit_code": 0,
            "stdout": "",
            "stderr": "",
            "duration_ms": 10,
        })
        .to_string();
        assert_eq!(format_exec_result(&json), "(ok, no output)");
    }

    #[test]
    fn format_failure() {
        let json = serde_json::json!({
            "success": false,
            "exit_code": 1,
            "stdout": "",
            "stderr": "command not found\n",
            "duration_ms": 5,
        })
        .to_string();
        assert_eq!(
            format_exec_result(&json),
            "Error (exit 1):\ncommand not found"
        );
    }
}
