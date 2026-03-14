//! Runtimes subsystem — execute scripts in external runtimes (node, python3, etc.).
//!
//! ## Bus API
//!
//! | Method             | Description                                      |
//! |--------------------|--------------------------------------------------|
//! | `runtimes/init`    | Bootstrap a runtime env. Payload: [`RuntimeInitRequest`] |
//! | `runtimes/exec`    | Execute a script. Payload: [`RuntimeExecRequest`] |
//! | `runtimes/status`  | Subsystem health / status check                  |
//!
//! ## On-Disk Layout
//!
//! ```text
//! {identity_dir}/runtimes/{runtime_name}/   ← working directory per runtime
//! ```
//!
//! Directories are created lazily on first execution.

mod types;

pub use types::{RuntimeExecRequest, RuntimeExecResult, RuntimeInitRequest, RuntimeInitResult};

use std::path::PathBuf;
use std::time::Instant;

use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::config::RuntimesConfig;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use crate::supervisor::component_info::{ComponentInfo, ComponentStatusResponse};
use crate::supervisor::dispatch::BusHandler;
use crate::supervisor::health::HealthReporter;

/// Subsystem that spawns external runtime processes to execute scripts.
///
/// Implements [`BusHandler`] directly — no internal trait hierarchy.
/// A `Runtime` trait or per-runtime structs can be introduced later when
/// a second runtime needs meaningfully different behavior.
pub struct RuntimesSubsystem {
    /// Root directory for per-runtime working directories:
    /// `{identity_dir}/runtimes/`.
    runtimes_root: PathBuf,
    /// Default timeout applied when the request does not specify one.
    default_timeout_secs: u64,
    reporter: Option<HealthReporter>,
}

impl RuntimesSubsystem {
    /// Create a new runtimes subsystem.
    ///
    /// `identity_dir` is the bot's persistent identity directory (e.g.
    /// `~/.araliya/bot-pkey.../`).  The runtimes root is
    /// `{identity_dir}/runtimes/`.
    pub fn new(identity_dir: &std::path::Path, config: &RuntimesConfig) -> Self {
        Self {
            runtimes_root: identity_dir.join("runtimes"),
            default_timeout_secs: config.default_timeout_secs,
            reporter: None,
        }
    }

    /// Attach a health reporter.  Reports healthy at startup.
    pub fn with_health_reporter(mut self, reporter: HealthReporter) -> Self {
        let r = reporter.clone();
        tokio::spawn(async move { r.set_healthy().await });
        self.reporter = Some(reporter);
        self
    }

