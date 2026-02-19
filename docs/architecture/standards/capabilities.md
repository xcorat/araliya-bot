# Capabilities Model

**Status:** Planned — typed state objects exist; supervisor-enforced permissions not yet implemented.

---

## Overview

The capabilities model governs what a component is allowed to do. Rather than giving components direct access to the raw `BusHandle` or filesystem, each subsystem exposes a **typed state object** that wraps only the operations that component class is permitted to perform. Supervisor-level permission enforcement is planned but not yet implemented.

---

## Current state: typed capability objects

Each subsystem constructs a typed state struct that hides the raw `BusHandle` and exposes only permitted operations:

| State type | Used by | Permitted operations |
|------------|---------|----------------------|
| `AgentsState` | `AgentPlugin` implementations | `complete_via_llm(channel_id, content)` |
| `CommsState` | `Component` implementations in the Comms subsystem | `send_message(channel_id, content)`, `report_event(CommsEvent)` |

The raw `BusHandle` is private within the owning module. Plugins cannot address arbitrary bus methods directly.

---

## Planned: supervisor permission enforcement

The following is the intended design — not yet implemented:

- The supervisor will maintain a permission table: `HashMap<prefix, AllowedMethods>`.
- When a subsystem or plugin sends a `BusMessage::Request`, the supervisor checks the caller's registered permissions before forwarding to the target handler.
- Permission grants are configured at startup; plugins cannot escalate their own permissions.
- A plugin requesting `fs_storage` access would receive a pre-scoped wrapper (e.g. a path-restricted write function) rather than raw filesystem access — analogous to the current `AgentsState`/`CommsState` pattern, generalized.
- System methods prefixed with `$/` are reserved and may only be invoked by the supervisor itself.

---

## Design principle

The current typed-state approach is the foundation of this model. Adding supervisor-level enforcement means instrumenting the dispatch loop in `supervisor/dispatch.rs` with a permission check — the state object pattern does not need to change.
