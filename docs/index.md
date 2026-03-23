# Araliya Docs

Welcome to the documentation portal.

Start here:

- [Quick Intro](quick-intro.md) — project overview and core concepts.
- [Getting Started](getting-started.md) — install, build, run, and first-run verification.
- [Configuration](configuration.md) — config files, runtime flags, and env vars.

Install in one line (Linux x86\_64/aarch64):

```bash
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
araliya-bot setup   # interactive wizard — LLM provider, agent profile, channels
araliya-bot doctor  # validate config before first run
```

UI notes:

- Svelte web UI: `frontend/svui` (served by bot HTTP channel).
- GPUI desktop UI: `araliya-gpui` binary (`cargo run --bin araliya-gpui --features ui-gpui`).

Deep dives:

- [Architecture Overview](architecture/overview.md)
- [UI Subsystem](architecture/subsystems/ui.md)
- [Identity](architecture/identity.md)
- [Agents](architecture/subsystems/agents.md) — runtime classes, agent families, orchestration, routing, session queries
- [Tools Subsystem](architecture/subsystems/tools.md)
- [Memory Subsystem](architecture/subsystems/memory.md) — sessions, transcripts, KV store, agent stores, spend accounting

Operations:

- [Deployment](operations/deployment.md)
- [Monitoring](operations/monitoring.md)

Development:

- [Contribution Guide](development/contributing.md)
- [Testing](development/testing.md)

If you're new, read **Quick Intro** and then **Getting Started**.
