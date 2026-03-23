# Getting Started

## Quickstart (prebuilt binary)

Download, configure, and run in three commands on Linux (x86\_64 / aarch64):

```bash
# 1. Download the binary and seed config directories
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash

# 2. Interactive setup wizard
araliya-bot setup

# 3. Validate the generated config
araliya-bot doctor

# 4. Start the bot
araliya-bot
```

`install.sh` auto-detects arch, resolves the latest GitHub release, downloads the
appropriate tier tarball (`minimal` / `default` / `full`), installs the binary to
`~/.local/bin/`, and seeds `~/.config/araliya/` with starter config files.

Override defaults with env vars before piping:
```bash
ARALIYA_TIER=full ARALIYA_VERSION=v0.2.0-alpha \
  curl -fsSL .../install.sh | bash
```

Config is written to `~/.config/araliya/config.toml` (TOML, commented).
Secrets (API keys, tokens) are written to `~/.config/araliya/.env` (mode 0600).
Runtime data (identity keypair, sessions, memory) lives in `~/.araliya/`.

### Setup wizard (`araliya-bot setup`)

The wizard walks through four steps:

1. **Bot identity** — bot name and runtime data directory
2. **LLM provider** — OpenAI / OpenRouter / Anthropic / Local Ollama / custom / dummy + API key + model
3. **Agent profile** — basic chat / session chat / agentic chat / docs / newsroom / custom
4. **Channels** — HTTP bind address; Telegram (token validated live via `getMe`)

Re-running `setup` does not overwrite existing secrets in `.env`.

### Config doctor (`araliya-bot doctor`)

Checks config file presence, TOML validity, required sections, and credential env vars.
Exits non-zero on failure — useful as a pre-flight check in scripts or CI:

```bash
araliya-bot doctor && araliya-bot
```

---

## Build from Source

### Requirements

- Rust toolchain 1.80+ (`rustup` recommended)
- Linux or macOS
- Internet access for initial `cargo build` (downloads dependencies)

## Build

```bash
cd araliya-bot
cargo build
```

### Modular Features

Araliya Bot uses Cargo features to enable or disable subsystems, plugins, and channels at compile-time. This allows for lean builds on resource-constrained hardware.

| Feature Group | Features | Description |
|---------------|----------|-------------|
| **Subsystems**| `subsystem-agents`, `subsystem-llm`, `subsystem-comms`, `subsystem-memory` | Main architectural blocks. |
| **Agents**    | `plugin-echo`, `plugin-basic-chat`, `plugin-chat`, `plugin-gmail-agent` | Capabilities for the `agents` subsystem. |
| **Channels**  | `channel-pty`, `channel-http`, `channel-telegram` | I/O channels for the `comms` subsystem. |
| **Tools**     | `subsystem-tools`, `plugin-gmail-tool` | Tool execution and implementations. |
| **UI**        | `subsystem-ui`, `ui-svui`, `ui-gpui` | Web UI backend and optional GPUI desktop client. |
| **Binaries**  | `cli`, `gmail-app` | Additional binaries (`araliya-ctl`, `gmail_read_one`). |

**Default build (Daemon only, all subsystems enabled):**
```bash
cargo build
```

**Build all binaries (Daemon, CLI, Gmail App):**
```bash
cargo build --all-features
```

**Minimal build (No subsystems enabled):**
```bash
cargo build --no-default-features
```

**Custom build (LLM and Agents only):**
```bash
cargo build --no-default-features --features subsystem-llm,subsystem-agents,plugin-basic-chat
```

For a release build:
```bash
cargo build --release --locked
```

### Building the Web UI

The Svelte UI lives in `frontend/svui/` and builds to `frontend/build/`:

> 🛈 The UI version is automatically copied from the Rust package (`crates/araliya-bot/Cargo.toml`) by a small prebuild script. Running `pnpm build` or `pnpm dev` will update `frontend/svui/.env` with the current version.

```bash
cd frontend/svui
pnpm install
pnpm build
```

The bot serves the built UI at `http://127.0.0.1:8080/ui/` when `comms.http.enabled = true` and `ui.svui.enabled = true`.

For development with hot reload:

```bash
cd frontend/svui
pnpm dev   # starts on http://localhost:5173/ui/
```

Set `VITE_API_BASE_URL=http://127.0.0.1:8080` in the dev environment to proxy API calls to the running bot.

### Building the GPUI Desktop Client

