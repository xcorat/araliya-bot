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
err()  { printf "  ${RED}✗ %s${NC}\n"   "$1"; }

need() {
  if ! command -v "$1" &>/dev/null; then
    err "Required tool '$1' not found. Please install it and re-run."
    exit 1
  fi
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
  ok "Downloaded $ARCHIVE"

  # ── extract + install binary ─────────────────────────────────────
  step "Installing binary"

  # Get top-level dir name from the archive
  EXTRACTED="$(tar -tzf "$TMP/$ARCHIVE" | head -1 | cut -d/ -f1)"
  tar -xzf "$TMP/$ARCHIVE" -C "$TMP"

  mkdir -p "$INSTALL_DIR"
  cp "$TMP/$EXTRACTED/bin/araliya-bot" "$INSTALL_DIR/araliya-bot"
  chmod +x "$INSTALL_DIR/araliya-bot"
  ok "Binary → $INSTALL_DIR/araliya-bot"

  # ── seed default config (skip if user already has one) ───────────
  step "Preparing config directory"

  mkdir -p "$CONFIG_DIR"
  mkdir -p "$WORK_DIR"

  if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
    # Copy the tier-matched default config out of the archive
    TIER_CONFIG="$TMP/$EXTRACTED/config/${TIER}.toml"
    if [[ -f "$TIER_CONFIG" ]]; then
      cp "$TIER_CONFIG" "$CONFIG_DIR/config.toml"
    else
      cp "$TMP/$EXTRACTED/config/default.toml" "$CONFIG_DIR/config.toml"
    fi
    ok "Default config → $CONFIG_DIR/config.toml"
  else
    warn "Config already exists — skipping (run 'araliya-bot setup' to reconfigure)"
  fi

  rm -rf "$TMP"

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

  # ── launch setup wizard ──────────────────────────────────────────
  step "Running setup wizard"
  echo "  (Configure bot name, LLM provider, agent profile, and channels)"
  echo ""

  "$INSTALL_DIR/araliya-bot" setup \
    --config "$CONFIG_DIR/config.toml" \
    --env    "$CONFIG_DIR/.env" \
    --work-dir "$WORK_DIR"

  # ── done ─────────────────────────────────────────────────────────
  printf "\n${GREEN}${BOLD}✓ Araliya Bot is ready!${NC}\n\n"
  echo "  Start (interactive terminal):"
  printf "    ${BOLD}araliya-bot -i -f $CONFIG_DIR/config.toml${NC}\n\n"
  echo "  Start (headless / HTTP API):"
  printf "    ${BOLD}araliya-bot -f $CONFIG_DIR/config.toml${NC}\n\n"
  echo "  Validate config at any time:"
  printf "    ${BOLD}araliya-bot doctor -f $CONFIG_DIR/config.toml${NC}\n\n"
}

main
