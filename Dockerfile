# syntax=docker/dockerfile:1

# ── Stage 1: builder ──────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Install C linker (required by some crates) and pkg-config.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies: copy manifests first, then fetch.
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked

# Copy the full source and build the release binary.
COPY . .
RUN cargo build --release --locked

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Install CA certificates (required for outbound TLS calls to LLM APIs).
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user; home dir provides a natural location for the ~/.araliya work dir.
RUN useradd --create-home --shell /bin/false araliya

WORKDIR /app

# Copy the release binary and bake in the docker-specific config.
COPY --from=builder /build/target/release/araliya-bot /usr/local/bin/araliya-bot
COPY config/ /app/config/
COPY config/docker.toml /app/config/default.toml

# Persistent data lives in the bot's home dir (~/.araliya).
VOLUME ["/home/araliya/.araliya"]

# HTTP / axum channel.
EXPOSE 8080

USER araliya

ENTRYPOINT ["/usr/local/bin/araliya-bot"]
