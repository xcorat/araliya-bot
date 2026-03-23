# Setup & Install System — Implementation Plan

> **Goal:** A user downloads one shell script via curl, it checks prerequisites,
> downloads the right pre-built binary tier, writes a config file by guiding the
> user through an interactive wizard (bot name, LLM provider/key, agent profile,
> channels), and leaves them with a working bot they can start immediately.

**Architecture in one paragraph:**
The entry point is `install.sh` (hosted at the project URL, curled by the user).
It handles prereqs and binary download only — no interactive config. After
the binary is installed, it delegates all configuration to a first-class
`araliya-bot setup` subcommand that lives inside the Rust binary itself. That
subcommand is a self-contained interactive TUI wizard built with the
`dialoguer` + `console` crates. It reads nothing from an existing config,
asks a series of opinionated questions, and writes a final `config.toml` +
`.env` to the install directory. A second lightweight subcommand
`araliya-bot doctor` lets users validate their config at any time.

**Tech stack:**
- Install script: `bash`, POSIX-compatible, uses only `curl`/`wget`, `tar`, `uname`
- Setup wizard: Rust — `dialoguer` (interactive prompts), `console` (colors/styling),
  `indicatif` (spinners), writes TOML with `toml` + `serde`
- Feature flag: `setup` feature gates the wizard binary behind a cargo feature
  so it doesn't bloat the minimal/headless build

**Design principles drawn from Zed / SvelteKit / Hermes:**
- Zed: thin install script, no root, `~/.local/bin` shim, arch detection, strict `set -euo pipefail`
- SvelteKit `sv create`: grouped feature selection with hints, per-feature follow-up questions, declarative "profile" output, "next steps" summary at the end
- Hermes: ASCII box banner, colored step/ok/warn/err helpers, prereq check, auto-runs wizard after install, PATH hint if needed

---

## Phase 1 — Install Script (`install.sh`)

Thin, fast, no interactive config. Just gets the binary on disk.

---

### Task 1.1: Scaffold `install.sh` with strict mode + color helpers

**Files:**
- Create: `install.sh` (repo root)

**Step 1: Create the file with shebang, strict mode, and color helpers**

```bash
#!/usr/bin/env bash
set -euo pipefail

# ── defaults ──────────────────────────────────────────────────────────
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
WORK_DIR="${ARALIYA_WORK_DIR:-$HOME/.araliya}"
REPO="xcorat/araliya-bot"                         # update when public
BASE_URL="https://github.com/${REPO}/releases/download"
TIER="${ARALIYA_TIER:-default}"                    # minimal | default | full

# ── colors ────────────────────────────────────────────────────────────
RED='\033[0;31m';  GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m';     NC='\033[0m'

step() { printf "\n${CYAN}── %s${NC}\n" "$1"; }
ok()   { printf "${GREEN}✓ %s${NC}\n"   "$1"; }
warn() { printf "${YELLOW}⚠ %s${NC}\n"  "$1"; }
err()  { printf "${RED}✗ %s${NC}\n"     "$1"; }
need() {
  if ! command -v "$1" &>/dev/null; then
    err "Required tool '$1' not found. Please install it and re-run."
    exit 1
  fi
}
```

**Step 2: Add `main()` entry point (empty for now) and call it at EOF**

```bash
main() {
  printf "\n${BOLD}${CYAN}"
  printf "╔══════════════════════════════════════════╗\n"
  printf "║        Araliya Bot  ·  Installer         ║\n"
  printf "╚══════════════════════════════════════════╝${NC}\n\n"
}

main
```

**Step 3: Make it executable and verify it runs**

```bash
chmod +x install.sh
bash install.sh
```

Expected: banner prints, exits 0.

**Step 4: Commit**

```bash
git add install.sh
git commit -m "feat(setup): scaffold install.sh with banner and color helpers"
```

---

### Task 1.2: Prerequisite checks in `install.sh`

**Files:**
- Modify: `install.sh` — fill in the prereq section of `main()`

**Step 1: Add arch + OS detection + prereq block inside `main()`**

```bash
  step "Checking prerequisites"

  need curl    # or wget — handled below
  need tar

  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)
      err "Unsupported OS: $OS"
      exit 1
      ;;
  esac

  case "$ARCH" in
    x86_64)          ARCH_TAG="x86_64" ;;
    aarch64|arm64)   ARCH_TAG="aarch64" ;;
    *)
      err "Unsupported architecture: $ARCH"
      exit 1
      ;;
  esac

  ok "Platform: $PLATFORM/$ARCH_TAG"
```

