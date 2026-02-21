# GPUI client bootstrap (separate binary)

Date: 2026-02-21

## Summary
- Added a new optional GPUI desktop client binary: `araliya-gpui`.
- Kept it isolated from the default bot binary via feature flag `ui-gpui`.
- Implemented baseline parity with Svelte basics:
  - health status
  - sessions sidebar
  - chat history for selected session
  - chat input/send flow

## PRD shell refactor (basic framework)
- Refactored `araliya-gpui` to use the same high-level shell zones described in UI/UX PRD:
  - activity rail (zone A)
  - header/context bar (zone B)
  - panel row: left panel, main content, optional right panel (zone C)
  - bottom status bar (zone D)
- Added activity-section state and panel visibility state in `state.rs`.
- Added section switching for: Chat, Memory, Tools, Status, Settings, Docs.
- Kept existing API-backed flows intact inside the new shell:
  - health fetch
  - sessions list + select
  - message send and transcript rendering
- Introduced placeholder main-panel views for Memory, Tools, Settings, Docs to establish a stable extensible frame without adding extra behavior.

## Responsive shell implementation (single adaptive model)
- Added width-based responsive layout modes in GPUI state:
  - Desktop (`>= 1200px`)
  - Tablet (`>= 860px` and `< 1200px`)
  - Compact (`< 860px`)
- Switched shell behavior to mode-aware rendering while keeping one render tree:
  - activity rail stays visible in all modes
  - desktop keeps inline left/right side panels
  - tablet/compact use drawer-style focused panels for Sessions/Context
- Updated chat bubble constraints to mode-aware max widths for better readability at narrow sizes.
- Added local layout preference persistence for panel visibility/width in:
  - `~/.config/araliya-bot/gpui-layout.json`
- Metadata field `updated_at` is written in ISO-8601 format.

## Session totals in context panel
- Added session spend read path in memory handle (`read_spend`) to expose persisted `spend.json` totals safely.
- Extended `agents/sessions/detail` JSON response with `session_usage_totals`:
  - `prompt_tokens`
  - `completion_tokens`
  - `total_tokens`
  - `estimated_cost_usd`
- Updated GPUI API/state to store `session_usage_totals` and render values in right context panel.
- Added fallback refresh path after send: if message response lacks totals, client re-reads session detail and updates totals.

## Paths
- `src/bin/araliya-gpui/main.rs`
- `src/bin/araliya-gpui/components.rs`
- `src/bin/araliya-gpui/state.rs`
- `src/bin/araliya-gpui/api.rs`
- `Cargo.toml`

## Build
- Compile GPUI client:
  - `cargo check --bin araliya-gpui --features ui-gpui`
- Run GPUI client:
  - `cargo run --bin araliya-gpui --features ui-gpui`

## Notes
- Uses backend HTTP endpoints exposed by bot API:
  - `GET /api/health`
  - `GET /api/sessions`
  - `GET /api/session/:id`
  - `POST /api/message`
- Current default base URL in client is `http://127.0.0.1:8080`.