    /// Initialize a runtime environment.
    ///
    /// Creates the per-runtime working directory and optionally runs a setup
    /// script (via `bash -c`) inside it.
    async fn init(
        runtimes_root: PathBuf,
        default_timeout: u64,
        req: RuntimeInitRequest,
    ) -> Result<RuntimeInitResult, String> {
        let runtime_dir = runtimes_root.join(&req.runtime);
        tokio::fs::create_dir_all(&runtime_dir)
            .await
            .map_err(|e| format!("failed to create runtime dir: {e}"))?;

        let runtime_dir_str = runtime_dir
            .to_str()
            .ok_or_else(|| "runtime dir path is not valid UTF-8".to_string())?
            .to_string();

        let Some(script) = req.setup_script else {
            return Ok(RuntimeInitResult {
                success: true,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                runtime_dir: runtime_dir_str,
            });
        };

        let timeout_secs = req.timeout_secs.unwrap_or(default_timeout);

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c")
            .arg(&script)
            .current_dir(&runtime_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (k, v) in &req.env {
            cmd.env(k, v);
        }

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code();
                Ok(RuntimeInitResult {
                    success: output.status.success(),
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    runtime_dir: runtime_dir_str,
                })
            }
            Ok(Err(e)) => Err(format!("setup script spawn failed: {e}")),
            Err(_) => Err(format!("setup script timed out after {timeout_secs}s")),
        }
    }

    /// Execute a script in the given runtime.
    ///
    /// 1. Ensures the per-runtime working directory exists.
    /// 2. If `source` is provided, writes it to a temp file inside the
    ///    working directory.
    /// 3. Spawns `tokio::process::Command::new(runtime)` with the script path.
    /// 4. Applies the configured timeout via `tokio::time::timeout`.
    /// 5. Captures stdout/stderr and cleans up the temp file.
    async fn exec(
        runtimes_root: PathBuf,
        default_timeout: u64,
        req: RuntimeExecRequest,
    ) -> Result<RuntimeExecResult, String> {
        let runtime_dir = runtimes_root.join(&req.runtime);
        tokio::fs::create_dir_all(&runtime_dir)
            .await
            .map_err(|e| format!("failed to create runtime dir: {e}"))?;

        let command = req.command.as_deref().unwrap_or("bash");

        // Determine script path — either caller-provided or temp file from inline source.
        let (script_path, temp_file) = match (&req.source, &req.script_path) {
            (Some(source), _) => {
                let ext = match command {
                    "node" => "js",
                    "python3" | "python" => "py",
                    "bash" | "sh" => "sh",
                    "ruby" => "rb",
                    _ => "tmp",
                };
                let filename = format!("_exec_{}.{}", uuid::Uuid::new_v4(), ext);
                let path = runtime_dir.join(&filename);
                tokio::fs::write(&path, source)
                    .await
                    .map_err(|e| format!("failed to write temp script: {e}"))?;
                (path.clone(), Some(path))
            }
            (None, Some(path)) => (PathBuf::from(path), None),
            (None, None) => {
                return Err("either `source` or `script_path` must be provided".to_string());
            }
        };

        let timeout_secs = req.timeout_secs.unwrap_or(default_timeout);

        let start = Instant::now();

        let mut cmd = tokio::process::Command::new(command);
        cmd.arg(&script_path)
            .current_dir(&runtime_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (k, v) in &req.env {
            cmd.env(k, v);
        }

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output()).await;

        // Clean up temp file regardless of outcome.
        if let Some(ref temp) = temp_file {
            let _ = tokio::fs::remove_file(temp).await;
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code();
                Ok(RuntimeExecResult {
                    success: output.status.success(),
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    duration_ms,
                })
            }
            Ok(Err(e)) => Err(format!("process spawn failed: {e}")),
            Err(_) => Err(format!("execution timed out after {timeout_secs}s")),
        }
    }
}

