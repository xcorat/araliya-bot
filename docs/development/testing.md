# Testing

## Running Tests

```bash
cargo test
```

## Test Coverage (v0.1)

| Module | Tests | Coverage |
|--------|-------|---------|
| `error.rs` | 4 | All variants: display, trait impl, IO conversion |
| `logger.rs` | 3 | Valid levels, invalid levels, init succeeds |
| `config.rs` | 6 | Parse, tilde expansion, absolute/relative paths, missing file, env overrides |
| `identity.rs` | 6 | bot_id format, unique keygen, save/load round-trip, dir creation, idempotency, file permissions |

**Total: 20 tests**

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

## CI (Future)

```yaml
# .github/workflows/ci.yml (planned)
- cargo check
- cargo test
- cargo clippy -- -D warnings
- cargo fmt --check
```
