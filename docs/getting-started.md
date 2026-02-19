# Getting Started

## Requirements

- Rust toolchain 1.80+ (`rustup` recommended)
- Linux or macOS
- Internet access for initial `cargo build` (downloads dependencies)

## Build

```bash
cd araliya-bot
cargo build
```

For a release build:

```bash
cargo build --release
```

Binary output: `target/debug/araliya-bot` or `target/release/araliya-bot`.

## Run

```bash
cargo run
```

Or directly:

```bash
./target/debug/araliya-bot
```

### First Run

On first run the bot generates a persistent ed25519 keypair and saves it to `~/.araliya/bot-pkey{id}/`. Expected output:

```
✓ Bot initialized: bot_id=5d16993c
```

### Subsequent Runs

The existing keypair is loaded. The same `bot_id` is printed every time:

```
✓ Bot initialized: bot_id=5d16993c
```

## Verify

```bash
# Check identity files were created
ls ~/.araliya/
# → bot-pkey5d16993c/

ls ~/.araliya/bot-pkey5d16993c/
# → id_ed25519   id_ed25519.pub

# Verify secret key permissions
stat -c "%a %n" ~/.araliya/bot-pkey5d16993c/id_ed25519
# → 600 ...
```

## Environment Variables

| Variable | Effect |
|----------|--------|
| `ARALIYA_WORK_DIR` | Override working directory (default: `~/.araliya`) |
| `ARALIYA_LOG_LEVEL` | Override log level (default: `info`) |
| `RUST_LOG` | Standard tracing env filter (overrides `log_level`) |

Example:

```bash
ARALIYA_WORK_DIR=/tmp/test-bot RUST_LOG=debug cargo run
```

## Run Tests

```bash
cargo test
```

All 20 tests should pass. Tests use `tempfile` for filesystem isolation — they do not touch `~/.araliya`.
