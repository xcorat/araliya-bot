# 🌸 Araliya Bot — Modular Agentic Assistant

**Araliya Bot** is a fast, modular, and fully autonomous AI assistant infrastructure built in Rust. It operates as a single-process supervisor with pluggable subsystems, designed to act as a cohesive agentic AI.

## ✨ Highlights

- **Modular Architecture:** The bot acts as the main entry point (supervisor), owning the event bus and managing global events.
- **Pluggable Subsystems:** Subsystems are separate modules that can be toggled on/off at startup. They can dynamically load, unload, and manage multiple agents at runtime.
- **Event-Driven Communication:** Subsystems and agents communicate seamlessly with each other and the supervisor via a central event bus.
- **Secure Identity:** Automatically generates a persistent cryptographic identity (ed25519 keypair) on the first run, ensuring secure and verifiable operations.
- **Lean & Fast:** Built in Rust for minimal overhead, fast cold starts, and memory safety.

## 🚀 Quick Start (TL;DR)

**Requirements:** Rust toolchain (1.80+), Linux/macOS

```bash
# Clone the repository
git clone <repo>
cd araliya-bot

# Build the release binary
cargo build --release

# Run the supervisor
cargo run --release
```

On the first run, a persistent bot identity is generated at `~/.araliya/bot-pkey{id}/`.

```text
INFO araliya_bot: identity ready — starting subsystems bot_id=51aee87e
```

### Logging & Debugging

Log verbosity can be set at runtime with `-v` flags:

```bash
cargo run -- -v      # warn  (quiet — errors and warnings only)
cargo run -- -vv     # info  (normal operational output)
cargo run -- -vvv    # debug (routing, handler registration, diagnostics)
cargo run -- -vvvv   # trace (full payload dumps, very verbose)
```

## 🏗️ Architecture

Araliya Bot is designed around a flexible, event-driven architecture:

1. **Supervisor:** The core application. It holds the primary public-key identity, handles global events, and owns the event bus.
2. **Subsystems:** Independent modules that provide specific capabilities. They can be enabled or disabled via configuration.
3. **Agents:** Autonomous actors loaded and managed by the agents subsystem at runtime. Each agent can be granted access to the event bus and memory system.
4. **Event Bus:** The central nervous system of the bot, routing messages between the supervisor, subsystems, and agents.



## 📈 Benchmarks (CI) - NOT SETUP [TODO]

A GitHub Actions workflow has been added at `.github/workflows/benchmarks.yml` that measures:

- `binary size` (`target/release/araliya-bot`)
- `startup latency` (time from process start until the log line `identity ready — starting subsystems`)
- `memory RSS` (VmRSS while running)

Run locally:

```bash
cargo build --release
./target/release/araliya-bot & sleep 1; pkill araliya-bot
```

Example local measurement (observed on this machine):

```text
$ ls -lh target/release/araliya-bot
-rwxr-xr-x. 2 sachi sachi 3.5M Feb 19 13:00 target/release/araliya-bot

# sample process info (RES)
PID    USER   RSS
603255 sachi  6.1M
```

## ⚙️ Configuration & Secrets

Araliya Bot strictly separates configuration from secrets:

- **Configuration:** Non-sensitive settings belong in `config/` (e.g., `config/default.toml`).
- **Secrets:** API keys (e.g., `OPENAI_API_KEY`) and tokens must be provided via environment variables or an `.env` file. 
  > **Note:** Never commit secrets. The `.env` file must remain gitignored.

## 🛠️ Development

We expect a clean and efficient developer workflow:

- Use `cargo check` for quick validation during development.
- Use `cargo test` to ensure reliability when behavior changes.
- Keep dependencies minimal. Prefer small, single-purpose crates over large frameworks.

## 📚 Documentation

Dive deeper into the specifics of Araliya Bot:

- [Getting Started](getting-started.md) — Build, run, and verify your setup.
- [Configuration](configuration.md) — Detailed guide on config files and environment variables.
- [Architecture Overview](architecture/overview.md) — In-depth look at the system design and event bus.
- [Identity](architecture/identity.md) — How cryptographic identities and bot IDs work.
- [Operations](operations/deployment.md) — Guide for running Araliya Bot in production.
- [Development](development/contributing.md) — Guidelines for contributing and testing.
