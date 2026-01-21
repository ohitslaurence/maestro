# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Multi-stage Dockerfile for Loom
# Builds loom-server (Rust) and loom-web (Svelte) into a single image

# =============================================================================
# Stage 1: Build loom-server (Rust)
# =============================================================================
FROM rust:1.83-slim-bookworm AS rust-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release --package loom-server

# =============================================================================
# Stage 2: Build loom-web (Node/pnpm)
# =============================================================================
FROM node:22-slim AS web-builder

WORKDIR /app

# Install pnpm
RUN corepack enable && corepack prepare pnpm@latest --activate

# Copy package files first for dependency caching
COPY web/loom-web/package.json web/loom-web/pnpm-lock.yaml* ./

# Install dependencies
RUN pnpm install --frozen-lockfile || pnpm install

# Copy source and build
COPY web/loom-web ./
RUN pnpm build

# =============================================================================
# Stage 3: Runtime image
# =============================================================================
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 loom

# Copy loom-server binary
COPY --from=rust-builder /app/target/release/loom-server /usr/local/bin/loom-server

# Copy loom-web static assets
COPY --from=web-builder /app/build /var/www/loom-web

# Create data directory
RUN mkdir -p /var/lib/loom && chown loom:loom /var/lib/loom

# Switch to non-root user
USER loom

# Environment variables
ENV RUST_LOG=info \
    LOOM_SERVER_HOST=0.0.0.0 \
    LOOM_SERVER_PORT=8080 \
    LOOM_SERVER_DATABASE_URL=sqlite:/var/lib/loom/loom.db?mode=rwc \
    LOOM_SERVER_WEB_DIR=/var/www/loom-web

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Default command
CMD ["loom-server"]
