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
