# Agent-scoped session ownership (2026-02-26)

## Summary

Implemented scoped session handling for agent conversations so each agent can own and reuse its own session state.

## Behavior changes

- `POST /api/message` now accepts optional `agent_id`.
- When `agent_id` is provided, comms routes message requests to `agents/{agent_id}`.
- `chat` plugin now creates/loads sessions under the chat agent identity directory (`memory/agents/.../sessions/...`) instead of the global sessions index.
- Session query payloads now support optional `agent_id` for scoped lookups on backend bus routes.

## Compatibility

- Existing global sessions remain untouched (no migration).
- Existing callers that omit `agent_id` continue using the default/global routing behavior.

## Follow-ups

- Add explicit HTTP query/body support for scoped session detail/memory/files endpoints if UI needs agent-scoped inspection.
- Update architecture docs in `docs/architecture/subsystems/memory.md` and `docs/architecture/subsystems/agents.md` with the new ownership semantics.
