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
| `LLM_API_KEY` | *(none)* | API key for the configured LLM provider |
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
  -e LLM_API_KEY=sk-... \
  araliya-bot:latest
```

## Production (Single Machine)

Build a release binary:

```bash
cargo build --release --locked
cp target/release/araliya-bot /usr/local/bin/
```

Verify artifact details (optional):

```bash
ls -lh target/release/araliya-bot
file target/release/araliya-bot
ldd target/release/araliya-bot
```

### systemd User Service

Create `~/.config/systemd/user/araliya-bot.service`:

```ini
[Unit]
Description=Araliya Bot
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/araliya-bot
WorkingDirectory=/usr/local/share/araliya-bot
EnvironmentFile=%h/.config/araliya-bot/.env
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now araliya-bot
journalctl --user -fu araliya-bot
```

### Environment File

`~/.config/araliya-bot/.env` (mode 0600):

```bash
ARALIYA_WORK_DIR=/var/lib/araliya-bot
ARALIYA_LOG_LEVEL=info
# LLM_API_KEY=sk-...          (future)
# TELEGRAM_BOT_TOKEN=...      (future)
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
