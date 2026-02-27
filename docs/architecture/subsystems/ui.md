# UI Subsystem

**Status:** v0.2.6 — `UiServe` trait · `svui` backend · static file serving with SPA fallback · built-in placeholder page · route-backed Status SPA panes.

---

## Overview

The UI subsystem provides display-oriented interface backends. Unlike comms or agents, it does **not** run independent tasks. Instead it constructs a `UiServeHandle` — a trait-object that the HTTP channel calls per-request to serve static assets or rendered pages.

Each backend (e.g. *svui*) implements `UiServe` and is selected at startup based on config. Only one backend is active at a time.

---

## Backends

### svui — Implemented

Svelte-based web UI backend. Serves static files from a build directory, or a built-in placeholder page when no build is available.

**Behaviour:**

| Condition | Result |
|-----------|--------|
| `static_dir` configured and exists | Files served from disk; SPA fallback to `index.html` for non-asset paths |
| `static_dir` absent or missing | Built-in placeholder HTML page served for `/` and `/index.html` |
| Path contains `..` | Rejected with 400 Bad Request |

MIME types are inferred from file extensions (html, css, js, svg, png, woff2, wasm, etc.).

**Source:** `src/subsystems/ui/svui.rs`

### SVUI route model (frontend)

The web UI lives in `frontend/svui` and uses SvelteKit (SPA mode via static adapter fallback). The shell and status area are now split into nested layouts so only the status main pane changes while keeping the status sidebar context.

Current key paths:

| Path | Purpose |
|------|---------|
| `/ui/` | Chat page |
| `/ui/status` | Status overview main pane |
| `/ui/status/[nodeId]` | Status component detail pane |
| `/ui/status/[nodeId]/details` | Explicit details pane |
| `/ui/status/[nodeId]/memory` | Memory inspector pane (MVP) |
| `/ui/docs/...` | Documentation view |

Status memory inspector MVP notes:

- Displays enabled store types for the selected agent.
- Displays files grouped by session (session-scoped folders), not a single flattened file list.
- Opens an inspector card below the lists when selecting a store or file link.
- File content preview is intentionally deferred; current MVP shows metadata and working-memory preview where available.

---

## Architecture

### Module layout

```
src/
  subsystems/
    ui/
      mod.rs    — UiServe trait, UiServeHandle type, start(config) → Option<UiServeHandle>
      svui.rs   — SvuiBackend: UiServe
```

### Integration with HTTP channel

The UI subsystem is a **provider**, not a runtime component. `ui::start()` builds the active backend and returns an `Arc<dyn UiServe>`. This handle is passed to `comms::start()`, which injects it into the `HttpChannel`.

The HTTP channel dispatches requests as follows:

```
GET /api/health  → management bus (API route)
GET /api/*       → future API routes
GET /anything    → ui_handle.serve("/anything")  → static file or SPA fallback
GET /anything    → 404 (if no UI backend or serve returns None)
```

When the `subsystem-ui` feature is disabled at compile time, the HTTP channel has no UI handle and all non-API paths return 404.

---

## Config

```toml
[ui.svui]
enabled = true
# static_dir = "frontend/build"
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Whether the svui backend is loaded. |
| `static_dir` | (none) | Path to the static build directory. If absent, built-in placeholder is served. |

---

## Features

| Feature | Requires | Description |
|---------|----------|-------------|
| `subsystem-ui` | — | UI subsystem scaffolding. |
| `ui-svui` | `subsystem-ui` | Svelte UI backend. |

Both are included in the default feature set.
