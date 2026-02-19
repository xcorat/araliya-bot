# 🌸 Araliya Bot — Modular Agentic Assistant

**Araliya Bot** is a fast, modular, and fully autonomous AI assistant infrastructure built in Rust. It operates as a single-process supervisor with pluggable subsystems, designed to act as a cohesive agentic AI.

## ✨ Highlights

- **Modular Architecture:** The bot acts as the main entry point (supervisor), owning the event bus and managing global events.
- **Pluggable Subsystems:** Subsystems are separate modules that can be toggled on/off at startup. They can dynamically load, unload, and manage multiple plugins at runtime.
- **Event-Driven Communication:** Subsystems and plugins communicate seamlessly with each other and the supervisor via a central event bus.
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

Log verbosity can be increased at runtime for troubleshooting:

```bash
cargo run -- -v    # debug
cargo run -- -vvv  # trace
```

## 🏗️ Architecture

Araliya Bot is designed around a flexible, event-driven architecture:

1. **Supervisor:** The core application. It holds the primary public-key identity, handles global events, and owns the event bus.
2. **Subsystems:** Independent modules that provide specific capabilities. They can be enabled or disabled via configuration.
3. **Plugins:** Loaded and managed by subsystems at runtime. Subsystems can grant plugins direct access to the event bus for deep integration.
4. **Event Bus:** The central nervous system of the bot, routing messages between the supervisor, subsystems, and plugins.

## 📊 Comparison

| Feature | Araliya Bot 🌸 | ZeroClaw 🦀 | OpenClaw 🦞 |
| :--- | :--- | :--- | :--- |
| **Language** | Rust | Rust | TypeScript / Node.js |
| **Architecture** | Single-process supervisor, pluggable subsystems, event bus | Trait-driven, single binary, swappable providers/channels | Gateway WS control plane, multi-agent routing |
| **Memory Footprint** | *Not implemented / No info* | < 5MB | > 1GB |
| **Startup Time** | *Not implemented / No info* | < 10ms | > 500s |
| **Binary Size** | *Not implemented / No info* | ~3.4 MB | ~28MB (dist) |
| **Identity** | ed25519 keypair, persistent bot ID | AIEOS (JSON) or OpenClaw (Markdown) | Markdown files (IDENTITY.md, SOUL.md, etc.) |
| **Security** | *Not implemented / No info* | Gateway pairing, strict sandboxing, explicit allowlists | Gateway pairing, sandboxing, allowlists |
| **Channels** | *Not implemented / No info* | CLI, Telegram, Discord, Slack, WhatsApp, etc. | WhatsApp, Telegram, Slack, Discord, etc. |
| **Memory System** | *Not implemented / No info* | SQLite hybrid search, PostgreSQL, Lucid bridge | *No info* |
| **Tools** | *Not implemented / No info* | Shell, file, memory, cron, browser, composio | Browser control, Canvas, Nodes, Skills |

## ⚙️ Configuration & Secrets

Araliya Bot strictly separates configuration from secrets:

- **Configuration:** Non-sensitive settings belong in `config/` (e.g., `config/default.toml`).
- **Secrets:** API keys (e.g., `LLM_API_KEY`) and tokens must be provided via environment variables or an `.env` file. 
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
