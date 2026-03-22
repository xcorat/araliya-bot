//! Request and response types for the runtimes subsystem.
//!
//! These types are serialized/deserialized as JSON when crossing the bus
//! boundary via `BusPayload::JsonResponse`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Request payload for `runtimes/exec`.
///
/// Callers must provide either `source` (inline script) or `script_path`
/// (path to an existing file).  When `source` is set the subsystem writes
/// it to a temporary file, executes it, and cleans up afterwards.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeExecRequest {
    /// Runtime environment name — used as the working directory under
    /// `{identity_dir}/runtimes/{runtime}/`.
    pub runtime: String,

    /// Interpreter binary (e.g. `node`, `python3`). Defaults to `bash`.
    #[serde(default)]
    pub command: Option<String>,

    /// Inline script source code.  Mutually exclusive with `script_path`.
    #[serde(default)]
    pub source: Option<String>,

    /// Path to an existing script file on disk.
    #[serde(default)]
    pub script_path: Option<String>,

    /// Extra environment variables passed to the child process.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Per-execution timeout in seconds.  Falls back to the subsystem
    /// default (`[runtimes] default_timeout_secs`) when `None`.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Request payload for `runtimes/init`.
///
/// Bootstraps a runtime environment by creating its working directory and
/// optionally running a setup script (e.g. `npm init -y`, `python3 -m venv .venv`).
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeInitRequest {
    /// Runtime environment name — used as the working directory name under
    /// `{identity_dir}/runtimes/{runtime}/`.
    pub runtime: String,

    /// Optional shell script to run inside the new directory after creation.
    /// Executed with `bash -c`.
    #[serde(default)]
    pub setup_script: Option<String>,

    /// Extra environment variables passed to the setup script.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Timeout for the setup script in seconds.  Falls back to the subsystem
    /// default when `None`.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Result payload returned from `runtimes/init`.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInitResult {
    /// `true` when the directory was created (and setup script exited 0, if any).
    pub success: bool,

    /// Exit code of the setup script, or `None` if no script was provided.
    pub exit_code: Option<i32>,

    /// Captured stdout from the setup script (empty if no script).
    pub stdout: String,

    /// Captured stderr from the setup script (empty if no script).
    pub stderr: String,

    /// Absolute path to the runtime directory.
    pub runtime_dir: String,
}

/// Result payload returned from `runtimes/exec`.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeExecResult {
    /// `true` when the process exited with code 0.
    pub success: bool,

    /// Raw exit code, or `None` if the process was killed by a signal.
    pub exit_code: Option<i32>,

    /// Captured stdout (UTF-8 lossy).
    pub stdout: String,

    /// Captured stderr (UTF-8 lossy).
    pub stderr: String,

    /// Wall-clock execution time in milliseconds.
    pub duration_ms: u64,
}
