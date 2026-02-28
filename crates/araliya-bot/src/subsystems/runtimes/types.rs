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
    /// Runtime binary name (e.g. `"node"`, `"python3"`, `"bash"`).
    pub runtime: String,

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
