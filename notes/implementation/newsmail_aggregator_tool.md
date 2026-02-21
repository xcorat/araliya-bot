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
  - `mailbox`
  - `n_last`
  - `tsec_last`

## Config

Added defaults in config:

- `tools.newsmail_aggregator.mailbox = "inbox"`
- `tools.newsmail_aggregator.n_last = 10`
- `tools.newsmail_aggregator.tsec_last` optional

## Internal architecture changes

- `ToolsSubsystem` now receives `NewsmailAggregatorConfig` from `Config`
- `gmail.rs` now exposes reusable `read_many(...)` and includes `internal_date_unix` on summaries for local time-window filtering
- `newsmail_aggregator.rs` resolves defaults + optional request overrides, fetches latest N, then applies `tsec_last` filter
