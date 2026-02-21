# GPUI Desktop Client — Development Guide

The optional native desktop client (`araliya-gpui`) is built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), Zed's GPU-accelerated UI framework. It runs as a separate binary alongside the main bot daemon and communicates with it over the HTTP API.

## System Dependencies (Linux)

GPUI on Linux links against several native system libraries that are **not** bundled by Cargo and must be present on the build host.

### XCB — X protocol C-language Binding

XCB is the low-level C library for the X Window System protocol. It replaces the older Xlib with a smaller, asynchronous interface. GPUI uses it to create windows and handle X events on Linux.

**Required package:** `libxcb-dev` (Debian/Ubuntu) — provides `libxcb.so` and headers.

### XKB — X Keyboard Extension

XKB (X Keyboard Extension) is the X11 subsystem that handles keyboard layouts, key maps, and modifier state (Shift, Ctrl, etc.). Two libraries are needed:

- **libxkbcommon** — a standalone XKB keymap compiler and state machine, used without any X connection (also works on Wayland).
- **libxkbcommon-x11** — extends libxkbcommon to load keymaps directly from an X server via XCB.

**Required packages:** `libxkbcommon-dev`, `libxkbcommon-x11-dev` (Debian/Ubuntu).

### Install all at once

```bash
# Debian / Ubuntu / Mint
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev

# Fedora / RHEL
sudo dnf install -y libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel

# Arch Linux
sudo pacman -S libxcb libxkbcommon libxkbcommon-x11
```

These are development (`-dev`) packages — they provide the `.so` symlinks and headers that the linker needs at build time. The runtime `.so` files are almost always already present on any desktop Linux system.

## Feature Flag

The GPUI binary is gated behind the `ui-gpui` Cargo feature:

```bash
# Check only (fast)
cargo check --bin araliya-gpui --features ui-gpui

# Build
cargo build --bin araliya-gpui --features ui-gpui

# Run
cargo run --bin araliya-gpui --features ui-gpui
```

## Running

The GPUI client connects to the bot's HTTP API. Start the bot daemon first:

```bash
# Terminal 1 — bot API (default: http://127.0.0.1:8080)
cargo run

# Terminal 2 — desktop client
cargo run --bin araliya-gpui --features ui-gpui
```

The client target URL defaults to `http://127.0.0.1:8080`. See `config/default.toml` for the relevant API address.

## Architecture Notes

- `gpui`'s `Application::run()` takes over the **main thread**, so the tokio runtime runs on a background `std::thread`.
- `Config` and `Identity` are loaded before the runtime starts and passed to the UI as a `UiSnapshot` (owned, no lifetimes).
- A shared `Arc<AtomicU8>` carries `BotStatus` so the status panel reflects the bot's lifecycle without holding locks.
- Source lives in `src/bin/araliya-gpui/`:
	- `main.rs` — app bootstrap and window wiring
	- `components.rs` — UI shell and panel rendering
	- `state.rs` — view/layout/session state
	- `api.rs` — HTTP API client + DTOs

## Current UI Framework (PRD-aligned basic shell)

The GPUI client now uses a basic shell mirroring the UI/UX PRD framework:

- **Zone A (Activity rail):** section switcher for `Chat`, `Memory`, `Tools`, `Status`, `Settings`, `Docs`
- **Zone B (Header):** app identity, active section context, health summary, panel toggles
- **Zone C (Panel row):**
	- Left panel: sessions list
	- Main panel: section content (chat and status implemented; others scaffolded)
	- Right panel: optional context panel scaffold
- **Zone D (Bottom bar):** compact session/message/mode summary

This keeps layout extensibility in place while preserving existing API-backed chat and status behavior.

## Responsive layout behavior

The GPUI shell now adapts to window width using a single responsive shell model:

- **Desktop** (`>= 1200px`): inline left sessions panel and inline right context panel.
- **Tablet** (`>= 860px` and `< 1200px`): compact shell with activity rail always visible; side panels open as focused drawers.
- **Compact** (`< 860px`): same drawer behavior as tablet with tighter content widths.

Current interaction model:

- Activity rail is always visible for section switching.
- Header toggles control Sessions and Context panel visibility.
- In tablet/compact modes, opening a side panel switches the center area into that panel view with a close action.

Layout preferences are persisted between runs in:

- `~/.config/araliya-bot/gpui-layout.json`

Persisted fields include:

- left/right panel open state
- left/right panel widths
- ISO-8601 `updated_at`

See [notes/gpui-plan.md](../../../notes/gpui-plan.md) for the original design notes.
