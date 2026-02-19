# Araliya Bot

Modular agentic assistant. Rust, single-process supervisor with pluggable subsystems.

**Status:** v0.1 — supervisor bootstrap (config, identity, logging)

---

## Quick Start

**Requirements:** Rust toolchain (1.80+), Linux/macOS

```bash
git clone <repo>
cd araliya-bot
cargo build
cargo run
```

On first run, a persistent bot identity is generated at `~/.araliya/bot-pkey{id}/`.

```
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

Log verbosity can be increased at runtime:

```bash
cargo run -- -v    # debug
cargo run -- -vvv  # trace
```

---

## Documentation

- [Getting Started](docs/getting-started.md) — build, run, verify
- [Configuration](docs/configuration.md) — config files and env vars
- [Architecture Overview](docs/architecture/overview.md) — system design
- [Operations](docs/operations/deployment.md) — running in production
- [Development](docs/development/contributing.md) — contributing and testing

---

## Project Structure

```
araliya-bot/
├── config/
│   └── default.toml       main config
├── src/
│   ├── main.rs            entry point / supervisor bootstrap
│   ├── config.rs          TOML loading + env overrides
│   ├── identity.rs        ed25519 keypair, bot_id derivation
│   ├── logger.rs          tracing-subscriber init
│   └── error.rs           error types
└── docs/                  documentation (this tree)
```
