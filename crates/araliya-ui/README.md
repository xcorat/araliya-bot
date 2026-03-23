# araliya-ui

UI backends for the Araliya bot. Provides three compile-time optional backends selected by Cargo feature flags.

## Backends

### `ui-svui` — SvelteKit web UI

Serves the compiled SvelteKit frontend as static files over the bot's Axum HTTP channel. Implements the `UiServe` trait from `araliya-core` so any HTTP channel can delegate non-API requests to it.

```bash
cargo build -p araliya-ui --features ui-svui
```

Files are served from the path configured in `[ui.svui] static_dir`. The SvelteKit app is built separately:

```bash
cd frontend/svui
pnpm install && pnpm build   # output → frontend/build/
```

### `ui-gpui` — GPUI native desktop client

Optional GPU-accelerated desktop client built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Zed's UI framework). The client runs as a separate binary (`araliya-gpui`) and communicates with the bot daemon over HTTP — it does not share in-process state.

```bash
cargo build --bin araliya-gpui --features ui-gpui
cargo run   --bin araliya-gpui --features ui-gpui
```

The bot daemon must be running first (default API: `http://127.0.0.1:8080`).

#### System prerequisites (Linux)

GPUI links against native X11/XCB system libraries that Cargo does not bundle. Install the development packages before building:

```bash
# Debian / Ubuntu / Mint
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev

# Fedora / RHEL
sudo dnf install -y libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel

# Arch Linux
sudo pacman -S libxcb libxkbcommon libxkbcommon-x11
```

These are `-dev` packages (`.so` symlinks + headers for the linker). The runtime `.so` files are typically present on any desktop Linux system already.

**What each library provides:**

| Library | Purpose |
|---------|---------|
| `libxcb` | Low-level C binding for the X Window System protocol — window creation and X event handling |
| `libxkbcommon` | Standalone XKB keymap compiler and keyboard state machine (also used on Wayland) |
| `libxkbcommon-x11` | Extends libxkbcommon to load keymaps directly from an X server via XCB |

### `ui-beacon` — floating desktop beacon widget

A minimal always-on-top transparent widget rendered via [vello](https://github.com/linebender/vello) (2D GPU vector renderer) over [wgpu](https://wgpu.rs/). No widget tree — every pixel is drawn directly into a `vello::Scene`. Acts as a persistent status indicator and launcher for `araliya-gpui`.

```bash
cargo build --bin araliya-beacon --features ui-beacon
cargo run   --bin araliya-beacon --features ui-beacon
```

**Visual:** a single hex (230×230 px borderless transparent window). Hover or click to reveal three control hexes: Close, UI (launches `araliya-gpui`), Settings. Click-drag to reposition. Click to toggle pin.

The same X11/XCB system libraries required by `ui-gpui` are also needed for `ui-beacon` (winit depends on them). Install them as shown above.

## Source layout

```
crates/araliya-ui/src/
├── lib.rs              start() → Option<UiServeHandle>; re-exports UiServe, UiServeHandle
├── svui.rs             SvuiBackend: serves static files or built-in placeholder  (ui-svui)
├── gpui/               GPUI desktop client                                        (ui-gpui)
│   ├── mod.rs          run() entry point, GpuiAssets, app/window bootstrap
│   ├── api.rs          HTTP API client + DTOs (ApiClient, HealthResponse, …)
│   ├── state.rs        AppState, LayoutState, layout persistence (~/.config/araliya-bot/)
│   ├── components.rs   AppView (Render impl) — activity rail, panels, chat, status
│   ├── canvas_scene.rs CanvasGeometry — polygon geometry and hit-test helpers
│   └── icons/          Embedded SVG icons (loaded via AssetSource)
└── beacon/             Floating beacon widget                                     (ui-beacon)
    ├── mod.rs          run() entry point, BeaconApp, winit ApplicationHandler
    ├── scene.rs        Vello scene builder — hex geometry, hit-testing, rendering
    └── ipc.rs          Unix-domain-socket client — sends commands to the daemon
```

Both binary entry points are thin shims in `araliya-bot`:

```rust
// src/bin/araliya-gpui/main.rs
fn main() { araliya_ui::gpui::run(); }

// src/bin/araliya-beacon/main.rs
fn main() { araliya_ui::beacon::run(); }
```

## UI shell overview (GPUI client)

The desktop client implements a responsive four-zone shell:

- **Activity rail** — section switcher: Chat, Memory, Tools, Status, Settings, Docs
- **Header** — app identity, active section, health summary, panel and surface toggles
- **Panel row** — sessions sidebar (left), section content (center), context panel (right)
- **Status bar** — compact session / message count / layout mode summary

**Surface modes:** the center panel toggles between `Shell` (section content) and `Canvas` (GPUI polygon scene built from `canvas_scene.rs`).

**Responsive breakpoints:**

| Width | Mode | Panel behaviour |
|-------|------|----------------|
| ≥ 1200 px | Desktop | Side panels inline |
| ≥ 860 px | Tablet | Side panels as drawers |
| < 860 px | Compact | Side panels as drawers, tighter widths |

Layout preferences (panel open/width state) persist to `~/.config/araliya-bot/gpui-layout.json`.

## Feature flags

| Flag | Enables |
|------|---------|
| `ui-svui` | `SvuiBackend`, static file serving via `UiServe` |
| `ui-gpui` | Full GPUI desktop client — requires X11/XCB system libraries on Linux |
| `ui-beacon` | Floating beacon widget (vello/wgpu/winit) — requires X11/XCB system libraries on Linux |

All features are independent and can be combined. None is included in `default`.
