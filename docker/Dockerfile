# Multi-stage Docker build for production Nym Mixnode
# Stage 1: Build stage
FROM rust:latest AS builder

# Install system dependencies for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1001 -U app

# Set working directory
WORKDIR /usr/src/app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs

# Build dependencies (cached layer)
RUN cargo build --release --locked && \
    rm -rf src target/release/deps/nym_mixnode*

# Copy source code
COPY src ./src
COPY benches ./benches

# Build the actual application
RUN cargo build --release --locked

# Strip the binary to reduce size
    RUN strip target/release/nym-mixnode-rs

# Stage 2: Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    net-tools \
    procps \
    tini \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create app user and group
RUN groupadd -g 1001 app && \
    useradd -r -u 1001 -g app -d /app -s /sbin/nologin app

# Create necessary directories
RUN mkdir -p /app/data /app/logs /app/config /app/keys && \
    chown -R app:app /app

# Copy binary from builder stage
COPY --from=builder /usr/src/app/target/release/nym-mixnode-rs /usr/local/bin/nym-mixnode

    # Copy entrypoint script
    COPY docker/entrypoint.sh /app/entrypoint.sh

# Make entrypoint executable and fix ownership
RUN chmod +x /app/entrypoint.sh && \
    chown app:app /app/entrypoint.sh

# Set working directory
WORKDIR /app

# Switch to app user
USER app

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose ports
EXPOSE 1789 8080 9090

# Use tini for proper signal handling
ENTRYPOINT ["/usr/bin/tini", "--", "/app/entrypoint.sh"]

# Default command
CMD ["nym-mixnode", "start", "--config", "/app/config/config.yaml"]

# Labels for metadata
LABEL org.opencontainers.image.title="Nym Mixnode"
LABEL org.opencontainers.image.description="High-performance Nym mixnode with advanced features"
LABEL org.opencontainers.image.version="1.0.0"
LABEL org.opencontainers.image.authors="Nym Mixnode Team"
LABEL org.opencontainers.image.url="https://github.com/your-org/nym-mixnode-rs"
LABEL org.opencontainers.image.source="https://github.com/your-org/nym-mixnode-rs"
LABEL org.opencontainers.image.licenses="Apache-2.0"