Note: macOS builds are not yet released (CI only targets linux). The script
should gracefully warn on macOS and offer to build from source (Task 1.5).

**Step 2: Detect curl vs wget; wrap into a uniform `fetch` function**

```bash
  if command -v curl &>/dev/null; then
    fetch() { command curl -fsSL "$1" -o "$2"; }
    fetch_stdout() { command curl -fsSL "$1"; }
  elif command -v wget &>/dev/null; then
    fetch() { wget -qO "$2" "$1"; }
    fetch_stdout() { wget -qO- "$1"; }
  else
    err "Neither curl nor wget found. Install one and re-run."
    exit 1
  fi
```

**Step 3: Verify by running and checking output**

```bash
bash install.sh
```

Expected: step/ok lines for prereqs, no error.

**Step 4: Commit**

```bash
git commit -am "feat(setup): add OS/arch detection and prereq checks"
```

---

### Task 1.3: Resolve latest release version

**Files:**
- Modify: `install.sh` — add version resolution inside `main()`

**Step 1: Add version logic (explicit env var > GitHub API latest)**

```bash
  step "Resolving version"

  if [[ -n "${ARALIYA_VERSION:-}" ]]; then
    VERSION="$ARALIYA_VERSION"
    ok "Using pinned version: $VERSION"
  else
    VERSION="$(fetch_stdout "https://api.github.com/repos/${REPO}/releases/latest" \
               | grep '"tag_name"' \
               | head -1 \
               | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    if [[ -z "$VERSION" ]]; then
      err "Could not determine latest release. Set ARALIYA_VERSION=vX.Y.Z and re-run."
      exit 1
    fi
    ok "Latest: $VERSION"
  fi
```

**Step 2: Validate TIER value**

```bash
  case "$TIER" in
    minimal|default|full) ;;
    *)
      warn "Unknown tier '$TIER'. Defaulting to 'default'."
      TIER="default"
      ;;
  esac
  ok "Tier: $TIER"
```

**Step 3: Commit**

```bash
git commit -am "feat(setup): resolve latest release version from GitHub API"
```

---

### Task 1.4: Download, verify, and install binary

**Files:**
- Modify: `install.sh` — add download + install section

**Step 1: Construct download URL and download**

```bash
  step "Downloading Araliya Bot ($TIER)"

  ARCHIVE="araliya-bot-${VERSION}-${TIER}-${ARCH_TAG}-unknown-linux-gnu.tar.gz"
  URL="${BASE_URL}/${VERSION}/${ARCHIVE}"
  TMP="$(mktemp -d /tmp/araliya-XXXXXX)"

  fetch "$URL" "$TMP/$ARCHIVE" || {
    err "Download failed: $URL"
    err "Check that version $VERSION has a $TIER build for $ARCH_TAG."
    exit 1
  }
  ok "Downloaded $ARCHIVE"
```

**Step 2: Extract and install binary**

```bash
  step "Installing"

  tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
  EXTRACTED="$(tar -tzf "$TMP/$ARCHIVE" | head -1 | cut -d/ -f1)"

  mkdir -p "$INSTALL_DIR"
  cp "$TMP/$EXTRACTED/bin/araliya-bot" "$INSTALL_DIR/araliya-bot"
  chmod +x "$INSTALL_DIR/araliya-bot"
  ok "Binary installed to $INSTALL_DIR/araliya-bot"

  # Copy default configs if not already present
  mkdir -p "$WORK_DIR/config"
  if [[ ! -f "$WORK_DIR/config/config.toml" ]]; then
    cp "$TMP/$EXTRACTED/config/default.toml" "$WORK_DIR/config/config.toml"
    ok "Default config written to $WORK_DIR/config/config.toml"
  else
    warn "Config already exists — skipping (run 'araliya-bot setup' to reconfigure)"
  fi
```

**Step 3: Clean up temp files**

```bash
  rm -rf "$TMP"
```

**Step 4: Commit**

```bash
git commit -am "feat(setup): download, extract, and install binary + default config"
```

---

### Task 1.5: PATH hint + launch setup wizard

**Files:**
- Modify: `install.sh` — add PATH check and final handoff

**Step 1: Add PATH check and profile hint**

