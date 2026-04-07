#!/usr/bin/env bash
# Araliya Bot — installer
# Usage: curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
# Env overrides: ARALIYA_TIER (minimal|default|full), ARALIYA_VERSION (vX.Y.Z),
#                INSTALL_DIR, ARALIYA_CONFIG_DIR, ARALIYA_WORK_DIR
set -euo pipefail

# ── defaults ──────────────────────────────────────────────────────────
REPO="xcorat/araliya-bot"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
# XDG: app config lives in ~/.config/araliya/
CONFIG_DIR="${ARALIYA_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/araliya}"
# Bot runtime data (identity keypair, sessions, memory) lives in ~/.araliya/
WORK_DIR="${ARALIYA_WORK_DIR:-$HOME/.araliya}"
TIER="${ARALIYA_TIER:-default}"
TMP=""

# ── colors ────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

banner() {
  printf "\n${BOLD}${CYAN}"
  printf "╔══════════════════════════════════════════════╗\n"
  printf "║        Araliya Bot  ·  Installer             ║\n"
  printf "╚══════════════════════════════════════════════╝${NC}\n\n"
}

step() { printf "\n${CYAN}── %s${NC}\n" "$1"; }
ok()   { printf "  ${GREEN}✓ %s${NC}\n" "$1"; }
warn() { printf "  ${YELLOW}⚠ %s${NC}\n" "$1"; }
err()  { printf "  ${RED}✗ %s${NC}\n"   "$1" >&2; }

# ── error + cleanup traps ─────────────────────────────────────────────
_cleanup() { [ -n "$TMP" ] && rm -rf "$TMP"; }
trap '_cleanup' EXIT
trap 'err "Installation failed at line ${LINENO}: ${BASH_COMMAND}"' ERR

need() {
  if ! command -v "$1" &>/dev/null; then
    err "Required tool '$1' not found. Please install it and re-run."
    exit 1
  fi
}

# ── use-case → tier selection (skipped when ARALIYA_TIER is pre-set) ──────────
pick_use_case() {
  if [[ -n "${ARALIYA_TIER:-}" ]]; then
    ok "Tier override: $ARALIYA_TIER (skipping use-case menu)"
    return
  fi

  printf "\n${BOLD}What do you want to use Araliya Bot for?${NC}\n\n"
  printf "  1) Basic chat           — direct LLM, no memory\n"
  printf "  2) Homebuilder          — AI-generated personal landing page\n"
  printf "  3) Docs / KG agent      — RAG over a local docs directory\n"
  printf "  4) Full feature set     — all built-in agents and tools\n"
  printf "  5) Build from source    — custom feature selection\n"
  printf "\n"

  while true; do
    printf "Enter choice [1-5]: "
    read -r choice </dev/tty
    case "$choice" in
      1) TIER="minimal";  ok "Basic chat → minimal tier";  break ;;
      2) TIER="default";  ok "Homebuilder → default tier"; break ;;
      3) TIER="default";  ok "Docs / KG agent → default tier"; break ;;
      4) TIER="full";     ok "Full feature set → full tier"; break ;;
      5) TIER="__source"; ok "Will build from source";      break ;;
      *) warn "Please enter a number between 1 and 5." ;;
    esac
  done
}