impl BusHandler for RuntimesSubsystem {
    fn prefix(&self) -> &str {
        "runtimes"
    }

    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        // ── runtimes/status ──────────────────────────────────────────────
        if method == "runtimes/status" {
            let reporter = self.reporter.clone();
            tokio::spawn(async move {
                let resp = match reporter {
                    Some(r) => match r.get_current().await {
                        Some(h) if h.healthy => ComponentStatusResponse::running("runtimes"),
                        Some(h) => ComponentStatusResponse::error("runtimes", h.message),
                        None => ComponentStatusResponse::running("runtimes"),
                    },
                    None => ComponentStatusResponse::running("runtimes"),
                };
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: resp.to_json(),
                }));
            });
            return;
        }

        // ── runtimes/init ────────────────────────────────────────────────
        if method == "runtimes/init" {
            let runtimes_root = self.runtimes_root.clone();
            let default_timeout = self.default_timeout_secs;

            let json_str = match &payload {
                BusPayload::JsonResponse { data } => Some(data.clone()),
                BusPayload::ToolRequest { args_json, .. } => Some(args_json.clone()),
                _ => None,
            };

            let Some(json_str) = json_str else {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected JsonResponse or ToolRequest payload with init request",
                )));
                return;
            };

            let req: RuntimeInitRequest = match serde_json::from_str(&json_str) {
                Ok(r) => r,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32602,
                        format!("invalid init request: {e}"),
                    )));
                    return;
                }
            };

            debug!(runtime = %req.runtime, "runtimes/init dispatched");

            tokio::spawn(async move {
                match Self::init(runtimes_root, default_timeout, req).await {
                    Ok(result) => {
                        let data = serde_json::to_string(&result).unwrap_or_default();
                        let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                    }
                    Err(e) => {
                        warn!(error = %e, "runtimes/init failed");
                        let _ = reply_tx.send(Err(BusError::new(-32000, e)));
                    }
                }
            });
            return;
        }

        // ── runtimes/exec ────────────────────────────────────────────────
        if method == "runtimes/exec" {
            let runtimes_root = self.runtimes_root.clone();
            let default_timeout = self.default_timeout_secs;

            // Accept either a JsonResponse (raw JSON string) or ToolRequest with args_json.
            let json_str = match &payload {
                BusPayload::JsonResponse { data } => Some(data.clone()),
                BusPayload::ToolRequest { args_json, .. } => Some(args_json.clone()),
                _ => None,
            };

            let Some(json_str) = json_str else {
                let _ = reply_tx.send(Err(BusError::new(
                    -32600,
                    "expected JsonResponse or ToolRequest payload with exec request",
                )));
                return;
            };

            let req: RuntimeExecRequest = match serde_json::from_str(&json_str) {
                Ok(r) => r,
                Err(e) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32602,
                        format!("invalid exec request: {e}"),
                    )));
                    return;
                }
            };

            debug!(runtime = %req.runtime, "runtimes/exec dispatched");

            tokio::spawn(async move {
                match Self::exec(runtimes_root, default_timeout, req).await {
                    Ok(result) => {
                        let data = serde_json::to_string(&result).unwrap_or_default();
                        let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
                    }
                    Err(e) => {
                        warn!(error = %e, "runtimes/exec failed");
                        let _ = reply_tx.send(Err(BusError::new(-32000, e)));
                    }
                }
            });
            return;
        }

        let _ = reply_tx.send(Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )));
    }

    fn component_info(&self) -> ComponentInfo {
        ComponentInfo::running("runtimes", "Runtimes", vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `init` with a setup script creates the directory and runs the script.
    #[tokio::test]
    async fn init_with_setup_script() {
        let tmp = tempfile::TempDir::new().unwrap();
        let req = RuntimeInitRequest {
            runtime: "test-env".to_string(),
            setup_script: Some("echo initialized".to_string()),
            env: Default::default(),
            timeout_secs: Some(10),
        };

        let result = RuntimesSubsystem::init(tmp.path().to_path_buf(), 30, req)
            .await
            .expect("init should succeed");

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), "initialized");
        assert!(tmp.path().join("test-env").is_dir());
        assert_eq!(
            result.runtime_dir,
            tmp.path().join("test-env").to_str().unwrap()
        );
    }

    /// Verify that `init` without a setup script just creates the directory.
    #[tokio::test]
    async fn init_no_script() {
        let tmp = tempfile::TempDir::new().unwrap();
        let req = RuntimeInitRequest {
            runtime: "bare-env".to_string(),
            setup_script: None,
            env: Default::default(),
            timeout_secs: None,
        };

        let result = RuntimesSubsystem::init(tmp.path().to_path_buf(), 30, req)
            .await
            .expect("init should succeed");

        assert!(result.success);
        assert!(result.exit_code.is_none());
        assert!(result.stdout.is_empty());
        assert!(tmp.path().join("bare-env").is_dir());
    }

    /// Verify that an inline `console.log('ok')` script executes successfully
    /// and produces the expected stdout.
    #[tokio::test]
    async fn exec_node_inline() {
        // Skip if node is not available in the test environment.
        let node_check = tokio::process::Command::new("node")
            .arg("--version")
            .output()
            .await;
        if node_check.is_err() || !node_check.unwrap().status.success() {
            eprintln!("skipping test: node not found");
            return;
        }

        let tmp = tempfile::TempDir::new().unwrap();
        let req = RuntimeExecRequest {
            runtime: "node".to_string(),
            command: Some("node".into()),
            source: Some("console.log('ok')".to_string()),
            script_path: None,
            env: Default::default(),
            timeout_secs: Some(10),
        };

        let result = RuntimesSubsystem::exec(tmp.path().to_path_buf(), 30, req)
            .await
            .expect("exec should succeed");

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), "ok");
        assert!(result.stderr.is_empty() || result.stderr.trim().is_empty());
    }

    /// Verify that a missing runtime produces an error (not a panic).
    #[tokio::test]
    async fn exec_missing_runtime() {
        let tmp = tempfile::TempDir::new().unwrap();
        let req = RuntimeExecRequest {
            runtime: "nonexistent_runtime_xyz".to_string(),
            command: Some("nonexistent_runtime_xyz".into()),
            source: Some("hello".to_string()),
            script_path: None,
            env: Default::default(),
            timeout_secs: Some(5),
        };

        let result = RuntimesSubsystem::exec(tmp.path().to_path_buf(), 30, req).await;
        assert!(result.is_err());
    }

    /// Verify that neither source nor script_path is rejected.
    #[tokio::test]
    async fn exec_no_source_no_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let req = RuntimeExecRequest {
            runtime: "node".to_string(),
            command: None,
            source: None,
            script_path: None,
            env: Default::default(),
            timeout_secs: Some(5),
        };

        let result = RuntimesSubsystem::exec(tmp.path().to_path_buf(), 30, req).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("source"));
    }
}