```bash
  step "Checking PATH"

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    warn "$INSTALL_DIR is not in your PATH."
    echo "  Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    printf "  ${BOLD}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}\n\n"
    BINARY="$INSTALL_DIR/araliya-bot"
  else
    BINARY="araliya-bot"
    ok "araliya-bot is on your PATH"
  fi
```

**Step 2: Handoff to setup wizard**

```bash
  step "Running setup wizard"
  echo "(Configure your bot name, LLM provider, agent profile, and channels)"
  echo

  "$INSTALL_DIR/araliya-bot" setup --config "$WORK_DIR/config/config.toml"

  printf "\n${GREEN}${BOLD}✓ Araliya Bot is ready!${NC}\n"
  echo "  Start it with: ${BOLD}$BINARY -i${NC}  (interactive/PTY mode)"
  echo "  Or run headless (HTTP API on :8080): ${BOLD}$BINARY${NC}"
  echo
```

**Step 3: Commit**

```bash
git commit -am "feat(setup): add PATH hint and launch setup wizard after install"
```

---

## Phase 2 — `araliya-bot setup` Subcommand (Rust)

The wizard runs inside the binary. It writes `config.toml` + `.env`.
It is gated behind a `setup` cargo feature so headless/embedded builds
can opt out and keep the binary small.

---

### Task 2.1: Add `setup` feature + `dialoguer` / `console` / `indicatif` deps

**Files:**
- Modify: `crates/araliya-bot/Cargo.toml`

**Step 1: Add dependencies**

```toml
[dependencies]
# ... existing deps ...
dialoguer  = { version = "0.11", optional = true }
console    = { version = "0.15", optional = true }
indicatif  = { version = "0.17", optional = true }
```

**Step 2: Add feature**

```toml
[features]
# existing features ...
setup = ["dep:dialoguer", "dep:console", "dep:indicatif"]

# Include setup in default so the wizard is available out of the box
default = [
    "setup",          # <-- add this
    "subsystem-agents",
    # ... rest unchanged
]
```

**Step 3: Verify it compiles**

```bash
cargo check -p araliya-bot --features setup
```

Expected: no errors (dialoguer/console/indicatif have no transitive conflicts).

**Step 4: Commit**

```bash
git commit -am "feat(setup): add setup feature gate with dialoguer/console/indicatif deps"
```

---

### Task 2.2: Create `setup` module skeleton with CLI argument hook

**Files:**
- Create: `crates/araliya-bot/src/setup/mod.rs`
- Modify: `crates/araliya-bot/src/main.rs` — add `setup` subcommand to CLI parser

**Step 1: Create the module with a stub `run()` function**

```rust
// crates/araliya-bot/src/setup/mod.rs
//! Interactive first-run setup wizard.
//!
//! Invoked via `araliya-bot setup [--config PATH]`.
//! Writes config.toml + .env and then exits.

use std::path::PathBuf;

pub struct SetupOpts {
    /// Path where config.toml will be written.
    pub config_path: PathBuf,
    /// Path where .env will be written (defaults to same dir as config).
    pub env_path: Option<PathBuf>,
}

/// Entry point — runs the wizard and returns Ok(()) on success.
pub fn run(opts: SetupOpts) -> anyhow::Result<()> {
    println!("(setup wizard placeholder)");
    Ok(())
}
```

**Step 2: Add feature gate and `pub mod setup` in `lib.rs`**

```rust
// crates/araliya-bot/src/lib.rs  (add at bottom)
#[cfg(feature = "setup")]
pub mod setup;
```

**Step 3: Hook into the existing CLI arg parser in `main.rs`**

Find the existing `clap` / manual arg parsing block (around the `-i` / `--config`
flags). Add a `setup` subcommand check **before** the normal boot path:

```rust
// At the top of main(), before any subsystem wiring:
#[cfg(feature = "setup")]
if let Some(pos) = std::env::args().position(|a| a == "setup") {
    let config_path = std::env::args()
        .skip(pos + 1)
        .zip(std::env::args().skip(pos + 2))
        .find(|(f, _)| f == "--config")
        .map(|(_, v)| std::path::PathBuf::from(v))
        .unwrap_or_else(|| dirs::home_dir()
            .unwrap_or_default()
            .join(".araliya/config/config.toml"));

    araliya_bot::setup::run(araliya_bot::setup::SetupOpts {
        config_path,
        env_path: None,
    })?;
    return Ok(());
}
```

**Step 4: Verify**

```bash
cargo build -p araliya-bot 2>&1 | grep -E "error|warning: unused"
./target/debug/araliya-bot setup
```

