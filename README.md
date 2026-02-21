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

## Binary Releases

Tagging `v*` creates a GitHub Release with a Linux x86_64 binary archive and checksum.

```bash
git tag v0.1.0
git push origin v0.1.0
```

Download from the repository Releases page:

- `araliya-bot-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS`

Verify and run:

```bash
sha256sum -c SHA256SUMS
tar -xzf araliya-bot-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
./araliya-bot --help
```

---

## Project Structure

```
araliya-bot/
├── config/
│   └── default.toml           main config
├── src/
│   ├── main.rs                entry point / supervisor bootstrap
│   ├── config.rs              TOML loading + env overrides
│   ├── identity.rs            ed25519 keypair, bot_id derivation
│   ├── logger.rs              tracing-subscriber init
│   ├── error.rs               error types
│   ├── llm/                   LLM provider abstraction
│   ├── supervisor/            bus, dispatch, run-loop
│   └── subsystems/
│       ├── agents/            agent routing + plugins
│       │   └── chat/          chat-family plugins (ChatCore composition)
│       ├── comms/             communication channels (PTY, Telegram)
│       └── llm/               LLM subsystem (BusHandler)
└── docs/                      documentation
```
