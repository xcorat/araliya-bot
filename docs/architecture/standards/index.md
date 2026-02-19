# Standards & Protocols

This section contains normative specifications for the fundamental contracts that all components in the araliya-bot architecture must follow. These are reference documents — they describe what *must* be true for a component to integrate correctly, not how any one subsystem happens to work internally.

---

## Documents

| Spec | What it covers | Status |
|------|---------------|--------|
| [Bus Protocol](bus-protocol.md) | Method naming, `BusMessage` variants, payload enum, error codes, `BusHandle` API, `BusHandler` registration | Implemented |
| [Component Runtime](runtime.md) | `Component` trait, `ComponentFuture`, `spawn_components`, cancellation / fail-fast model | Implemented |
| [Plugin Interfaces](plugin-interfaces.md) | `AgentPlugin`, `BusHandler`, `LlmProvider` enum-dispatch extension pattern | Implemented |
| [Capabilities Model](capabilities.md) | Typed capability objects, planned permission enforcement | Planned |

---

## Why a standards section?

The subsystem docs under `architecture/subsystems/` describe what each subsystem does. This section describes the *shared contracts* those subsystems depend on — things like the event bus wire format, the component lifecycle, and how extension points are defined. Any contributor adding a new subsystem, plugin, or provider should read the relevant spec here before writing code.