Expected: "(setup wizard placeholder)" prints, exits 0.

**Step 5: Commit**

```bash
git commit -am "feat(setup): add setup subcommand skeleton with CLI hook"
```

---

### Task 2.3: Banner + step helpers inside the wizard

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs`

**Step 1: Add `console`-based helpers at the top of the module**

```rust
use console::{style, Term};

fn banner() {
    let term = Term::stdout();
    let _ = term.write_line("");
    let _ = term.write_line(&format!(
        "{}",
        style("╔══════════════════════════════════════════╗").cyan().bold()
    ));
    let _ = term.write_line(&format!(
        "{}",
        style("║        Araliya Bot  ·  Setup Wizard      ║").cyan().bold()
    ));
    let _ = term.write_line(&format!(
        "{}",
        style("╚══════════════════════════════════════════╝").cyan().bold()
    ));
    let _ = term.write_line("");
}

fn step(msg: &str) {
    println!("\n{} {}", style("──").cyan(), style(msg).bold());
}

fn ok(msg: &str) {
    println!("{} {}", style("✓").green(), msg);
}

fn warn(msg: &str) {
    println!("{} {}", style("⚠").yellow(), msg);
}
```

**Step 2: Call `banner()` at the start of `run()`**

```rust
pub fn run(opts: SetupOpts) -> anyhow::Result<()> {
    banner();
    step("Welcome! Let's configure your Araliya bot.");
    // ... wizard steps will go here
    Ok(())
}
```

**Step 3: Verify**

```bash
cargo build -p araliya-bot && ./target/debug/araliya-bot setup
```

Expected: colored banner + step line.

**Step 4: Commit**

```bash
git commit -am "feat(setup): add styled banner and step helpers to wizard"
```

---

### Task 2.4: Wizard step 1 — Bot identity (name, work dir)

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs`
- Create: `crates/araliya-bot/src/setup/answers.rs` — collects all answers

**Step 1: Create `answers.rs` to hold the accumulated answers**

```rust
// crates/araliya-bot/src/setup/answers.rs
use std::path::PathBuf;

/// All answers collected by the wizard before writing files.
#[derive(Debug)]
pub struct Answers {
    // Identity
    pub bot_name: String,
    pub work_dir: PathBuf,

    // LLM
    pub llm_provider: LlmProvider,
    pub llm_api_key: String,
    pub llm_model: String,
    pub llm_api_base_url: String,

    // Profile
    pub profile: BotProfile,

    // Channels
    pub enable_telegram: bool,
    pub telegram_token: Option<String>,
    pub http_bind: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    OpenAI,
    OpenRouter,
    Anthropic,
    LocalOllama,
    Dummy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BotProfile {
    BasicChat,
    AgenticChat,
    Newsroom,
    Docs,
    Custom,
}
```

**Step 2: Add the bot identity step to `run()`**

```rust
// in mod.rs, inside run()
use dialoguer::{Input, Confirm};

step("Bot identity");

let bot_name: String = Input::new()
    .with_prompt("Bot name")
    .default("araliya".into())
    .interact_text()?;

let default_work_dir = dirs::home_dir()
    .unwrap_or_default()
    .join(".araliya")
    .display()
    .to_string();

let work_dir_str: String = Input::new()
    .with_prompt("Work directory")
    .default(default_work_dir)
    .interact_text()?;

ok(&format!("Bot name: {bot_name}"));
ok(&format!("Work dir: {work_dir_str}"));
```

**Step 3: Verify**

```bash
cargo build -p araliya-bot && ./target/debug/araliya-bot setup
```

Expected: prompts appear for name and work dir, enter proceeds.

**Step 4: Commit**

```bash
git commit -am "feat(setup): add bot identity step (name, work dir)"
```

---

### Task 2.5: Wizard step 2 — LLM provider + API key + model

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs`

**Step 1: Add provider select prompt**

```rust
use dialoguer::Select;

step("LLM provider");
println!("  (API keys are stored in .env — never in config.toml)");

let providers = &[
    ("OpenAI (api.openai.com)", "https://api.openai.com/v1/chat/completions", "gpt-4o-mini"),
    ("OpenRouter (openrouter.ai — 100+ models)", "https://openrouter.ai/api/v1/chat/completions", "openai/gpt-4o-mini"),
    ("Anthropic (direct)", "https://api.anthropic.com/v1/messages", "claude-opus-4-5"),
    ("Local / Ollama / LM Studio (no key)", "http://127.0.0.1:11434/v1/chat/completions", "llama3"),
    ("Other OpenAI-compatible", "", ""),
    ("Dummy (no API — for testing)", "", ""),
];