# ── build from source path ────────────────────────────────────────────────────
build_from_source() {
  step "Build from source"

  if ! command -v git &>/dev/null; then
    err "'git' is required to build from source. Install it and re-run."
    exit 1
  fi

  if ! command -v cargo &>/dev/null; then
    warn "'cargo' (Rust toolchain) not found."
    printf "  Install via rustup:  ${BOLD}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}\n"
    printf "  Then re-run this installer.\n"
    exit 1
  fi

  BUILD_DIR="${TMPDIR:-/tmp}/araliya-src-$$"
  ok "Cloning into $BUILD_DIR"
  git clone --depth=1 "https://github.com/${REPO}.git" "$BUILD_DIR"

  printf "\n${BOLD}Select features to include:${NC}\n\n"
  printf "  Agents (space-separated numbers, e.g. 1 3 5):\n"
  printf "    1) basic-chat      — direct LLM pass-through\n"
  printf "    2) chat            — session-aware multi-turn\n"
  printf "    3) homebuilder     — personal landing-page generator\n"
  printf "    4) docs            — RAG over local docs\n"
  printf "    5) docs-agent      — public-facing docs variant\n"
  printf "    6) webbuilder      — iterative Svelte page builder\n"
  printf "    7) agentic-chat    — dual-pass with tool use\n"
  printf "  Channels:\n"
  printf "    8) axum/http       — web UI + REST API\n"
  printf "    9) telegram        — Telegram bot channel\n"
  printf "\n"
  printf "Enter numbers (default: 1 2 8): "
  read -r feature_choices </dev/tty
  feature_choices="${feature_choices:-1 2 8}"

  FEATURES="subsystem-agents,subsystem-memory,subsystem-llm,subsystem-comms,channel-pty"
  for n in $feature_choices; do
    case "$n" in
      1) FEATURES="$FEATURES,plugin-basic-chat" ;;
      2) FEATURES="$FEATURES,plugin-chat" ;;
      3) FEATURES="$FEATURES,plugin-homebuilder,subsystem-runtimes,subsystem-ui,ui-svui" ;;
      4) FEATURES="$FEATURES,plugin-docs,idocstore" ;;
      5) FEATURES="$FEATURES,plugin-docs-agent,idocstore" ;;
      6) FEATURES="$FEATURES,plugin-webbuilder,subsystem-runtimes,subsystem-ui,ui-svui" ;;
      7) FEATURES="$FEATURES,plugin-agentic-chat" ;;
      8) FEATURES="$FEATURES,channel-axum,subsystem-ui,ui-svui" ;;
      9) FEATURES="$FEATURES,channel-telegram" ;;
      *) warn "Unknown option '$n' — skipped." ;;
    esac
  done

  ok "Building with features: $FEATURES"
  (cd "$BUILD_DIR" && cargo build --release --locked --bin araliya-bot --no-default-features --features "$FEATURES")

  mkdir -p "$INSTALL_DIR"
  cp "$BUILD_DIR/target/release/araliya-bot" "$INSTALL_DIR/araliya-bot"
  chmod +x "$INSTALL_DIR/araliya-bot"
  ok "Binary → $INSTALL_DIR/araliya-bot"

  # Seed config dir from the cloned repo
  mkdir -p "$CONFIG_DIR" "$WORK_DIR"
  if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
    cp "$BUILD_DIR/config/default.toml" "$CONFIG_DIR/config.toml"
    ok "Default config → $CONFIG_DIR/config.toml"
  else
    warn "Config already exists — skipping (run 'araliya-bot setup' to reconfigure)"
  fi

  rm -rf "$BUILD_DIR"
}

# ── download abstraction (curl or wget) ───────────────────────────────
setup_fetch() {
  if command -v curl &>/dev/null; then
    fetch()        { command curl -fsSL "$1" -o "$2"; }
    fetch_stdout() { command curl -fsSL "$1"; }
  elif command -v wget &>/dev/null; then
    fetch()        { wget -qO  "$2" "$1"; }
    fetch_stdout() { wget -qO- "$1"; }
  else
    err "Neither curl nor wget found. Install one and re-run."
    exit 1
  fi
}

