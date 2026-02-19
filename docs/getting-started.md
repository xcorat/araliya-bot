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

### Modular Features

Araliya Bot uses Cargo features to enable or disable subsystems, plugins, and channels at compile-time. This allows for lean builds on resource-constrained hardware.

| Feature Group | Features | Description |
|---------------|----------|-------------|
| **Subsystems**| `subsystem-agents`, `subsystem-llm`, `subsystem-comms`, `subsystem-memory` | Main architectural blocks. |
| **Agents**    | `plugin-echo`, `plugin-basic-chat`, `plugin-chat` | Capabilities for the `agents` subsystem. |
| **Channels**  | `channel-pty` | I/O channels for the `comms` subsystem. |

**Default build (All features enabled):**
```bash
cargo build
```

**Minimal build (No subsystems enabled):**
```bash
cargo build --no-default-features
```

**Custom build (LLM and Agents only):**
```bash
cargo build --no-default-features --features subsystem-llm,subsystem-agents,plugin-basic-chat
```

For a release build:
```bash
cargo build --release --locked
```

For CI/reproducible environments:

```bash
cargo build --release --locked --frozen
```

Binary output: `target/debug/araliya-bot` or `target/release/araliya-bot`.

Quick size checks:

```bash
ls -lh target/release/araliya-bot
size target/release/araliya-bot
readelf -S target/release/araliya-bot | grep -E '\.debug|\.symtab|\.strtab'
```

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
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

### Subsequent Runs

The existing keypair is loaded. The same `bot_id` is printed every time:

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
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
| `-v` / `--verbose` | CLI debug verbosity override |
| `-vv` / `-vvv` | CLI trace verbosity override |

Example:

```bash
ARALIYA_WORK_DIR=/tmp/test-bot RUST_LOG=debug cargo run

# CLI verbosity override
cargo run -- -vvv
```

## Run Tests

```bash
cargo test
```

All 41 tests should pass. Tests use `tempfile` for filesystem isolation — they do not touch `~/.araliya`.
