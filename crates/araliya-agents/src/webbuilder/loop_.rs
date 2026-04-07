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

use araliya_core::bus::message::{BusPayload, BusResult, StreamReceiver};
use araliya_llm::StreamChunk;

use super::super::AgentsState;
use super::tools;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Vite + Svelte 5 scaffold: creates a minimal project that `npm run build`
/// will successfully compile.  `npm install` is run as the last step so the
/// LLM can modify the scaffold before the first build.
const SCAFFOLD_VITE_SVELTE: &str = r#"
set -e
cat > package.json << 'PKGJSON'
{
  "name": "webbuilder-page",
  "version": "1.0.0",
  "private": true,
  "scripts": { "build": "vite build", "dev": "vite" },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5.0.0",
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

/// Same scaffold as [`SCAFFOLD_VITE_SVELTE`] but builds with `--base ./` so
/// Vite emits relative asset paths (`./assets/...`).  Required for homebuilder
/// because the page is served under `/home/` — relative paths let the browser
/// resolve `./assets/foo.js` → `/home/assets/foo.js` correctly.
#[allow(dead_code)]
const SCAFFOLD_HOMEBUILDER: &str = r#"
set -e
cat > package.json << 'PKGJSON'
{
  "name": "homebuilder-page",
  "version": "1.0.0",
  "private": true,
  "scripts": { "build": "vite build --base ./", "dev": "vite" },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5.0.0",
    "svelte": "^5.0.0",
    "vite": "^6.0.0"
  }
}
PKGJSON

