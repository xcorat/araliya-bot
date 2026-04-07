# Installation

## Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
```

The installer will ask what you want to use Araliya Bot for, download the matching pre-built binary from [GitHub Releases](https://github.com/xcorat/araliya-bot/releases), and launch an interactive setup wizard.

---

## Use cases

```
What do you want to use Araliya Bot for?

  1) Basic chat           — direct LLM, no memory
  2) Homebuilder          — AI-generated personal landing page
  3) Docs / KG agent      — RAG over a local docs directory
  4) Full feature set     — all built-in agents and tools
  5) Build from source    — custom feature selection
```

| Option | Binary tier | What's included |
|---|---|---|
| Basic chat | `minimal` | PTY terminal, basic-chat agent, LLM |
| Homebuilder | `default` | HTTP/web UI, homebuilder agent, session chat, cron |
| Docs / KG agent | `default` | HTTP/web UI, docs agent (BM25 + optional KG RAG), session chat |
| Full feature set | `full` | All agents, Telegram, Gmail, GDELT news, KG docstore |
| Build from source | — | You pick the features; requires Rust (installer links to rustup) |

---

## Setup wizard

After the binary is installed, `araliya-bot setup` launches automatically and asks:

1. **Bot name** and runtime data directory
2. **LLM provider** — OpenAI, OpenRouter, Anthropic, Ollama/local, or custom OpenAI-compatible endpoint
3. **API key** — written to `.env` only, never to `config.toml`
4. **Agent profile** — including profile-specific prompts:
   - *Homebuilder*: your display name + optional notes directory
   - *Docs / Docs KG*: path to your local docs directory
5. **Communication channels** — HTTP/web UI and/or Telegram bot

Files written:

| File | Contents |
|---|---|
| `~/.config/araliya/config.toml` | All config except secrets |
| `~/.config/araliya/.env` | API keys and tokens (mode 0600) |

---

## After install

```bash
# Interactive terminal (PTY)
araliya-bot -i -f ~/.config/araliya/config.toml

# Headless — serves web UI + REST API at localhost:8080
araliya-bot -f ~/.config/araliya/config.toml

# Validate config
araliya-bot doctor -f ~/.config/araliya/config.toml

# Re-run the setup wizard
araliya-bot setup -f ~/.config/araliya/config.toml
```

---

## Environment overrides

Set these before running the installer to skip prompts or change defaults:

| Variable | Default | Description |
|---|---|---|
| `ARALIYA_TIER` | *(menu)* | Skip the use-case menu: `minimal`, `default`, or `full` |
| `ARALIYA_VERSION` | latest | Pin a specific release tag, e.g. `v0.2.0-alpha` |
| `INSTALL_DIR` | `~/.local/bin` | Where to place the binary |
| `ARALIYA_CONFIG_DIR` | `~/.config/araliya` | Config directory |
| `ARALIYA_WORK_DIR` | `~/.araliya` | Runtime data directory (identity, sessions, memory) |

Example — install the minimal tier without the menu:

```bash
ARALIYA_TIER=minimal curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
```

---

## Manual binary install

Download a release bundle directly from [GitHub Releases](https://github.com/xcorat/araliya-bot/releases):

```bash
# Verify checksum
sha256sum -c SHA256SUMS

# Extract
tar -xzf araliya-bot-v0.2.0-alpha-default-x86_64-unknown-linux-gnu.tar.gz
cd araliya-bot-v0.2.0-alpha-default-x86_64-unknown-linux-gnu

# First-time setup
./bin/araliya-bot setup

# Run
./bin/araliya-bot -i
```

---

## Build from source

See the [README](../README.md#build-from-source) for build instructions and tier flags.
