//! Iterative web-building loop for the `webbuilder` agent.
//!
//! [`WebBuilderLoop::run_stream`] returns a [`BusPayload::LlmStreamResult`]
//! immediately; the actual work happens in a background task that emits
//! [`StreamChunk::Content`] events for each step and terminates with
//! [`StreamChunk::Done`].
//!
//! ## Event protocol
//!
//! Steps are emitted as lines prefixed with `>>STEP<<` followed by a JSON
//! object, so the frontend can distinguish structured events from plain LLM
//! text:
//!
//! ```text
//! >>STEP<<{"type":"init","message":"Scaffolding workspace..."}
//! >>STEP<<{"type":"file_write","path":"src/App.svelte"}
//! >>STEP<<{"type":"run_cmd","cmd":"npm run build"}
//! >>STEP<<{"type":"cmd_result","ok":true,"stdout":"...","stderr":""}
//! >>STEP<<{"type":"done","preview_url":"/preview/webbuilder-abc12345/"}
//! >>STEP<<{"type":"error","message":"..."}
//! ```

use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::warn;

use crate::llm::StreamChunk;
use crate::supervisor::bus::{BusPayload, BusResult, StreamReceiver};

use super::super::AgentsState;
use super::tools;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Vite + Svelte 5 scaffold: creates a minimal project that `npm run build`
/// will successfully compile.  `npm install` is run as the last step so the
/// LLM can modify the scaffold before the first build.
const SCAFFOLD_VITE_SVELTE: &str = r#"
cat > package.json << 'PKGJSON'
{
  "name": "webbuilder-page",
  "version": "1.0.0",
  "private": true,
  "scripts": { "build": "vite build", "dev": "vite" },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "svelte": "^5.0.0",
    "vite": "^6.0.0"
  }
}
PKGJSON

cat > vite.config.js << 'VITECFG'
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
export default defineConfig({
  plugins: [svelte()],
  build: { outDir: 'dist' }
})
VITECFG

mkdir -p src

cat > index.html << 'IDXHTML'
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Svelte App</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js"></script>
  </body>
</html>
IDXHTML

cat > src/main.js << 'MAINJS'
import { mount } from 'svelte'
import App from './App.svelte'
const app = mount(App, { target: document.getElementById('app') })
export default app
MAINJS

cat > src/App.svelte << 'APPSV'
<script>
  let message = $state('Hello, World!');
</script>
<main>
  <h1>{message}</h1>
</main>
<style>
  main { text-align: center; padding: 2rem; font-family: sans-serif; }
</style>
APPSV

npm install
"#;

/// Timeout (seconds) for the scaffold `npm install` step.
const SCAFFOLD_TIMEOUT_SECS: u64 = 180;
/// Timeout for normal command execution (build, etc.).
const CMD_TIMEOUT_SECS: u64 = 60;
/// Maximum file size for reading workspace files back.
#[allow(dead_code)]
const MAX_FILE_READ_BYTES: usize = 32 * 1024;

// ── Command types ─────────────────────────────────────────────────────────────

/// A command returned by the LLM in its JSON response.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WbCommand {
    WriteFile {
        path: String,
        content: String,
    },
    RunCmd {
        command: String,
    },
    Finish {
        #[serde(default)]
        message: String,
    },
}

/// Top-level wrapper for the LLM's JSON response.
#[derive(Debug, Deserialize)]
struct LlmResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    commands: Vec<WbCommand>,
    #[serde(default)]
    finish: bool,
}

// ── WebBuilderLoop ────────────────────────────────────────────────────────────

pub(crate) struct WebBuilderLoop {
    pub max_iterations: usize,
}

impl WebBuilderLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self { max_iterations }
    }

    /// Kick off the build loop asynchronously and return a streaming result
    /// that emits progress events as [`StreamChunk::Content`] items.
    pub async fn run_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        state: Arc<AgentsState>,
    ) -> BusResult {
        let (tx, rx) = mpsc::channel::<StreamChunk>(128);
        let max_iters = self.max_iterations;

        tokio::spawn(async move {
            run_loop(channel_id, content, session_id, max_iters, state, tx).await;
        });

        Ok(BusPayload::LlmStreamResult {
            rx: StreamReceiver(rx),
        })
    }
}