# ── main ──────────────────────────────────────────────────────────────
main() {
  banner

  # ── prerequisites ────────────────────────────────────────────────
  step "Checking prerequisites"
  need tar
  setup_fetch
  ok "tar found"

  # ── use-case selection ───────────────────────────────────────────
  pick_use_case

  OS="$(uname -s)"
  ARCH_RAW="$(uname -m)"

  case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin)
      warn "macOS detected — pre-built binaries are Linux-only."
      warn "To build from source: cargo install --git https://github.com/${REPO}"
      exit 1
      ;;
    *)
      err "Unsupported OS: $OS"
      exit 1
      ;;
  esac

  case "$ARCH_RAW" in
    x86_64)        ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)
      err "Unsupported architecture: $ARCH_RAW"
      exit 1
      ;;
  esac

  ok "Platform: $PLATFORM / $ARCH"

  # ── source-build branch ──────────────────────────────────────────
  if [[ "$TIER" == "__source" ]]; then
    build_from_source

    # PATH check
    step "Checking PATH"
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
      warn "$INSTALL_DIR is not in your PATH."
      echo "  Add this line to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
      printf "    ${BOLD}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}\n\n"
    else
      ok "araliya-bot is on your PATH"
    fi

    step "Running setup wizard"
    echo "  (Configure bot name, LLM provider, agent profile, and channels)"
    echo ""
    "$INSTALL_DIR/araliya-bot" setup \
      --config "$CONFIG_DIR/config.toml" \
      --env    "$CONFIG_DIR/.env" \
      --work-dir "$WORK_DIR"

    printf "\n${GREEN}${BOLD}✓ Araliya Bot is ready!${NC}\n\n"
    echo "  Start (interactive terminal):"
    printf "    ${BOLD}araliya-bot -i -f $CONFIG_DIR/config.toml${NC}\n\n"
    echo "  Validate config:"
    printf "    ${BOLD}araliya-bot doctor -f $CONFIG_DIR/config.toml${NC}\n\n"
    return
  fi

  case "$TIER" in
    minimal|default|full) ;;
    *)
      warn "Unknown tier '$TIER' — falling back to 'default'."
      TIER="default"
      ;;
  esac
  ok "Tier: $TIER"

  # ── resolve version ──────────────────────────────────────────────
  step "Resolving version"

  if [[ -n "${ARALIYA_VERSION:-}" ]]; then
    VERSION="$ARALIYA_VERSION"
    ok "Pinned: $VERSION"
  else
    VERSION="$(fetch_stdout "https://api.github.com/repos/${REPO}/releases/latest" \
               | grep '"tag_name"' \
               | head -1 \
               | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    if [[ -z "$VERSION" ]]; then
      err "Could not resolve latest version."
      err "Set ARALIYA_VERSION=vX.Y.Z and re-run, or check your network."
      exit 1
    fi
    ok "Latest: $VERSION"
  fi

  # ── download binary ──────────────────────────────────────────────
  step "Downloading araliya-bot ($TIER / $ARCH)"

  ARCHIVE="araliya-bot-${VERSION}-${TIER}-${ARCH}-unknown-linux-gnu.tar.gz"
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
  TMP="$(mktemp -d /tmp/araliya-XXXXXX)"

  fetch "$URL" "$TMP/$ARCHIVE" || {
    err "Download failed: $URL"
    err "Check that version $VERSION has a $TIER/$ARCH build."
    rm -rf "$TMP"
    exit 1
  }
  ok "Downloaded to $TMP/$ARCHIVE"
  ok "Size: $(du -sh "$TMP/$ARCHIVE" | cut -f1)"

  gzip -t "$TMP/$ARCHIVE" \
    || { err "Downloaded file is not a valid gzip archive."; \
         err "The URL may have returned a 404 or error page: $URL"; \
         exit 1; }
  ok "Archive integrity OK"

  # ── extract + install binary ─────────────────────────────────────
  step "Installing binary"

  ARCHIVE_LISTING="$(tar -tzf "$TMP/$ARCHIVE")" \
    || { err "Failed to read archive contents: $ARCHIVE"; exit 1; }
  EXTRACTED="$(printf '%s\n' "$ARCHIVE_LISTING" | head -1 | cut -d/ -f1)"
  [[ -n "$EXTRACTED" ]] \
    || { err "Archive appears empty or malformed: $ARCHIVE"; exit 1; }
  ok "Archive root: $EXTRACTED"

  tar -xzf "$TMP/$ARCHIVE" -C "$TMP" \
    || { err "Failed to extract archive: $ARCHIVE"; exit 1; }
  ok "Extracted to $TMP/$EXTRACTED"

  BIN_SRC="$TMP/$EXTRACTED/bin/araliya-bot"
  [[ -f "$BIN_SRC" ]] \
    || { err "Binary not found in archive at: $BIN_SRC"; tar -tzf "$TMP/$ARCHIVE" >&2 || true; exit 1; }

  mkdir -p "$INSTALL_DIR" \
    || { err "Failed to create install directory: $INSTALL_DIR"; exit 1; }

  cp "$BIN_SRC" "$INSTALL_DIR/araliya-bot" \
    || { err "Failed to copy binary to $INSTALL_DIR"; exit 1; }

  chmod +x "$INSTALL_DIR/araliya-bot" \
    || { err "Failed to set permissions on $INSTALL_DIR/araliya-bot"; exit 1; }

  ok "Binary installed → $INSTALL_DIR/araliya-bot"

  # ── seed default config (skip if user already has one) ───────────
  step "Preparing config directory"

  mkdir -p "$CONFIG_DIR" \
    || { err "Failed to create config directory: $CONFIG_DIR"; exit 1; }
  mkdir -p "$WORK_DIR" \
    || { err "Failed to create work directory: $WORK_DIR"; exit 1; }

  # Always copy default.toml — tier configs reference it via [meta] base = "default.toml"
  cp "$TMP/$EXTRACTED/config/default.toml" "$CONFIG_DIR/default.toml" \
    || { err "Failed to copy default.toml to $CONFIG_DIR/default.toml"; exit 1; }
  ok "Base config → $CONFIG_DIR/default.toml"

  if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
    # Copy the tier-matched config as the user-editable overlay
    TIER_CONFIG="$TMP/$EXTRACTED/config/${TIER}.toml"
    if [[ -f "$TIER_CONFIG" ]]; then
      cp "$TIER_CONFIG" "$CONFIG_DIR/config.toml" \
        || { err "Failed to copy tier config to $CONFIG_DIR/config.toml"; exit 1; }
    else
      cp "$TMP/$EXTRACTED/config/default.toml" "$CONFIG_DIR/config.toml" \
        || { err "Failed to copy default config to $CONFIG_DIR/config.toml"; exit 1; }
    fi
    ok "Tier config → $CONFIG_DIR/config.toml"
  else
    warn "Config already exists — skipping (run 'araliya-bot setup' to reconfigure)"
  fi

  # ── PATH check ───────────────────────────────────────────────────
  step "Checking PATH"

  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    warn "$INSTALL_DIR is not in your PATH."
    echo "  Add this line to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    printf "    ${BOLD}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}\n"
    echo ""
  else
    ok "araliya-bot is on your PATH"
  fi

  # ── done ─────────────────────────────────────────────────────────
  printf "\n${GREEN}${BOLD}✓ Installation complete!${NC}\n\n"
  printf "  ${BOLD}Next steps:${NC}\n\n"
  printf "  1. Configure the bot (interactive setup):\n"
  printf "     ${BOLD}araliya-bot setup -f $CONFIG_DIR/config.toml${NC}\n\n"
  printf "  2. Start (interactive terminal):\n"
  printf "     ${BOLD}araliya-bot -i -f $CONFIG_DIR/config.toml${NC}\n\n"
  printf "  3. Start (headless — serves web UI + API):\n"
  printf "     ${BOLD}araliya-bot -f $CONFIG_DIR/config.toml${NC}\n\n"
  printf "  ${BOLD}Other commands:${NC}\n\n"
  printf "  • Validate config:\n"
  printf "    ${BOLD}araliya-bot doctor -f $CONFIG_DIR/config.toml${NC}\n\n"
  printf "  • Reconfigure at any time:\n"
  printf "    ${BOLD}araliya-bot setup -f $CONFIG_DIR/config.toml${NC}\n\n"
  printf "  ${BOLD}Locations:${NC}\n\n"
  printf "  • Binary: ${BOLD}$INSTALL_DIR/araliya-bot${NC}\n"
  printf "  • Config: ${BOLD}$CONFIG_DIR/config.toml${NC}\n"
  printf "  • Secrets: ${BOLD}$CONFIG_DIR/.env${NC}\n"
  printf "  • Runtime data: ${BOLD}$WORK_DIR${NC}\n\n"
  printf "  ${BOLD}Uninstall:${NC}\n\n"
  printf "  • Remove binary only:\n"
  printf "    ${BOLD}curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/uninstall.sh | bash${NC}\n\n"
  printf "  • Remove everything (with prompts):\n"
  printf "    ${BOLD}curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/uninstall.sh | bash -s -- --purge${NC}\n\n"
}

main
