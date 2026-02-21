# News Agent MVP

Date: 2026-02-21

## Summary

Implemented a minimal `news-agent` plugin in the agents subsystem.

- Agent id: `news-agent`
- Actions: `handle` (default), `read`
- Tool call: `tools/execute` with `tool = "newsmail_aggregator"`, `action = "get"`
- Args: `{}` (defaults-only)
- Response: raw `ToolResponse.data_json` passed through as `CommsMessage.content`
- Memory: available through `AgentsState`, intentionally unused in MVP

## Files touched

- `src/subsystems/agents/news.rs`
- `src/subsystems/agents/mod.rs` (feature-gated test)
- `config/default.toml`
- `config/news.toml`
- `docs/configuration.md`
- `docs/architecture/subsystems/agents.md`
- `docs/architecture/subsystems/tools.md`

## Routing

For comms-first use, `config/news.toml` now routes `pty0` to `news-agent` via:

```toml
[agents.routing]
pty0 = "news-agent"
```

`default` remains `echo` in that profile so only mapped channels use the news agent.