let provider_idx = Select::new()
    .with_prompt("Which LLM provider?")
    .items(&providers.iter().map(|(label, _, _)| *label).collect::<Vec<_>>())
    .default(0)
    .interact()?;

let (_, default_base_url, default_model) = providers[provider_idx];
```

**Step 2: Conditionally ask for API key (skip for local/dummy)**

```rust
let needs_key = provider_idx < 3 || provider_idx == 4;

let llm_api_key = if needs_key {
    use dialoguer::Password;
    Password::new()
        .with_prompt("API key")
        .interact()?
} else {
    String::new()
};

let llm_model: String = Input::new()
    .with_prompt("Model name")
    .default(default_model.into())
    .interact_text()?;

let llm_api_base_url: String = Input::new()
    .with_prompt("API base URL")
    .default(default_base_url.into())
    .interact_text()?;

ok(&format!("Provider: {}", providers[provider_idx].0));
ok(&format!("Model: {llm_model}"));
```

**Step 3: Verify**

```bash
./target/debug/araliya-bot setup
```

Expected: after identity step, provider/key/model prompts appear.

**Step 4: Commit**

```bash
git commit -am "feat(setup): add LLM provider/key/model selection step"
```

---

### Task 2.6: Wizard step 3 — Agent profile selection

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs`

The profile maps directly to which agent is set as `[agents] default` and
which agents are `enabled = true` in the output config.

**Step 1: Add grouped profile select**

```rust
step("Agent profile");
println!("  (You can change this later in config.toml)");

let profiles = &[
    ("Basic chat",     "Simple LLM chat. No memory, no tools.",               "basic_chat"),
    ("Session chat",   "Multi-turn chat with conversation history.",           "chat"),
    ("Agentic chat",   "Dual-pass agent: instruction + tool use + response.", "agentic-chat"),
    ("Docs agent",     "RAG over a local docs/ directory.",                   "docs"),
    ("Newsroom",       "Persistent GDELT news monitoring agent.",             "newsroom"),
    ("Custom (skip)",  "Write config manually — use default profile.",        "echo"),
];

let profile_idx = Select::new()
    .with_prompt("Which agent profile?")
    .items(&profiles.iter().map(|(label, hint, _)| {
        format!("{:<20} — {}", label, hint)
    }).collect::<Vec<_>>())
    .default(0)
    .interact()?;

let (profile_label, _, profile_agent) = profiles[profile_idx];
ok(&format!("Profile: {profile_label}"));
```

**Step 2: Commit**

```bash
git commit -am "feat(setup): add agent profile selection step"
```

---

### Task 2.7: Wizard step 4 — Channels (PTY, HTTP, Telegram)

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs`

**Step 1: Add channel checkboxes with `MultiSelect`**

```rust
use dialoguer::MultiSelect;

step("Communication channels");
println!("  (PTY = interactive terminal, HTTP = web UI + API)");

let channel_options = &[
    "PTY (interactive terminal — use -i flag at runtime)",
    "HTTP / Web UI (serves frontend at http://localhost:8080)",
    "Telegram bot",
];

let channel_defaults = &[true, true, false];

let channels = MultiSelect::new()
    .with_prompt("Enable channels (Space to toggle, Enter to confirm)")
    .items(channel_options)
    .defaults(channel_defaults)
    .interact()?;

let enable_http     = channels.contains(&1);
let enable_telegram = channels.contains(&2);

let http_bind = if enable_http {
    let bind: String = Input::new()
        .with_prompt("HTTP bind address")
        .default("127.0.0.1:8080".into())
        .interact_text()?;
    Some(bind)
} else {
    None
};

let telegram_token = if enable_telegram {
    use dialoguer::Password;
    let token = Password::new()
        .with_prompt("Telegram bot token (from @BotFather)")
        .interact()?;
    Some(token)
} else {
    None
};
```

**Step 2: Commit**

```bash
git commit -am "feat(setup): add channel selection step (PTY, HTTP, Telegram)"
```

---

### Task 2.8: Write `config.toml` and `.env` from collected answers

**Files:**
- Create: `crates/araliya-bot/src/setup/writer.rs`
- Modify: `crates/araliya-bot/src/setup/mod.rs` — call writer at the end

**Step 1: Create `writer.rs` with `write_config()` and `write_env()`**

The writer generates TOML as a hand-built string (not via serde serialization)
so the output is human-readable with comments preserved.

```rust
// crates/araliya-bot/src/setup/writer.rs
use std::fs;
use std::path::Path;
use anyhow::Result;
use super::answers::Answers;