// ── Main loop ─────────────────────────────────────────────────────────────────

/// Helper: send a `>>STEP<<{...}` event to the client.
async fn emit_step(tx: &mpsc::Sender<StreamChunk>, json: serde_json::Value) {
    let line = format!(">>STEP<<{}\n", json);
    let _ = tx.send(StreamChunk::Content(line)).await;
}

/// Helper: extract text from a `BusPayload::CommsMessage` result.
fn extract_text(result: BusResult) -> Option<String> {
    match result {
        Ok(BusPayload::CommsMessage { content, .. }) => Some(content),
        _ => None,
    }
}

/// Helper: strip markdown code fences and parse JSON from LLM output.
fn parse_llm_response(text: &str) -> Option<LlmResponse> {
    let stripped = text.trim();
    let json_text = if let Some(inner) = stripped
        .strip_prefix("```json")
        .or_else(|| stripped.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim()
    } else {
        stripped
    };

    // Try to find a JSON object in the text.
    if let Some(start) = json_text.find('{') {
        if let Some(end) = json_text.rfind('}') {
            let slice = &json_text[start..=end];
            if let Ok(resp) = serde_json::from_str::<LlmResponse>(slice) {
                return Some(resp);
            }
        }
    }
    None
}

/// Build the system prompt for the webbuilder agent.
fn system_prompt() -> String {
    r#"You are a Svelte web page builder agent. You work iteratively to build and improve static Svelte 5 + Vite pages.

You have a pre-scaffolded Vite + Svelte 5 project. You can write files and run shell commands to build the page.

IMPORTANT: Always respond with a JSON object in this EXACT format (no other text outside the JSON):

{
  "message": "Brief description of what you are doing",
  "commands": [
    {"type": "write_file", "path": "relative/path/to/file", "content": "file content here"},
    {"type": "run_cmd", "command": "npm run build"}
  ],
  "finish": false
}

Set "finish": true when the page is fully built and the build succeeded.

Rules:
- Write files using "write_file" commands with the file path relative to the workspace root.
- Run "npm run build" to compile the Svelte project. Check the output for errors.
- If the build fails, fix the errors and rebuild.
- When finish is true, include a final "message" summarising what was built.
- The main component is src/App.svelte. You can create additional .svelte files in src/.
- Use Svelte 5 rune syntax ($state, $derived, $effect).
- Keep the index.html and vite.config.js as-is unless specifically required to change them.
"#.to_string()
}

/// Build the per-turn context prompt.
fn context_prompt(task: &str, file_tree: &[String], iteration: usize, history: &[String]) -> String {
    let tree_str = if file_tree.is_empty() {
        "(workspace is empty)".to_string()
    } else {
        file_tree.join("\n")
    };

    let history_str = if history.is_empty() {
        "(no previous steps)".to_string()
    } else {
        history.join("\n\n---\n\n")
    };

    format!(
        "Task: {task}\n\nIteration: {iteration}\n\nCurrent workspace files:\n{tree_str}\n\nPrevious steps:\n{history_str}\n\nWhat should be done next?"
    )
}

