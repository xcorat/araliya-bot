# Deployment

## Development

```bash
cd araliya-bot
cargo run
```

Logs go to stderr. Data goes to `~/.araliya/`.

```bash
# Verbose output
RUST_LOG=debug cargo run

# Custom data directory
ARALIYA_WORK_DIR=/tmp/araliya-dev cargo run
```

## Docker

### Quick start

```bash
# Copy and edit the env file with your API keys.
cp .env.example .env
$EDITOR .env

# Build and run (data persisted in ./data/).
docker compose up --build
```

The HTTP/axum channel is available at `http://localhost:8080`.

### Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `ARALIYA_WORK_DIR` | `/data/araliya` | Persistent state (identity, memory) |
| `ARALIYA_HTTP_BIND` | `0.0.0.0:8080` | Bind address for the HTTP/axum channel |
| `ARALIYA_LOG_LEVEL` | *(from config)* | Log level override (`info`, `debug`, …) |
| `OPENAI_API_KEY` | *(none)* | API key for the configured LLM provider |
| `TELEGRAM_BOT_TOKEN` | *(none)* | Telegram bot token (when Telegram channel is enabled) |

### Persistent data

The bot stores its identity keypair, memory, and other state under `ARALIYA_WORK_DIR`.
Mount a host directory or a named volume over `/data/araliya` so the identity is preserved
across container restarts:

```yaml
volumes:
  - ./data:/data/araliya   # host-directory bind mount (default in docker-compose.yml)
  # or
  - araliya_data:/data/araliya   # named Docker volume
```

### Building the image manually

```bash
docker build -t araliya-bot:latest .
docker run --rm \
  -p 8080:8080 \
  -v "$(pwd)/data:/data/araliya" \
  -e OPENAI_API_KEY=sk-... \
  araliya-bot:latest
```

## Production (Single Machine)

### One-line install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
```

`install.sh` auto-detects arch (Linux x86\_64 / aarch64), resolves the latest GitHub
release, downloads and verifies the tarball, installs the binary to `~/.local/bin/`,
and seeds `~/.config/araliya/` with starter config.

Override defaults with environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `ARALIYA_TIER` | `default` | Feature tier: `minimal`, `default`, or `full` |
| `ARALIYA_VERSION` | latest | Pin to a specific release tag (e.g. `v0.2.0-alpha`) |
| `INSTALL_DIR` | `~/.local/bin` | Binary installation directory |
| `ARALIYA_CONFIG_DIR` | `~/.config/araliya` | Config directory |
| `ARALIYA_WORK_DIR` | `~/.araliya` | Runtime data directory |

After installation, run the interactive setup wizard:

```bash
araliya-bot setup   # LLM provider, agent profile, channels
araliya-bot doctor  # validate config before first run
araliya-bot         # start the bot
```

### Manual download

Every `v*` tag publishes release assets on GitHub Releases.

```bash
VERSION=v0.2.0-alpha
TIER=default

curl -LO https://github.com/xcorat/araliya-bot/releases/download/${VERSION}/araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu.tar.gz
curl -LO https://github.com/xcorat/araliya-bot/releases/download/${VERSION}/SHA256SUMS
sha256sum -c SHA256SUMS
tar -xzf araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu.tar.gz
install -m 755 araliya-bot-${VERSION}-${TIER}-x86_64-unknown-linux-gnu/bin/araliya-bot ~/.local/bin/araliya-bot
araliya-bot setup
```

`TIER` options: `minimal`, `default`, `full`.

Each tiered tarball includes `bin/araliya-bot`, `config/`, and `frontend/svui/`.

> **Note:** the canonical version string lives only in `crates/araliya-bot/Cargo.toml`.
> When building the Svelte UI the frontend sync script (`scripts/sync-version.mjs`)
> copies that value into `frontend/svui/.env` so the UI can display it. Update the
> Cargo version before tagging.

To create a release from this repository:

```bash
git tag v0.2.0-alpha
git push origin v0.2.0-alpha
```

The GitHub Actions workflow publishes the assets automatically.

Build a release binary:

```bash
cargo build --release --locked
cp target/release/araliya-bot /usr/local/bin/
```

### systemd Service

A ready-to-use unit file is provided at `deploy/araliya-bot.service` (see inline comments for full options). Quick setup:

```bash
# Install binary (install.sh or build from source)
curl -fsSL https://raw.githubusercontent.com/xcorat/araliya-bot/main/install.sh | bash
# or: cargo build --release && install -m 755 target/release/araliya-bot /usr/local/bin/

# Run setup wizard to generate config + .env
araliya-bot setup

# Validate config
araliya-bot doctor

# Install as a system service
useradd -r -s /sbin/nologin araliya
cp deploy/araliya-bot.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now araliya-bot
journalctl -u araliya-bot -f
```

The service runs without `-i` — daemon mode, no stdin. Use `araliya-ctl` to interact with the running daemon:

```bash
araliya-ctl status
araliya-ctl health
araliya-ctl subsystems
araliya-ctl shutdown
```

### Environment File

`/etc/araliya-bot/env` (mode 0600, owned root):

```bash
OPENAI_API_KEY=sk-...
RUST_LOG=info              # optional; default comes from config log_level
# TELEGRAM_BOT_TOKEN=...  # if channel-telegram is enabled
```

## Data Backup

Back up the identity keypair to retain `bot_id` across reinstalls:

```bash
# Backup
cp -r ~/.araliya/bot-pkey*/ /secure/backup/

# Restore
mkdir -p ~/.araliya/
cp -r /secure/backup/bot-pkey*/ ~/.araliya/
chmod 600 ~/.araliya/bot-pkey*/id_ed25519
```

Losing the keypair generates a new identity with a different `bot_id`.