cat > vite.config.js << 'VITECFG'
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
// base MUST stay './' — page is served under /home/ and needs relative asset paths
export default defineConfig({
  base: './',
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
    <title>Araliya</title>
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
  let message = $state('Welcome to Araliya');
</script>
<main>
  <h1>{message}</h1>
</main>
<style>
  main { text-align: center; padding: 2rem; font-family: sans-serif; background: #0f0f0f; color: #fff; min-height: 100vh; }
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
    ReadTheme {
        /// Theme filename to load (e.g. `"seedling-design-guide.html"`),
        /// or `"list"` to list available themes.
        name: String,
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
    pub theme_guides_dir: Option<std::path::PathBuf>,
}

impl WebBuilderLoop {
    pub fn new(max_iterations: usize, theme_guides_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            max_iterations,
            theme_guides_dir,
        }
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
        let theme_guides_dir = self.theme_guides_dir.clone();

        tokio::spawn(async move {
            let themes = theme_guides_dir
                .as_deref()
                .map(tools::list_theme_guides)
                .unwrap_or_default();
            run_loop(
                channel_id,
                content,
                session_id,
                max_iters,
                "webbuilder",
                None, // runtime_name derived from session
                webbuilder_system_prompt(&themes),
                None, // finish URL derived from runtime_name
                SCAFFOLD_VITE_SVELTE,
                theme_guides_dir,
                state,
                tx,
            )
            .await;
        });

        Ok(BusPayload::LlmStreamResult {
            rx: StreamReceiver(rx),
        })
    }
}

// ── HomebuilderLoop ───────────────────────────────────────────────────────────

/// Singleton variant of [`WebBuilderLoop`] for the `homebuilder` agent.
///
/// **NEW:** Uses static initialization — writes deterministic HTML/CSS/JS on first run.
/// No LLM, no npm, no build step. Idempotent.
///
/// User identity is optionally created/loaded from `{work_dir}/users/user-*/`.
#[cfg(feature = "plugin-homebuilder")]
pub(crate) struct HomebuilderLoop {
    pub user_name: String,
    pub notes_dir: Option<String>,
}

#[cfg(feature = "plugin-homebuilder")]
impl HomebuilderLoop {
    pub fn new(user_name: String, notes_dir: Option<String>) -> Self {
        Self {
            user_name,
            notes_dir,
        }
    }

    pub async fn run_stream(
        &self,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        state: Arc<AgentsState>,
    ) -> BusResult {
        let (tx, rx) = mpsc::channel::<StreamChunk>(128);
        let user_name = self.user_name.clone();
        let notes_dir = self.notes_dir.clone();

        tokio::spawn(async move {
            // Derive dist_dir
            let memory_root = state.memory.memory_root().to_path_buf();
            let identity_dir = memory_root.parent().expect("memory has parent");
            let dist_dir = identity_dir
                .join("runtimes")
                .join("homebuilder")
                .join("dist");

            // Route: if page already exists, use LLM modification path; else init
            if dist_dir.join("index.html").exists() {
                run_llm_modify(channel_id, content, dist_dir, state, tx).await;
            } else {
                run_static_init(channel_id, session_id, user_name, notes_dir, state, tx).await;
            }
        });

        Ok(BusPayload::LlmStreamResult {
            rx: StreamReceiver(rx),
        })
    }
}

/// Static initialization flow for homebuilder.
/// Emits step events and completes without calling the LLM.
#[cfg(feature = "plugin-homebuilder")]
async fn run_static_init(
    _channel_id: String,
    _session_id: Option<String>,
    user_name: String,
    notes_dir: Option<String>,
    state: Arc<AgentsState>,
    tx: mpsc::Sender<StreamChunk>,
) {
    use std::path::PathBuf;

    // Step 1: Emit init event
    emit_step(
        &tx,
        serde_json::json!({
            "type": "init",
            "message": "Initializing homebuilder welcome page..."
        }),
    )
    .await;

    // Step 2: Derive dirs from identity_dir.
    // memory_root is {identity_dir}/memory; runtimes root is {identity_dir}/runtimes.
    let memory_root = state.memory.memory_root().to_path_buf();
    let identity_dir = memory_root.parent().expect("memory has parent");
    let dist_dir = identity_dir
        .join("runtimes")
        .join("homebuilder")
        .join("dist");

    // Step 3: Create/load user identity so we can show public_id on the page.
    let work_dir = identity_dir.parent().expect("identity_dir has parent");
    let work_dir_str = work_dir.to_string_lossy().into_owned();
    let public_id = match araliya_core::user_identity::create_or_load(
        &work_dir_str,
        if user_name.is_empty() {
            None
        } else {
            Some(user_name.clone())
        },
        notes_dir.as_ref().map(PathBuf::from),
    ) {
        Ok(u) => Some(u.identity.public_id),
        Err(e) => {
            tracing::warn!("failed to create user identity: {}", e);
            None
        }
    };

    // Step 4: Generate static page (idempotent), passing public_id for display.
    if let Err(e) = super::init_home::write_static_page(
        &dist_dir,
        &user_name,
        notes_dir.as_deref(),
        public_id.as_deref(),
    ) {
        emit_step(
            &tx,
            serde_json::json!({
                "type": "error",
                "message": format!("failed to write page: {}", e)
            }),
        )
        .await;
        return;
    }

    emit_step(
        &tx,
        serde_json::json!({
            "type": "file_write",
            "path": "dist/index.html"
        }),
    )
    .await;

    // Step 5: Emit completion
    emit_step(
        &tx,
        serde_json::json!({
            "type": "done",
            "preview_url": "/home/",
            "message": "Welcome page ready at /home/"
        }),
    )
    .await;
}

/// LLM-driven modification flow for homebuilder.
/// Reads current page files, passes them + user request to LLM, writes back modified files.
#[cfg(feature = "plugin-homebuilder")]
async fn run_llm_modify(
    channel_id: String,
    content: String,
    dist_dir: std::path::PathBuf,
    state: Arc<AgentsState>,
    tx: mpsc::Sender<StreamChunk>,
) {
    use std::path::Path;

    // Step 1: Emit init event
    emit_step(
        &tx,
        serde_json::json!({
            "type": "init",
            "message": "Modifying page..."
        }),
    )
    .await;

    // Step 2: Read current files
    let read_file = |path: &Path| match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => "(file not found)".to_string(),
    };

    let html = read_file(&dist_dir.join("index.html"));
    let css = read_file(&dist_dir.join("style.css"));
    let js = read_file(&dist_dir.join("app.js"));

    // Step 3: Build user prompt with current files + request
    let user_prompt = format!(
        "Current page files:\n\n=== index.html ===\n{}\n\n=== style.css ===\n{}\n\n=== app.js ===\n{}\n\nUser request:\n{}\n\nRespond with JSON as specified.",
        html, css, js, content
    );

    // Step 4: Call LLM
    let llm_result = state
        .complete_via_llm_with_system(&channel_id, &user_prompt, Some(HOMEBUILDER_MODIFY_SYSTEM))
        .await;

    let llm_text = match llm_result {
        Ok(BusPayload::CommsMessage { content, .. }) => content,
        _ => {
            emit_step(
                &tx,
                serde_json::json!({
                    "type": "error",
                    "message": "LLM call failed"
                }),
            )
            .await;
            let _ = tx
                .send(StreamChunk::Done {
                    usage: None,
                    timing: None,
                })
                .await;
            return;
        }
    };

    // Step 5: Parse LLM response
    let response = match parse_llm_response(&llm_text) {
        Some(r) => r,
        None => {
            emit_step(
                &tx,
                serde_json::json!({
                    "type": "error",
                    "message": "Invalid JSON response from LLM"
                }),
            )
            .await;
            let _ = tx
                .send(StreamChunk::Done {
                    usage: None,
                    timing: None,
                })
                .await;
            return;
        }
    };

    // Step 6: Execute write_file commands
    for cmd in response.commands {
        if let WbCommand::WriteFile { path, content } = cmd {
            // Path validation: must be relative, no ..
            if path.contains("..") || path.contains("/") && path.starts_with("/") {
                emit_step(
                    &tx,
                    serde_json::json!({
                        "type": "error",
                        "message": format!("Invalid path: {path}")
                    }),
                )
                .await;
                continue;
            }

            let full_path = dist_dir.join(&path);

            // Ensure parent directory exists
            if let Some(parent) = full_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match std::fs::write(&full_path, &content) {
                Ok(_) => {
                    emit_step(
                        &tx,
                        serde_json::json!({
                            "type": "file_write",
                            "path": path
                        }),
                    )
                    .await;
                }
                Err(e) => {
                    emit_step(
                        &tx,
                        serde_json::json!({
                            "type": "error",
                            "message": format!("Failed to write {path}: {e}")
                        }),
                    )
                    .await;
                }
            }
        }
    }

    // Step 7: Emit completion
    let message = if response.message.is_empty() {
        "Page updated.".to_string()
    } else {
        response.message
    };
    emit_step(
        &tx,
        serde_json::json!({
            "type": "done",
            "preview_url": "/home/",
            "message": message
        }),
    )
    .await;

    let _ = tx
        .send(StreamChunk::Done {
            usage: None,
            timing: None,
        })
        .await;
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
    if let Some(start) = json_text.find('{')
        && let Some(end) = json_text.rfind('}')
    {
        let slice = &json_text[start..=end];
        if let Ok(resp) = serde_json::from_str::<LlmResponse>(slice) {
            return Some(resp);
        }
    }
    None
}

const JSON_FORMAT_RULES: &str = r#"IMPORTANT: Always respond with a JSON object in this EXACT format (no other text outside the JSON):

{
  "message": "Brief description of what you are doing",
  "commands": [
    {"type": "write_file", "path": "relative/path/to/file", "content": "file content here"},
    {"type": "run_cmd", "command": "npm run build"},
    {"type": "read_theme", "name": "seedling-design-guide.html"},
    {"type": "read_theme", "name": "list"}
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
- Use "read_theme" with name "list" to list available design guides. Use "read_theme" with a filename to load a guide's CSS palette and component reference into context."#;

/// System prompt for the webbuilder agent.
/// `themes` is the list of available design guide filenames (may be empty).
pub(crate) fn webbuilder_system_prompt(themes: &[String]) -> String {
    let theme_hint = if themes.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nAvailable design themes (use read_theme to load): {}\n",
            themes.join(", ")
        )
    };
    format!(
        "You are a Svelte web page builder agent. You work iteratively to build and improve static Svelte 5 + Vite pages.\n\nYou have a pre-scaffolded Vite + Svelte 5 project. You can write files and run shell commands to build the page.{theme_hint}\n\n{JSON_FORMAT_RULES}\n"
    )
}

/// System prompt for the homebuilder agent (unused — kept for reference).
#[cfg(feature = "plugin-homebuilder")]
#[allow(dead_code)]
pub(crate) fn homebuilder_system_prompt() -> String {
    format!(
        r#"You are a landing-page builder for an AI bot called Araliya.
Build a clean, dark-themed Vite + Svelte 5 single-page website that serves as the bot's public homepage.

The page must:
- Display "Araliya" as the bot name with a short tagline.
- Show a status indicator ("Bot is running.").
- Include a prominent link to /ui/ ("Open Chat →").
- List agents by name if provided in the task description.
- Use system-ui fonts only (no external CDN links).
- Dark theme: background #0f0f0f, card #1a1a1a, accent #646cff.

CRITICAL BUILD RULES (do not change these):
- vite.config.js MUST keep `base: './'` — removing it breaks asset loading.
- If you write a package.json, the build script must be `"vite build"` (base is set in config, not CLI).
- Never add `base: '/'` or any other base value.

{JSON_FORMAT_RULES}
"#
    )
}

/// System prompt for modifying an existing homebuilder page via LLM.
#[cfg(feature = "plugin-homebuilder")]
const HOMEBUILDER_MODIFY_SYSTEM: &str = r#"You are modifying an existing Araliya homebuilder page.
You will receive the current HTML, CSS, and JS files. Modify them according to the user's request.

Respond with ONLY a JSON object:
{
  "message": "Brief description of the change",
  "commands": [
    {"type": "write_file", "path": "index.html", "content": "...full modified file..."},
    {"type": "write_file", "path": "style.css", "content": "..."}
  ],
  "finish": true
}

Rules:
- Only include files you actually changed.
- Always write full file contents (not diffs).
- Preserve the Seedling dark marine palette (--void/#080c0e bg, --ice/#4db8cc accent) unless asked to change it.
- Do not add external dependencies or CDN links.
- Keep the /ui/ navigation link."#;

/// Build the per-turn context prompt.
fn context_prompt(
    task: &str,
    file_tree: &[String],
    iteration: usize,
    history: &[String],
) -> String {
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
///
/// - `agent_name`: used for session store and log labels (`"webbuilder"` or `"homebuilder"`)
/// - `fixed_runtime_name`: `Some("homebuilder")` for the singleton agent; `None` to derive
///   the runtime name from the session ID (webbuilder behaviour)
/// - `system`: LLM system prompt
/// - `fixed_finish_url`: `Some("/home/")` for homebuilder; `None` to use
///   `/preview/{runtime_name}/` (webbuilder behaviour)
/// - `scaffold_script`: the bash script used to initialise the workspace
#[allow(clippy::too_many_arguments)]
async fn run_loop(
    channel_id: String,
    content: String,
    session_id: Option<String>,
    max_iterations: usize,
    agent_name: &'static str,
    fixed_runtime_name: Option<&'static str>,
    system: String,
    fixed_finish_url: Option<&'static str>,
    scaffold_script: &'static str,
    theme_guides_dir: Option<std::path::PathBuf>,
    state: Arc<AgentsState>,
    tx: mpsc::Sender<StreamChunk>,
) {
    // ── Session ───────────────────────────────────────────────────────────────
    let handle = {
        let agent_store = match state.open_agent_store(agent_name) {
            Ok(s) => s,
            Err(e) => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": format!("session error: {e}")}),
                )
                .await;
                let _ = tx
                    .send(StreamChunk::Done {
                        usage: None,
                        timing: None,
                    })
                    .await;
                return;
            }
        };
        let memory = &state.memory;
        let result = if let Some(ref sid) = session_id {
            memory.load_session_in(
                &agent_store.agent_sessions_dir(),
                &agent_store.agent_sessions_index(),
                sid,
                Some(agent_name),
            )
        } else {
            agent_store.get_or_create_session(memory, agent_name)
        };
        match result {
            Ok(h) => h,
            Err(e) => {
                emit_step(
                    &tx,
                    serde_json::json!({"type": "error", "message": format!("session init failed: {e}")}),
                )
                .await;
                let _ = tx
                    .send(StreamChunk::Done {
                        usage: None,
                        timing: None,
                    })
                    .await;
                return;
            }
        }
    };

    // ── Runtime name: fixed (homebuilder) or derived from session (webbuilder) ─
    let runtime_name = match fixed_runtime_name {
        Some(name) => name.to_string(),
        None => {
            let short_sid = handle
                .session_id
                .replace('-', "")
                .chars()
                .take(8)
                .collect::<String>();
            format!("{agent_name}-{short_sid}")
        }
    };

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
                let _ = tx
                    .send(StreamChunk::Done {
                        usage: None,
                        timing: None,
                    })
                    .await;
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
            Some(scaffold_script),
            SCAFFOLD_TIMEOUT_SECS,
        )
        .await
        {
            Ok(dir) => {
                if let Err(e) = handle.kv_set("workspace_ready", "true").await {
                    warn!("{agent_name}: kv_set workspace_ready failed: {e}");
                }
                if let Err(e) = handle.kv_set("workspace_dir", &dir).await {
                    warn!("{agent_name}: kv_set workspace_dir failed: {e}");
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
                let _ = tx
                    .send(StreamChunk::Done {
                        usage: None,
                        timing: None,
                    })
                    .await;
                return;
            }
        }
    };

    // Append user message to transcript.
    if let Err(e) = handle.transcript_append("user", &content).await {
        warn!("{agent_name}: transcript_append(user) failed: {e}");
    }

    // ── Iteration loop ────────────────────────────────────────────────────────
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
        if let Some(ref resp) = parsed
            && !resp.message.is_empty()
        {
            let _ = tx
                .send(StreamChunk::Content(format!("{}\n", resp.message)))
                .await;
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
                WbCommand::WriteFile {
                    path,
                    content: file_content,
                } => {
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
                    emit_step(&tx, serde_json::json!({"type": "run_cmd", "cmd": &command})).await;
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
                                step_summary
                                    .push_str(&format!(" stdout={};", result.stdout.trim()));
                            }
                            if !result.stderr.is_empty() {
                                step_summary
                                    .push_str(&format!(" stderr={};", result.stderr.trim()));
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

                WbCommand::ReadTheme { name } => {
                    let result = if let Some(ref dir) = theme_guides_dir {
                        if name == "list" {
                            let names = tools::list_theme_guides(dir);
                            if names.is_empty() {
                                "(no theme guides available)".to_string()
                            } else {
                                names.join("\n")
                            }
                        } else {
                            match tools::read_theme_guide(dir, &name) {
                                Ok(content) => content,
                                Err(e) => format!("error: {e}"),
                            }
                        }
                    } else {
                        "(no theme guides configured)".to_string()
                    };
                    emit_step(
                        &tx,
                        serde_json::json!({
                            "type": "theme_loaded",
                            "name": name,
                        }),
                    )
                    .await;
                    history.push(format!("=== Theme: {name} ===\n{result}"));
                    step_summary.push_str(&format!(" theme={name};"));
                }

                WbCommand::Finish { message } => {
                    let preview_url = fixed_finish_url
                        .map(|u| u.to_string())
                        .unwrap_or_else(|| format!("/preview/{runtime_name}/"));
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
                let preview_url = fixed_finish_url
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| format!("/preview/{runtime_name}/"));
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
            serde_json::json!({"type": "error", "message": format!("{agent_name}: reached max iterations ({max_iterations}) without finishing")}),
        )
        .await;
    }

    // Persist assistant summary.
    let final_msg = format!(
        "{agent_name} session complete. Workspace: {workspace_dir}. Steps: {}",
        history.len()
    );
    if let Err(e) = handle.transcript_append("assistant", &final_msg).await {
        warn!("{agent_name}: transcript_append(assistant) failed: {e}");
    }

    let _ = tx
        .send(StreamChunk::Done {
            usage: None,
            timing: None,
        })
        .await;
}
