# Config Layering (`[meta] base`)

**Status:** Implemented — 2026-02-21

## Summary

Config files can now declare a base file they extend. The loader deep-merges the
chain before deserialising into `RawConfig`.  No new CLI flags or dependencies.

## How it works

1. `load_raw_merged(path, visited)` reads the selected file as `toml::Value`.
2. If `meta.base` is present, the base file is loaded recursively (same function), then `merge_toml(base, overlay)` is applied — tables merged recursively, scalars/arrays replaced by overlay value.
3. `visited` carries canonicalised paths; a repeated path is a cycle error.
4. The resulting `toml::Value` is deserialised into `RawConfig` via `serde`.
5. `[meta]` is silently ignored by serde (no field declared for it).

## Key files

| File | Change |
|------|--------|
| `src/config.rs` | `merge_toml`, `load_raw_merged`, updated `load_from` |
| `config/full.toml` | Rewritten as partial delta over `default.toml` |
| `docs/configuration.md` | Inheritance section, updated resolution order |

## Merge semantics

- **Tables** — recursive merge; overlay keys win, base keys not in overlay are preserved.
- **Scalars / arrays** — overlay wins wholesale.
- **Chains** — grandbase → base → overlay supported.
- **Cycles** — detected via canonicalised path set; returns `AppError::Config`.
- **Missing base** — clear `cannot read` error propagated to caller.

## Tests (in `src/config.rs`)

| Test | Covers |
|------|--------|
| `overlay_keeps_base_fields` | Fields absent from overlay are inherited |
| `overlay_wins_scalar` | Overlay scalar replaces base; sibling keys preserved |
| `chained_bases` | Three-level chain resolves correctly |
| `missing_base_errors` | Non-existent base → error |
| `cycle_detection` | Self-referential file → circular error |
