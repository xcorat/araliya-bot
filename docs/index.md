# Araliya Docs

Welcome to the documentation portal.

Start here:

- [Quick Intro](quick-intro.md) — project overview and core concepts.
- [Getting Started](getting-started.md) — build, run, and first-run verification.
- [Configuration](configuration.md) — config files, runtime flags, and env vars.

UI notes:

- Svelte web UI: `frontend/svui` (served by bot HTTP channel).
- GPUI desktop UI: `araliya-gpui` binary (`cargo run --bin araliya-gpui --features ui-gpui`).

Deep dives:

- [Architecture Overview](architecture/overview.md)
- [UI Subsystem](architecture/subsystems/ui.md)
- [Identity](architecture/identity.md)
- [Agents](architecture/subsystems/agents.md)
- [Tools Subsystem](architecture/subsystems/tools.md)
- [Memory Subsystem](architecture/subsystems/memory.md) — typed value model, TmpStore, SessionHandle (v0.4.0: typed Value/Collection model, `TmpStore`, memory always enabled)

Operations:

- [Deployment](operations/deployment.md)
- [Monitoring](operations/monitoring.md)

Development:

- [Contribution Guide](development/contributing.md)
- [Testing](development/testing.md)

If you're new, read **Quick Intro** and then **Getting Started**.
