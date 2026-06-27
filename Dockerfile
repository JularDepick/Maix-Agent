# Multi-stage build for Maix-Agent
# Build: docker build -t maix-agent .
# Run:   docker run -it --rm -e DEEPSEEK_API_KEY=xxx maix-agent chat

# ---- Build stage ----
FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
COPY config/ config/
COPY proto/ proto/

RUN cargo build --release --bin maix

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/maix /usr/local/bin/maix
COPY config/default.toml /etc/maix/config.toml

ENV MAIX_HOME=/root/.maix
ENV MAIX_CONFIG=/etc/maix/config.toml

VOLUME ["/root/.maix", "/workspace"]
WORKDIR /workspace

ENTRYPOINT ["/usr/local/bin/maix"]
CMD ["chat"]