The optional native desktop client is provided as a separate binary under `src/bin/araliya-gpui/` and is gated behind the `ui-gpui` feature.

**Linux system dependencies** (XCB and XKB libraries) must be installed first — see [docs/development/gpui.md](development/gpui.md) for details and distro-specific install commands.

```bash
cargo check --bin araliya-gpui --features ui-gpui
cargo run --bin araliya-gpui --features ui-gpui
```

By default it targets `http://127.0.0.1:8080` and expects the bot API to be running there.

For CI/reproducible environments:

```bash
cargo build --release --locked --frozen
```

Binary output: `target/debug/araliya-bot` or `target/release/araliya-bot`.

Quick size checks:

```bash
ls -lh target/release/araliya-bot
size target/release/araliya-bot
readelf -S target/release/araliya-bot | grep -E '\.debug|\.symtab|\.strtab'
```

## Run

### Daemon mode (default)

```bash
cargo run
# or
./target/debug/araliya-bot
```

No stdin is read, no stdout is written. All tracing output goes to stderr (journald-compatible). The Unix domain socket at `{work_dir}/araliya.sock` is always active for management.

To write logs to a file instead, pass:

```bash
./target/debug/araliya-bot --log-file /tmp/araliya.log
```

The file is opened in append mode.

### Interactive mode

```bash
./target/debug/araliya-bot -i
```

Activates the stdio management adapter and PTY channel:

```
# /status
# /health
# /chat <message>
# /exit
```

### GPUI Desktop mode

Run the bot API and GPUI desktop client in separate terminals:

```bash
# Terminal 1: bot API
cargo run

# Terminal 2: desktop UI
cargo run --bin araliya-gpui --features ui-gpui
```

The GPUI client currently covers baseline chat flows: health status, sessions list, transcript view, and message send.

### Management CLI (`araliya-ctl`)

While the daemon is running (in either mode), use `araliya-ctl` from any terminal:

```bash
./target/debug/araliya-ctl status
./target/debug/araliya-ctl health
./target/debug/araliya-ctl subsystems
./target/debug/araliya-ctl shutdown
```

Socket path resolution: `--socket <path>` → `$ARALIYA_WORK_DIR/araliya.sock` → `~/.araliya/araliya.sock`.

### First Run

On first run the bot generates a persistent ed25519 keypair and saves it to `~/.araliya/bot-pkey{id}/`. Expected output:

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

### Subsequent Runs

The existing keypair is loaded. The same `bot_id` is printed every time:

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

## Verify

```bash
# Check identity files were created
ls ~/.araliya/
# → bot-pkey5d16993c/

ls ~/.araliya/bot-pkey5d16993c/
# → id_ed25519   id_ed25519.pub

# Verify secret key permissions
stat -c "%a %n" ~/.araliya/bot-pkey5d16993c/id_ed25519
# → 600 ...
```

## Environment Variables

| Flag / Variable | Effect |
|-----------------|--------|
| `-i` / `--interactive` | Enable interactive mode (management adapter + PTY). Default: daemon mode, no stdio. |
| `ARALIYA_WORK_DIR` | Override working directory (default: `~/.araliya`) |
| `ARALIYA_LOG_LEVEL` | Override log level (default: `info`) |
| `RUST_LOG` | Standard tracing env filter (overrides `log_level`) |
| `-v` | CLI override → `warn` |
| `-vv` | CLI override → `info` |
| `-vvv` | CLI override → `debug` |
| `-vvvv` | CLI override → `trace` |

Example:

```bash
ARALIYA_WORK_DIR=/tmp/test-bot RUST_LOG=debug cargo run

# CLI verbosity override
cargo run -- -vvv
```

## Run Tests

```bash
# All crates
cargo test --workspace        # ~318 tests

# Per-crate
cargo test -p araliya-core
cargo test -p araliya-supervisor
cargo test -p araliya-llm
cargo test -p araliya-comms
cargo test -p araliya-bot
```

Tests use `tempfile` for filesystem isolation — they do not touch `~/.araliya`.

## Local Testing Without an API Key

The `dummy` LLM provider echoes input back as `[echo] {input}`. Combined with the `minimal` feature set and `basic_chat` agent it gives a full bus round-trip with zero external dependencies:

```bash
cargo build -p araliya-bot --no-default-features --features minimal
./target/debug/araliya-bot -i --config config/dummy.toml
```

Type any message at the prompt — the reply will be `[echo] <your message>`.
