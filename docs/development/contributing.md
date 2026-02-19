# Contributing

## Prerequisites

- Rust toolchain 1.80+ (`rustup`)
- `cargo` (bundled with Rust)

## Workflow

```bash
# Check compilation (fast)
cargo check

# Run tests
cargo test

# Build
cargo build

# Run
cargo run
```

Always run `cargo check` and `cargo test` before committing changes.

## Code Style

- One concern per module — `config.rs` only loads config, `identity.rs` only manages identity
- `main.rs` is an orchestrator only — no business logic
- Errors via `thiserror` — no `unwrap()` in non-test code, no `Box<dyn Error>` in public APIs
- Logging via `tracing` macros (`info!`, `debug!`, `warn!`, `error!`) — not `println!`

## Adding a New Module

1. Create `src/{name}.rs`
2. Declare it in `main.rs`: `mod {name};`
3. Define a typed error variant in `error.rs` if needed
4. Add unit tests in a `#[cfg(test)]` block at the bottom of the module
5. Use `tempfile::TempDir` for any filesystem tests

## Subsystem Development (Future)

Each subsystem will live in `src/subsystems/{name}/` with its own `mod.rs`. See the [architecture overview](../architecture/overview.md) for the planned structure.

## Documentation

Update the relevant doc in `docs/` when making significant changes to a module or subsystem. Keep `docs/architecture/overview.md` current with module status.
