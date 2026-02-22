# Newsmail Aggregator Tool (MVP)

## Summary

Implemented `newsmail_aggregator/get` in the tools subsystem via the existing `tools/execute` route.

## Key decisions

- Kept one tools protocol route: `tools/execute`
- Chose `tsec_last` as the canonical time-window key name
- Avoided duplication by reusing Gmail core API logic (`read_many`) instead of copying OAuth and API calls

## Behavior

- Tool/action: `newsmail_aggregator/get`
- Returns JSON list of email summaries
- Supports empty args (`{}`) and optional overrides:
  - `label` — single Gmail label ID string, or JSON array of label IDs
  - `n_last`
  - `t_interval` (e.g. `"1d"`, `"1h"`) — preferred over `tsec_last`
  - `tsec_last` — time window in seconds (legacy, overridden by `t_interval`)
  - `q` — extra Gmail search terms (ANDed with labelIds filter)

## Config

Defaults in config:

- `tools.newsmail_aggregator.label_ids = ["INBOX"]` — fallback when LLM provides no label
- `tools.newsmail_aggregator.n_last = 10`
- `tools.newsmail_aggregator.tsec_last` optional
- `tools.newsmail_aggregator.q` optional

## Internal architecture changes

- `ToolsSubsystem` now receives `NewsmailAggregatorConfig` from `Config`
- `gmail.rs` now exposes reusable `read_many(...)` and includes `internal_date_unix` on summaries for local time-window filtering
- `newsmail_aggregator.rs` resolves defaults + optional request overrides, fetches latest N, then applies `tsec_last` filter
