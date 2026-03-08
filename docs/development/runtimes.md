# Runtimes Subsystem

The runtimes subsystem lets agents execute scripts in external language runtimes
via the message bus.

- A **runtime** is a named, persistent working directory:
  `{identity_dir}/runtimes/{runtime}/`. It isolates file-system side-effects
  (installed packages, generated files, config) so they survive across calls.

- The **command** field specifies the interpreter binary to spawn (e.g. `node`,
  `python3`). It defaults to `bash` when omitted.

Because directory and interpreter are separate, you can have multiple
environments backed by the same binary — for example `runtime: "ml-project"`
with `command: "python3"` — each with its own isolated directory.

## On-Disk Layout

```
{identity_dir}/runtimes/
├── my-node-app/   ← working dir for runtime "my-node-app"
├── ml-project/    ← working dir for runtime "ml-project"
├── svelte-app/    ← working dir for runtime "svelte-app"
└── ...
```

Inline scripts are written to temporary files (`_exec_{uuid}.{ext}`) inside the
runtime directory and cleaned up after execution. Everything else the script
creates (e.g. `node_modules/`, `package.json`) persists.

## Configuration

In your TOML config (or `config/default.toml`):

```toml
[runtimes]
enabled = true
default_timeout_secs = 30
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Enable/disable the subsystem |
| `default_timeout_secs` | `30` | Per-execution timeout; overridable per request |

The subsystem is feature-gated behind `subsystem-runtimes` (included in `default`
and `full` feature tiers).

## Bus API Reference

### `runtimes/init`

Bootstrap a runtime environment by creating its working directory and optionally
running a setup script.

**Request — `RuntimeInitRequest`**

```json
{
  "runtime": "my-node-app",
  "setup_script": "npm init -y && npm install express",
  "env": {},
  "timeout_secs": 60
}
```

| Field | Type | Description |
|-------|------|-------------|
| `runtime` | `String` | Environment name — used as the working directory name |
| `setup_script` | `Option<String>` | Shell script to run inside the directory after creation (via `bash -c`) |
| `env` | `HashMap<String, String>` | Extra environment variables for the setup script |
| `timeout_secs` | `Option<u64>` | Override the default timeout for the setup script |

**Response — `RuntimeInitResult`**

```json
{
  "success": true,
  "exit_code": 0,
  "stdout": "...",
  "stderr": "",
  "runtime_dir": "/home/user/.araliya/bot-pkey.../runtimes/my-node-app"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `success` | `bool` | `true` when directory was created and setup script (if any) exited 0 |
| `exit_code` | `Option<i32>` | Exit code of setup script, or `null` if no script was provided |
| `stdout` | `String` | Captured stdout from setup script (empty if no script) |
| `stderr` | `String` | Captured stderr from setup script (empty if no script) |
| `runtime_dir` | `String` | Absolute path to the runtime directory |

### `runtimes/exec`

Execute a script in an external runtime.

**Request — `RuntimeExecRequest`**

```json
{
  "runtime": "my-node-app",
  "command": "node",
  "source": "console.log('hello')",
  "env": {},
  "timeout_secs": 30
}
```

| Field | Type | Description |
|-------|------|-------------|
| `runtime` | `String` | Environment name — used as the working directory name |
| `command` | `Option<String>` | Interpreter binary (e.g. `node`, `python3`). Defaults to `bash`. |
| `source` | `Option<String>` | Inline script code. Mutually exclusive with `script_path`. |
| `script_path` | `Option<String>` | Path to an existing script file. Mutually exclusive with `source`. |
| `env` | `HashMap<String, String>` | Extra environment variables for the child process |
| `timeout_secs` | `Option<u64>` | Override the default timeout for this execution |

Exactly one of `source` or `script_path` must be provided.

**Response — `RuntimeExecResult`**

```json
{
  "success": true,
  "exit_code": 0,
  "stdout": "hello\n",
  "stderr": "",
  "duration_ms": 42
}
```

| Field | Type | Description |
|-------|------|-------------|
| `success` | `bool` | `true` when process exited with code 0 |
| `exit_code` | `Option<i32>` | Raw exit code, or `null` if killed by signal |
| `stdout` | `String` | Captured stdout (UTF-8 lossy) |
| `stderr` | `String` | Captured stderr (UTF-8 lossy) |
| `duration_ms` | `u64` | Wall-clock execution time in milliseconds |

### `runtimes/status`

Returns a `ComponentStatusResponse` with the subsystem's health state.

## Node.js Project Example

An agent bootstraps and uses a Node.js project:

```json
// 1. Init the environment and install dependencies
{ "runtime": "my-node-app", "setup_script": "npm init -y && npm install express", "timeout_secs": 60 }
```

```json
// 2. Run a Node script that uses the installed package
{
  "runtime": "my-node-app",
  "command": "node",
  "source": "console.log('express v' + require('express/package.json').version)"
}
```

Step 1 uses `runtimes/init` — the `setup_script` runs in a shell and installs
dependencies. Step 2 uses `runtimes/exec` with `command: "node"` to run
JavaScript directly. Both share the same working directory.

## Python Example

```json
// 1. Init with a virtualenv
{ "runtime": "ml-project", "setup_script": "python3 -m venv .venv && .venv/bin/pip install requests", "timeout_secs": 60 }
```

```json
// 2. Run Python code inside the environment
{
  "runtime": "ml-project",
  "command": "python3",
  "source": "import json, sys\ndata = {'status': 'ok', 'python': sys.version}\nprint(json.dumps(data))"
}
```

## End-to-End Example: Agent Creates a SvelteKit App

### Step 1 — Init and scaffold

```json
{
  "runtime": "svelte-app",
  "setup_script": "yes '' | npx sv create my-app --template minimal --types ts && cd my-app && npm install",
  "timeout_secs": 120
}
```

### Step 2 — Start the dev server

```json
{
  "runtime": "svelte-app",
  "command": "bash",
  "source": "cd my-app && npm run dev -- --port 4000 &\nsleep 3\ncurl -s -o /dev/null -w '%{http_code}' http://localhost:4000",
  "timeout_secs": 30
}
```

### Step 3 — Return the URL

The agent reads `stdout` (`"200"`) from step 2 to confirm the server is up,
then responds to the user with `http://localhost:4000`.

## Timeouts & Error Handling

- Each execution is bounded by `timeout_secs` (per-request) or
  `default_timeout_secs` (config-level, default 30s).
- When a timeout fires the child process is killed and the response returns
  `success: false` with `exit_code: null`.
- Non-zero exit codes set `success: false`; `stderr` contains the error output.
- For `runtimes/exec`: if neither `source` nor `script_path` is provided, the
  bus call returns an error immediately.