/// The main async loop that does all the work.
async fn run_loop(
    channel_id: String,
    content: String,
    session_id: Option<String>,
    max_iterations: usize,
    state: Arc<AgentsState>,
    tx: mpsc::Sender<StreamChunk>,
) {
    // ── Session ───────────────────────────────────────────────────────────────
    let handle = {
        let agent_store = match state.open_agent_store("webbuilder") {
            Ok(s) => s,
            Err(e) => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": format!("session error: {e}")}),
                )
                .await;
                let _ = tx.send(StreamChunk::Done { usage: None, timing: None }).await;
                return;
            }
        };
        let memory = &state.memory;
        let result = if let Some(ref sid) = session_id {
            memory.load_session_in(
                &agent_store.agent_sessions_dir(),
                &agent_store.agent_sessions_index(),
                sid,
                Some("webbuilder"),
            )
        } else {
            agent_store.get_or_create_session(memory, "webbuilder")
        };
        match result {
            Ok(h) => h,
            Err(e) => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": format!("session init failed: {e}")}),
                )
                .await;
                let _ = tx.send(StreamChunk::Done { usage: None, timing: None }).await;
                return;
            }
        }
    };

    // ── Derive workspace name from session ────────────────────────────────────
    let short_sid = handle
        .session_id
        .replace('-', "")
        .chars()
        .take(8)
        .collect::<String>();
    let runtime_name = format!("webbuilder-{short_sid}");

    // ── Init workspace (idempotent) ───────────────────────────────────────────
    let workspace_ready = handle.kv_get("workspace_ready").await.ok().flatten();
    let workspace_dir = if workspace_ready.as_deref() == Some("true") {
        // Re-derive path from a previous init.
        match handle.kv_get("workspace_dir").await.ok().flatten() {
            Some(d) => d,
            None => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": "workspace_dir missing from session"}),
                )
                .await;
                let _ = tx.send(StreamChunk::Done { usage: None, timing: None }).await;
                return;
            }
        }
    } else {
        emit_step(
            &tx,
            serde_json::json!({"type": "init", "message": "Scaffolding Vite + Svelte 5 workspace (npm install)..."}),
        )
        .await;

        match tools::init_workspace(
            &state,
            &channel_id,
            &runtime_name,
            Some(SCAFFOLD_VITE_SVELTE),
            SCAFFOLD_TIMEOUT_SECS,
        )
        .await
        {
            Ok(dir) => {
                if let Err(e) = handle.kv_set("workspace_ready", "true").await {
                    warn!("webbuilder: kv_set workspace_ready failed: {e}");
                }
                if let Err(e) = handle.kv_set("workspace_dir", &dir).await {
                    warn!("webbuilder: kv_set workspace_dir failed: {e}");
                }
                emit_step(
                    &tx,
                    serde_json::json!({"type": "init_done", "message": "Workspace ready", "workspace": &dir}),
                )
                .await;
                dir
            }
            Err(e) => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": format!("workspace init failed: {e}")}),
                )
                .await;
                let _ = tx.send(StreamChunk::Done { usage: None, timing: None }).await;
                return;
            }
        }
    };

    // Append user message to transcript.
    if let Err(e) = handle.transcript_append("user", &content).await {
        warn!("webbuilder: transcript_append(user) failed: {e}");
    }

    // ── Iteration loop ────────────────────────────────────────────────────────
    let system = system_prompt();
    let mut history: Vec<String> = Vec::new();
    let mut finished = false;

    for iteration in 1..=max_iterations {
        emit_step(
            &tx,
            serde_json::json!({"type": "thinking", "iteration": iteration}),
        )
        .await;

        // Get current file tree.
        let file_tree = tools::list_files(&workspace_dir).await;

        // Build context prompt.
        let prompt = context_prompt(&content, &file_tree, iteration, &history);

        // Call LLM (buffered).
        let llm_result = state
            .complete_via_llm_with_system(&channel_id, &prompt, Some(&system))
            .await;

        let response_text = match extract_text(llm_result) {
            Some(t) => t,
            None => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": "LLM call failed"}),
                )
                .await;
                break;
            }
        };

        // Emit the LLM's message as plain content so the user can read the reasoning.
        let parsed = parse_llm_response(&response_text);
        if let Some(ref resp) = parsed {
            if !resp.message.is_empty() {
                let _ = tx
                    .send(StreamChunk::Content(format!("{}\n", resp.message)))
                    .await;
            }
        }

        // Execute commands.
        let mut step_summary = format!("Iteration {iteration}:");
        let commands = parsed.map(|r| {
            let finish = r.finish;
            (r.commands, finish)
        });

        let (cmds, wants_finish) = match commands {
            Some((c, f)) => (c, f),
            None => {
                // Graceful: LLM output wasn't parseable — try to continue.
                step_summary.push_str(" (no commands parsed, retrying)");
                history.push(step_summary);
                continue;
            }
        };

        for cmd in cmds {
            match cmd {
                WbCommand::WriteFile { path, content: file_content } => {
                    emit_step(
                        &tx,
                        serde_json::json!({"type": "file_write", "path": &path}),
                    )
                    .await;
                    match tools::write_file(&workspace_dir, &path, &file_content).await {
                        Ok(()) => {
                            step_summary.push_str(&format!(" wrote {path};"));
                        }
                        Err(e) => {
                            emit_step(
                                &tx,
                                serde_json::json!({"type": "error", "message": format!("write_file {path}: {e}")}),
                            )
                            .await;
                            step_summary.push_str(&format!(" FAILED write {path}: {e};"));
                        }
                    }
                }

                WbCommand::RunCmd { command } => {
                    emit_step(
                        &tx,
                        serde_json::json!({"type": "run_cmd", "cmd": &command}),
                    )
                    .await;
                    match tools::exec_cmd(
                        &state,
                        &channel_id,
                        &runtime_name,
                        &command,
                        CMD_TIMEOUT_SECS,
                    )
                    .await
                    {
                        Ok(result) => {
                            let stdout_snip = result.stdout.chars().take(512).collect::<String>();
                            let stderr_snip = result.stderr.chars().take(512).collect::<String>();
                            emit_step(
                                &tx,
                                serde_json::json!({
                                    "type": "cmd_result",
                                    "ok": result.success,
                                    "stdout": stdout_snip,
                                    "stderr": stderr_snip,
                                    "duration_ms": result.duration_ms,
                                }),
                            )
                            .await;
                            let status = if result.success { "ok" } else { "failed" };
                            step_summary.push_str(&format!(" cmd={command} status={status};"));
                            if !result.stdout.is_empty() {
                                step_summary.push_str(&format!(" stdout={};", result.stdout.trim()));
                            }
                            if !result.stderr.is_empty() {
                                step_summary.push_str(&format!(" stderr={};", result.stderr.trim()));
                            }
                        }
                        Err(e) => {
                            emit_step(
                                &tx,
                                serde_json::json!({"type": "error", "message": format!("run_cmd failed: {e}")}),
                            )
                            .await;
                            step_summary.push_str(&format!(" FAILED cmd={command}: {e};"));
                        }
                    }
                }

                WbCommand::Finish { message } => {
                    let preview_url = format!("/preview/{runtime_name}/");
                    emit_step(
                        &tx,
                        serde_json::json!({
                            "type": "done",
                            "message": message,
                            "preview_url": preview_url,
                            "session_id": handle.session_id,
                        }),
                    )
                    .await;
                    // Emit a human-readable summary too.
                    let summary = if message.is_empty() {
                        format!("Build complete! Preview: {preview_url}\n")
                    } else {
                        format!("{message}\n\nPreview: {preview_url}\n")
                    };
                    let _ = tx.send(StreamChunk::Content(summary)).await;
                    finished = true;
                    break;
                }
            }
        }

        history.push(step_summary);

        if finished || wants_finish {
            if !finished {
                // Agent said finish: true but didn't send a Finish command.
                let preview_url = format!("/preview/{runtime_name}/");
                emit_step(
                    &tx,
                    serde_json::json!({
                        "type": "done",
                        "preview_url": preview_url,
                        "session_id": handle.session_id,
                    }),
                )
                .await;
                let _ = tx
                    .send(StreamChunk::Content(format!(
                        "Build complete! Preview: {preview_url}\n"
                    )))
                    .await;
            }
            break;
        }
    }

    if !finished && history.len() >= max_iterations {
        emit_step(
            &tx,
            serde_json::json!({"type": "error", "message": format!("reached max iterations ({max_iterations}) without finishing")}),
        )
        .await;
    }

    // Persist assistant summary.
    let final_msg = format!(
        "WebBuilder session complete. Workspace: {workspace_dir}. Steps: {}",
        history.len()
    );
    if let Err(e) = handle.transcript_append("assistant", &final_msg).await {
        warn!("webbuilder: transcript_append(assistant) failed: {e}");
    }

    let _ = tx
        .send(StreamChunk::Done {
            usage: None,
            timing: None,
        })
        .await;
}