pub fn write_config(answers: &Answers, path: &Path) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(path))?;

    let axum_enabled     = answers.http_bind.is_some();
    let telegram_enabled = answers.enable_telegram;
    let bind             = answers.http_bind.as_deref().unwrap_or("127.0.0.1:8080");

    // Determine which agents to enable based on profile
    let (default_agent, agents_block) = render_agents_block(&answers.profile);

    let toml = format!(r#"# Generated by `araliya-bot setup` — edit freely.

[supervisor]
bot_name = "{bot_name}"
work_dir = "{work_dir}"
log_level = "info"

[comms.pty]
enabled = true

[comms.telegram]
enabled = {telegram_enabled}

[comms.axum_channel]
enabled = {axum_enabled}
bind = "{bind}"

[agents]
default = "{default_agent}"
debug_logging = false

{agents_block}

[llm]
default = "openai"

[llm.openai]
api_base_url = "{api_base}"
model = "{model}"
temperature = 0.2
timeout_seconds = 600
input_per_million_usd = 0.0
output_per_million_usd = 0.0

[memory.basic_session]

[ui.svui]
enabled = {axum_enabled}
static_dir = "frontend/build"
"#,
        bot_name   = answers.bot_name,
        work_dir   = answers.work_dir.display(),
        api_base   = answers.llm_api_base_url,
        model      = answers.llm_model,
    );

    fs::write(path, toml)?;
    Ok(())
}

pub fn write_env(answers: &Answers, path: &Path) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(path))?;

    let mut env = String::new();

    if !answers.llm_api_key.is_empty() {
        env.push_str(&format!("LLM_API_KEY={}\n", answers.llm_api_key));
    }
    if let Some(token) = &answers.telegram_token {
        env.push_str(&format!("TELEGRAM_BOT_TOKEN={}\n", token));
    }

    if path.exists() {
        // Append — don't overwrite existing keys
        let existing = fs::read_to_string(path)?;
        for line in env.lines() {
            let key = line.split('=').next().unwrap_or("");
            if !existing.contains(&format!("{key}=")) {
                fs::OpenOptions::new()
                    .append(true)
                    .open(path)?
                    .write_all(format!("\n{line}").as_bytes())?;
            }
        }
    } else {
        fs::write(path, env)?;
    }

    // Set file permissions to 0600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn render_agents_block(profile: &super::answers::BotProfile) -> (&'static str, String) {
    use super::answers::BotProfile::*;
    match profile {
        BasicChat => ("basic_chat", "[agents.basic_chat]\nenabled = true\n".into()),
        AgenticChat => ("agentic-chat", "[agents.chat]\nenabled = true\n\n[agents.agentic-chat]\nenabled = true\nmemory = [\"basic_session\"]\n".into()),
        Docs => ("docs", "[agents.chat]\nenabled = true\n\n[agents.docs]\nenabled = true\ndocsdir = \"docs/\"\nindex = \"index.md\"\nmemory = [\"basic_session\"]\n".into()),
        Newsroom => ("newsroom", "[agents.newsroom]\nenabled = true\nmemory = [\"basic_session\"]\n\n[agents.newsroom.gdelt_query]\nlookback_minutes = 60\nlimit = 50\n".into()),
        Custom | _ => ("echo", "".into()),
    }
}
```

**Step 2: Call writer at the end of `run()` in `mod.rs`**

```rust
// in run(), after all prompts:
use crate::setup::writer;
use std::io::Write; // for append

step("Writing configuration");

// Assemble answers
let answers = answers::Answers {
    bot_name: bot_name.clone(),
    work_dir: std::path::PathBuf::from(&work_dir_str),
    llm_provider: /* map provider_idx */ answers::LlmProvider::OpenAI,
    llm_api_key: llm_api_key.clone(),
    llm_model: llm_model.clone(),
    llm_api_base_url: llm_api_base_url.clone(),
    profile: /* map profile_idx */ answers::BotProfile::BasicChat,
    enable_telegram,
    telegram_token,
    http_bind,
};

let env_path = opts.config_path.with_file_name(".env");

