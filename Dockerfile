# Multi-stage build for REAL Nym Mixnode - NO SIMULATIONS
FROM rust:latest as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy cargo files
COPY Cargo.toml ./

# Copy source code
COPY src/ ./src/

# Build the REAL application (release mode for performance)
RUN cargo build --release --bin nym-mixnode-rs

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -r -u 1001 -g root mixnode

# Create app directory
WORKDIR /app

# Copy the REAL binary from builder
COPY --from=builder /app/target/release/nym-mixnode-rs /usr/local/bin/nym-mixnode-rs

# Set ownership and permissions
RUN chown 1001:root /usr/local/bin/nym-mixnode-rs && \
    chmod 755 /usr/local/bin/nym-mixnode-rs

# Switch to non-root user
USER 1001

# Expose ports
EXPOSE 1789/udp
EXPOSE 8080/tcp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Set environment variables
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# Label the image
LABEL org.opencontainers.image.title="Nym Mixnode" \
      org.opencontainers.image.description="High-Performance Nym Mixnode - REAL Implementation" \
      org.opencontainers.image.version="1.0.0" \
      org.opencontainers.image.vendor="Nym Project" \
      org.opencontainers.image.source="https://github.com/nymtech/nym"

# Run the REAL Nym Mixnode (NO SIMULATIONS)
CMD ["/usr/local/bin/nym-mixnode-rs"]
