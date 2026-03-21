//! Async tool helpers for the webbuilder agent.
//!
//! All functions interact with the runtimes subsystem (via bus) or the
//! local filesystem directly.  They are plain async fns — no `LocalTool`
//! wrapper is needed because the webbuilder loop is itself async.

use araliya_core::bus::message::BusPayload;

use super::super::AgentsState;

// ── ExecResult ────────────────────────────────────────────────────────────────

/// Captured output from a shell command executed in the workspace.
pub(crate) struct ExecResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

// ── Workspace init ────────────────────────────────────────────────────────────

/// Initialise the runtime workspace via `runtimes/init`.
///
/// Returns the absolute path to the workspace directory on success.
pub(crate) async fn init_workspace(
    state: &AgentsState,
    channel_id: &str,
    runtime_name: &str,
    setup_script: Option<&str>,
    timeout_secs: u64,
) -> Result<String, String> {
    let _ = channel_id; // reserved for future use
    let req = serde_json::json!({
        "runtime": runtime_name,
        "setup_script": setup_script,
        "timeout_secs": timeout_secs,
    })
    .to_string();

    match state.runtime_init(req).await {
        Ok(BusPayload::JsonResponse { data }) => {
            let v: serde_json::Value =
                serde_json::from_str(&data).map_err(|e| e.to_string())?;
            let runtime_dir = v["runtime_dir"]
                .as_str()
                .ok_or_else(|| "runtimes/init: missing runtime_dir in response".to_string())?
                .to_string();
            Ok(runtime_dir)
        }
        Ok(other) => Err(format!("runtimes/init: unexpected payload: {other:?}")),
        Err(e) => Err(format!("runtimes/init failed: {}", e.message)),
    }
}

// ── Command execution ─────────────────────────────────────────────────────────

/// Execute a shell command in the workspace via `runtimes/exec`.
///
/// The `source` string is written to a temp `.sh` file and executed with
/// `bash`, with the working directory set to the runtime workspace.
pub(crate) async fn exec_cmd(
    state: &AgentsState,
    channel_id: &str,
    runtime_name: &str,
    source: &str,
    timeout_secs: u64,
) -> Result<ExecResult, String> {
    let _ = channel_id;
    let req = serde_json::json!({
        "runtime": runtime_name,
        "command": "bash",
        "source": source,
        "timeout_secs": timeout_secs,
    })
    .to_string();

    match state.runtime_exec(req).await {
        Ok(BusPayload::JsonResponse { data }) => {
            let v: serde_json::Value =
                serde_json::from_str(&data).map_err(|e| e.to_string())?;
            Ok(ExecResult {
                success: v["success"].as_bool().unwrap_or(false),
                stdout: v["stdout"].as_str().unwrap_or("").to_string(),
                stderr: v["stderr"].as_str().unwrap_or("").to_string(),
                duration_ms: v["duration_ms"].as_u64().unwrap_or(0),
            })
        }
        Ok(other) => Err(format!("runtimes/exec: unexpected payload: {other:?}")),
        Err(e) => Err(format!("runtimes/exec failed: {}", e.message)),
    }
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/// Write `content` to `{workspace}/{rel_path}`, creating parent directories.
///
/// `rel_path` must stay within `workspace` — path traversal is rejected.
pub(crate) async fn write_file(
    workspace: &str,
    rel_path: &str,
    content: &str,
) -> Result<(), String> {
    let workspace_path = std::path::Path::new(workspace);
    let target = workspace_path.join(rel_path);

    // Reject path traversal attempts.
    let canonical_workspace = tokio::fs::canonicalize(workspace_path)
        .await
        .unwrap_or_else(|_| workspace_path.to_path_buf());
    let normalized = target
        .components()
        .fold(std::path::PathBuf::new(), |mut acc, c| {
            match c {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                std::path::Component::CurDir => {}
                other => acc.push(other),
            }
            acc
        });
    if !normalized.starts_with(&canonical_workspace) && !normalized.starts_with(workspace_path) {
        return Err(format!("path traversal rejected: {rel_path}"));
    }

    // Create parent directories.
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }

    tokio::fs::write(&target, content)
        .await
        .map_err(|e| format!("write {}: {e}", target.display()))
}

/// Read a file from `{workspace}/{rel_path}`.  Returns an error if missing
/// or if the file exceeds `max_bytes`.
#[allow(dead_code)]
pub(crate) async fn read_file(
    workspace: &str,
    rel_path: &str,
    max_bytes: usize,
) -> Result<String, String> {
    let path = std::path::Path::new(workspace).join(rel_path);
    let data = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read {rel_path}: {e}"))?;
    if data.len() > max_bytes {
        return Err(format!(
            "{rel_path} is too large ({} bytes > {max_bytes} limit)",
            data.len()
        ));
    }
    String::from_utf8(data).map_err(|e| format!("read {rel_path}: not UTF-8: {e}"))
}

// ── File tree ─────────────────────────────────────────────────────────────────

/// Return a shallow list of files/directories in the workspace.
///
/// Skips `node_modules`, `dist`, `.git`, and dotfiles.
/// Recurses into `src/` one level deep.
pub(crate) async fn list_files(workspace: &str) -> Vec<String> {
    let mut result = Vec::new();
    collect_entries(workspace, workspace, 0, &mut result).await;
    result
}

#[allow(clippy::manual_async_fn)]
fn collect_entries<'a>(
    base: &'a str,
    dir: &'a str,
    depth: usize,
    result: &'a mut Vec<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if depth > 3 {
            return;
        }
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // Skip hidden, node_modules, dist, lock files.
            if name.starts_with('.')
                || name == "node_modules"
                || name == "dist"
                || name.ends_with(".lock")
            {
                continue;
            }

            let rel = path
                .strip_prefix(base)
                .map(|p| p.to_str().unwrap_or("").to_string())
                .unwrap_or_else(|_| name.clone());

            if path.is_dir() {
                result.push(format!("{rel}/"));
                collect_entries(base, path.to_str().unwrap_or(dir), depth + 1, result).await;
            } else {
                result.push(rel);
            }
        }
    })
}
