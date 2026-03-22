# Testing

> The current package version is defined in each crate's `Cargo.toml` (all at `0.2.0-alpha`). Tests and documentation should refer to that file rather than hard‑coding the string.


## Running Tests

```bash
cargo test
```

## Test Coverage (v0.2.0-alpha)

Run all tests with:

```bash
cargo test --workspace               # ~318 tests
```

Per-crate breakdown:

| Crate | Tests | Notes |
|-------|-------|-------|
| `araliya-core` | 44 | config, identity, error, logger |
| `araliya-supervisor` | 6 | dispatch loop, control plane |
| `araliya-llm` | 10 | provider dispatch, dummy provider |
| `araliya-comms` | 4+ | comms state |
| `araliya-memory` | 64 base / 91 with features | `isqlite`, `idocstore`, `ikgdocstore` |
| `araliya-cron` | 4 | timer service |
| `araliya-agents` | varies | feature-gated plugin tests |
| `araliya-bot` | 142+ | integration, subsystem wiring |

Feature-gated tests require explicit flags:

```bash
cargo test -p araliya-memory --features "isqlite,idocstore,ikgdocstore"
cargo test --features idocstore
cargo test --features ikgdocstore
```

## Filesystem Tests

All tests that touch the filesystem use `tempfile::TempDir`. Tests never write to `~/.araliya` or any shared path. Each test gets an isolated temporary directory that is cleaned up automatically on drop.

```rust
use tempfile::TempDir;

let tmp = TempDir::new().unwrap();
let cfg = Config { work_dir: tmp.path().to_path_buf(), .. };
let identity = identity::setup(&cfg).unwrap();
// tmp cleaned up when it goes out of scope
```

## Env Var Tests

Config tests that need to verify override behaviour pass values directly into `load_from()` rather than mutating env vars — no `unsafe` required.

```rust
// Pass override directly — no env mutation
let cfg = load_from(f.path(), Some("/tmp/override"), None).unwrap();
assert_eq!(cfg.work_dir, PathBuf::from("/tmp/override"));
```

## Adding Tests

- Place tests in a `#[cfg(test)]` block at the bottom of the module file
- One assertion per test where possible — keep failures specific
- Use `tempfile::TempDir` for any test that creates files
- Test error paths as well as happy paths
- Parser and helper code (e.g. FTS5 query escaping) should have unit or integration tests to prevent regressions

## CI

`.github/workflows/ci.yml` runs on every push and PR:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
# Plus per-crate build tier jobs (minimal, default, full, runtimes, ui, agents, memory-extended)
```