writer::write_config(&answers, &opts.config_path)?;
ok(&format!("Config written: {}", opts.config_path.display()));

writer::write_env(&answers, &env_path)?;
ok(&format!("Secrets written: {}", env_path.display()));
```

**Step 3: Verify end-to-end**

```bash
cargo build -p araliya-bot
mkdir -p /tmp/araliya-test
./target/debug/araliya-bot setup --config /tmp/araliya-test/config.toml
cat /tmp/araliya-test/config.toml
cat /tmp/araliya-test/.env
```

Expected: valid TOML with correct sections, .env has the key, permissions 600.

**Step 4: Commit**

```bash
git commit -am "feat(setup): write config.toml and .env from wizard answers"
```

---

### Task 2.9: "Next steps" summary at the end of the wizard

**Files:**
- Modify: `crates/araliya-bot/src/setup/mod.rs` — final section of `run()`

**Step 1: Print actionable next-steps block**

```rust
use console::style;

println!();
println!("{}", style("─────────────────────────────────────────────").cyan());
println!("{}", style("  You're all set!").green().bold());
println!("{}", style("─────────────────────────────────────────────").cyan());
println!();
println!("  Start the bot (interactive PTY):");
println!("    {}", style("araliya-bot -i --config <config_path>").bold());
println!();
println!("  Start the bot (headless HTTP mode):");
println!("    {}", style("araliya-bot --config <config_path>").bold());
println!();
if enable_http {
    println!("  Open the web UI:");
    println!("    {}", style(format!("http://{}", http_bind.as_deref().unwrap_or("127.0.0.1:8080"))).bold());
    println!();
}
println!("  Edit config at any time:");
println!("    {}", style(opts.config_path.display()).bold());
println!();
```

**Step 2: Commit**

```bash
git commit -am "feat(setup): add next-steps summary at end of wizard"
```

---

## Phase 3 — `araliya-bot doctor` Subcommand

Lets users diagnose a broken setup without re-running the full wizard.

---

### Task 3.1: `doctor` subcommand skeleton

**Files:**
- Create: `crates/araliya-bot/src/setup/doctor.rs`
- Modify: `crates/araliya-bot/src/setup/mod.rs` — pub use + hook in main.rs

**Step 1: Create `doctor.rs`**

```rust
// crates/araliya-bot/src/setup/doctor.rs
use std::path::Path;
use console::style;

pub fn run(config_path: &Path) -> anyhow::Result<()> {
    println!("\n{}", style("── Araliya Bot Doctor").cyan().bold());
    println!();

    check("Config file exists",
        config_path.exists(),
        &format!("Missing: {}", config_path.display()));

    let env_path = config_path.with_file_name(".env");
    check(".env file exists",
        env_path.exists(),
        &format!("Missing: {} — run 'araliya-bot setup'", env_path.display()));

    check("LLM_API_KEY set",
        std::env::var("LLM_API_KEY").map(|v| !v.is_empty()).unwrap_or(false),
        "LLM_API_KEY is empty — check your .env file");

    // Parse config and check required sections present
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        check("[llm] section present", content.contains("[llm]"),
            "Config missing [llm] section");
        check("[agents] section present", content.contains("[agents]"),
            "Config missing [agents] section");
    }

    println!();
    Ok(())
}

