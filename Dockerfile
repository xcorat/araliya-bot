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

# Non-root user for least-privilege operation.
RUN useradd --no-create-home --shell /bin/false araliya

WORKDIR /app

# Copy the release binary and default config.
COPY --from=builder /build/target/release/araliya-bot /usr/local/bin/araliya-bot
COPY config/ /app/config/

# Persistent data lives here; mount a volume over this path in production.
VOLUME ["/data/araliya"]

# HTTP / axum channel.
EXPOSE 8080

USER araliya

# ARALIYA_WORK_DIR  — persistent state directory (identity keypair, memory, …)
# ARALIYA_HTTP_BIND — bind address; 0.0.0.0 required inside a container
ENV ARALIYA_WORK_DIR=/data/araliya \
    ARALIYA_HTTP_BIND=0.0.0.0:8080

ENTRYPOINT ["/usr/local/bin/araliya-bot"]
