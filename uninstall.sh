#!/usr/bin/env bash
# Araliya Bot — uninstaller
# Usage: bash uninstall.sh [--purge]
#        curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/uninstall.sh | bash [--purge]
# Env overrides: INSTALL_DIR, ARALIYA_CONFIG_DIR, ARALIYA_WORK_DIR

set -euo pipefail

# ── defaults ──────────────────────────────────────────────────────────
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="${ARALIYA_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/araliya}"
WORK_DIR="${ARALIYA_WORK_DIR:-$HOME/.araliya}"
PURGE=false

# ── colors ────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

step() { printf "\n${CYAN}── %s${NC}\n" "$1"; }
ok()   { printf "  ${GREEN}✓ %s${NC}\n" "$1"; }
warn() { printf "  ${YELLOW}⚠ %s${NC}\n" "$1"; }
err()  { printf "  ${RED}✗ %s${NC}\n" "$1" >&2; }

# ── error + cleanup traps ─────────────────────────────────────────────
trap 'err "Uninstall failed at line ${LINENO}: ${BASH_COMMAND}"' ERR

# ── usage ─────────────────────────────────────────────────────────────
usage() {
  printf "\n${BOLD}${CYAN}Araliya Bot  ·  Uninstaller${NC}\n\n"
  printf "Usage: bash uninstall.sh [OPTIONS]\n\n"
  printf "Options:\n"
  printf "  --purge         Remove config files and runtime data\n"
  printf "  --help, -h      Show this help message\n\n"
  printf "By default, only the binary is removed.\n"
  printf "Config files and runtime data are preserved.\n\n"
}

# ── parse flags ───────────────────────────────────────────────────────
for arg in "$@"; do
  case "$arg" in
    --purge) PURGE=true ;;
    --help|-h) usage; exit 0 ;;
    *) err "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

# ── remove binary ─────────────────────────────────────────────────────
step "Removing binary"

if [[ -f "$INSTALL_DIR/araliya-bot" ]]; then
  rm -f "$INSTALL_DIR/araliya-bot" \
    || { err "Failed to remove $INSTALL_DIR/araliya-bot"; exit 1; }
  ok "Removed $INSTALL_DIR/araliya-bot"
else
  warn "Binary not found at $INSTALL_DIR/araliya-bot"
fi

# ── remove auto-generated config ──────────────────────────────────────
step "Removing auto-generated config"

if [[ -f "$CONFIG_DIR/default.toml" ]]; then
  rm -f "$CONFIG_DIR/default.toml" \
    || { err "Failed to remove $CONFIG_DIR/default.toml"; exit 1; }
  ok "Removed $CONFIG_DIR/default.toml"
fi

# ── handle purge ──────────────────────────────────────────────────────
if [[ "$PURGE" == "true" ]]; then
  # Prompt: remove config directory
  if [[ -d "$CONFIG_DIR" ]]; then
    step "Remove config directory?"
    printf "  ${BOLD}$CONFIG_DIR${NC}\n"
    printf "  (contains config.toml, .env, and other settings)\n\n"
    printf "  Remove? [y/N] "
    read -r -t 30 response < /dev/tty || response="N"

    if [[ "${response:0:1}" =~ [yY] ]]; then
      rm -rf "$CONFIG_DIR" \
        || { err "Failed to remove $CONFIG_DIR"; exit 1; }
      ok "Removed $CONFIG_DIR"
    else
      warn "Config directory preserved"
    fi
  fi

  # Prompt: remove runtime data
  if [[ -d "$WORK_DIR" ]]; then
    step "Remove runtime data?"
    printf "  ${BOLD}$WORK_DIR${NC}\n"
    printf "  (contains identity keypair, sessions, and memory)\n\n"
    printf "  Remove? [y/N] "
    read -r -t 30 response < /dev/tty || response="N"

    if [[ "${response:0:1}" =~ [yY] ]]; then
      rm -rf "$WORK_DIR" \
        || { err "Failed to remove $WORK_DIR"; exit 1; }
      ok "Removed $WORK_DIR"
    else
      warn "Runtime data preserved"
    fi
  fi
else
  # Not purging — just inform user
  printf "\n${CYAN}── Summary${NC}\n"
  if [[ -d "$CONFIG_DIR" ]] || [[ -d "$WORK_DIR" ]]; then
    printf "  ${YELLOW}⚠ Config and runtime data preserved.${NC}\n\n"
    printf "  To also remove:\n"
    printf "    ${BOLD}curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/uninstall.sh | bash -s -- --purge${NC}\n"
  fi
fi

printf "\n${GREEN}${BOLD}✓ Uninstall complete.${NC}\n\n"