fn check(label: &str, ok: bool, hint: &str) {
    if ok {
        println!("  {} {}", style("✓").green(), label);
    } else {
        println!("  {} {}",  style("✗").red(), label);
        println!("    {}", style(hint).yellow());
    }
}
```

**Step 2: Hook `doctor` into `main.rs` (same pattern as `setup`)**

```rust
#[cfg(feature = "setup")]
if std::env::args().any(|a| a == "doctor") {
    let config_path = /* same resolution as setup */;
    araliya_bot::setup::doctor::run(&config_path)?;
    return Ok(());
}
```

**Step 3: Verify**

```bash
./target/debug/araliya-bot doctor --config /tmp/araliya-test/config.toml
```

Expected: all checks shown with green ✓ or red ✗.

**Step 4: Commit**

```bash
git commit -am "feat(setup): add doctor subcommand for config validation"
```

---

## Phase 4 — Tests

### Task 4.1: Unit test for `writer::write_config`

**Files:**
- Modify: `crates/araliya-bot/src/setup/writer.rs` — add `#[cfg(test)]` block

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::answers::*;
    use tempfile::TempDir;

    fn dummy_answers() -> Answers {
        Answers {
            bot_name: "testbot".into(),
            work_dir: "/tmp/testbot".into(),
            llm_provider: LlmProvider::OpenAI,
            llm_api_key: "sk-test".into(),
            llm_model: "gpt-4o-mini".into(),
            llm_api_base_url: "https://api.openai.com/v1/chat/completions".into(),
            profile: BotProfile::BasicChat,
            enable_telegram: false,
            telegram_token: None,
            http_bind: Some("127.0.0.1:8080".into()),
        }
    }

    #[test]
    fn test_write_config_creates_valid_toml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&dummy_answers(), &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // Basic sanity checks
        assert!(content.contains("[supervisor]"));
        assert!(content.contains("bot_name = \"testbot\""));
        assert!(content.contains("[llm.openai]"));
        assert!(content.contains("[agents]"));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_write_env_sets_permissions() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        let mut answers = dummy_answers();
        answers.llm_api_key = "sk-secret".into();
        write_env(&answers, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("LLM_API_KEY=sk-secret"));
        // Check 0600 permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
    }
}
```

**Run:**

```bash
cargo test -p araliya-bot --features setup test_write_config
cargo test -p araliya-bot --features setup test_write_env
```

Expected: 2 tests pass.

**Commit:**

```bash
git commit -am "test(setup): unit tests for config and .env writer"
```

---

### Task 4.2: Fix existing 10 failing prompt tests

**Files:**
- Modify: `crates/araliya-bot/tests/test_prompts.rs`

The test file looks for `config/prompts/` (old path). Prompts moved to
`config/agents/_shared/`. Update the `prompts_dir()` helper:

```rust
fn prompts_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/agents/_shared")
}
```

Then update each test to use the actual filenames that exist in `_shared/`:
- `chat.md` → check `agent.md`
- `docs.md` → check `memory_and_tools.md`
- `memory_and_tools.md` → exists as-is
- `subagent.md` → check `subagent.md`
- `news_summary.txt` → the news prompt lives in `config/agents/news/` or similar

Run `ls config/agents/_shared/` and `ls config/agents/` to map exact names.
Update each `assert!(prompt_path("X").exists(), "X prompt file missing")` to
the correct filename.

**Run:**

```bash
cargo test -p araliya-bot
```

Expected: 14/14 tests pass.

**Commit:**

```bash
git commit -am "fix(tests): update test_prompts.rs to point at config/agents/_shared/"
```

---

## Phase 5 — Documentation

### Task 5.1: Write `docs/setup.md`

**Files:**
- Create: `docs/setup.md`

Document:
1. The curl one-liner
2. What the install script does (prereqs → download → binary → wizard)
3. Available env overrides (`ARALIYA_TIER`, `ARALIYA_VERSION`, `INSTALL_DIR`)
4. What the wizard asks and what it writes
5. Running `araliya-bot doctor` to validate
6. Manual install path (for users who don't want curl | bash)
7. Updating: re-run install script, it upgrades in place

**Commit:**

```bash
git commit -am "docs: add setup.md covering install script and wizard"
```

---

## Delivery Summary

After all phases are complete, the user experience is:

```
curl -fsSL https://<host>/install.sh | bash
```

Which runs:
1. Prereq check (curl/tar)
2. OS/arch detection
3. GitHub release version resolution
4. Binary download + extraction to ~/.local/bin/
5. Default config seeded to ~/.araliya/config/
6. Automatic handoff to `araliya-bot setup` for interactive wizard:
   - Bot name + work dir
   - LLM provider + API key + model
   - Agent profile (grouped select with hints)
   - Channels (PTY / HTTP / Telegram)
7. Writes config.toml + .env (0600)
8. Prints "next steps" with exact commands

At any time afterwards: `araliya-bot doctor` runs a config health check.

---

## Open Questions (decide before starting)

1. **Hosting**: Where will `install.sh` be served from? (GitHub raw? custom domain?)
2. **macOS support**: CI only builds linux. Should the install script offer
   `cargo install` as a fallback for macOS, or block it explicitly?
3. **Telegram wizard flow**: Should the wizard offer to test the Telegram token
   (call `getMe`) before writing it, so the user knows it's valid immediately?
4. **Config location**: Should the wizard write to `~/.araliya/config/config.toml`
   always, or ask the user for a custom path?
5. **Update path**: Phase 1 shows re-running install.sh updates the binary.
   Should there also be an `araliya-bot update` subcommand that does this
   from within the installed binary?
