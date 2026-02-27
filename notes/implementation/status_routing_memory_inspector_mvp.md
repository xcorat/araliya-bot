# Status routing + memory inspector MVP

**Status:** Implemented — 2026-02-26

## Scope delivered

- Moved global app chrome ownership (top header + footer) into the root frontend layout.
- Split status area into nested route/layout structure so status sidebar context stays mounted and only the main pane changes.
- Added route-backed status panes:
  - `/ui/status`
  - `/ui/status/[nodeId]`
  - `/ui/status/[nodeId]/[pane]` (currently `details`, `memory`)
- Added memory inspector MVP for `/ui/status/[nodeId]/memory`.

## Memory inspector MVP behavior

- Uses `agent_id` / node id route segment as source of truth for selected target.
- Shows enabled store types derived from sessions and agent metadata.
- Shows session files grouped per session (session-scoped storage), with each session rendered separately.
- Clicking a store/file opens an inspector card below the list.
- Current inspection depth:
  - working-memory preview available from existing API
  - file metadata available
  - file content preview deferred to later phase

## Technical notes

- Dynamic status routes are marked non-prerenderable to avoid static prerender crawl failures under SvelteKit static fallback mode.
- Existing `ComponentTreeNode` runes/type warnings were cleaned so frontend checks are currently clean.

## Deferred items

- Rich per-store browsing and file-content viewer.
- Additional pane types beyond `details` and `memory`.
- Optional collapse/expand UX for per-session file groups.
