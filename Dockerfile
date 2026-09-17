# syntax=docker/dockerfile:1

# ------------------------------------------------------------------------------
# Stage 1: Chef Base
# ------------------------------------------------------------------------------
FROM rust:1.98.0-slim-bookworm AS chef

WORKDIR /app

# Install build dependencies and cargo-chef
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    ca-certificates \
    && cargo install cargo-chef --locked \
    && rm -rf /var/lib/apt/lists/*

# ------------------------------------------------------------------------------
# Stage 2: Planner (computes recipe.json for Cargo.lock dependencies)
# ------------------------------------------------------------------------------
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ------------------------------------------------------------------------------
# Stage 3: Builder (cooks dependencies and compiles release binary)
# ------------------------------------------------------------------------------
FROM chef AS builder

# Cook dependencies from recipe — cached across builds unless Cargo.lock/toml changes
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy application source and build release binaries
COPY . .
RUN cargo build --release && \
    strip target/release/myrologic_pos_backend && \
    mkdir -p target/release/bins && \
    for src in src/bin/*.rs; do \
      bin=$(basename "$src" .rs); \
      if [ -f "target/release/$bin" ]; then \
        strip "target/release/$bin" && \
        cp "target/release/$bin" target/release/bins/; \
      fi; \
    done

# ------------------------------------------------------------------------------
# Stage 4: Minimal Runtime Image
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runner

# Install runtime utilities (curl for healthcheck, ca-certificates for TLS, tzdata)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

# Create non-root system user and group (UID/GID 10001)
RUN groupadd -g 10001 appuser && \
    useradd -u 10001 -g appuser -d /app -s /bin/false -M appuser

WORKDIR /app

# Prepare directory for generated invoice/receipt PDFs, persistent SQLite data, and bin tools with write permission
RUN mkdir -p /app/generated_documents /app/data /app/bin && \
    chown -R appuser:appuser /app

# Copy stripped binaries from builder stage
COPY --from=builder --chown=appuser:appuser /app/target/release/myrologic_pos_backend /app/myrologic_pos_backend
COPY --from=builder --chown=appuser:appuser /app/target/release/bins/ /app/bin/

# Environment defaults
ENV PORT=8080 \
    DATABASE_URL=sqlite:///app/data/pos.db?mode=rwc \
    GENERATED_DOCUMENTS_DIR=/app/generated_documents \
    RUST_LOG=myrologic_pos_backend=info,tower_http=info,info

# Declare persistent volumes for generated invoice/receipt PDF storage and SQLite database
VOLUME ["/app/generated_documents", "/app/data"]

USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/api/health || exit 1

CMD ["/app/myrologic_pos_backend"]
