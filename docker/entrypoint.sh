#!/bin/bash
set -euo pipefail

# Nym Mixnode Docker Entrypoint Script
# Provides initialization, configuration validation, and graceful startup

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_debug() {
    if [[ "${DEBUG:-false}" == "true" ]]; then
        echo -e "${BLUE}[DEBUG]${NC} $1"
    fi
}

# Signal handlers for graceful shutdown
cleanup() {
    log_info "Received shutdown signal, stopping Nym Mixnode gracefully..."
    if [[ -n "${MIXNODE_PID:-}" ]]; then
        kill -TERM "$MIXNODE_PID" 2>/dev/null || true
        wait "$MIXNODE_PID" 2>/dev/null || true
    fi
    log_info "Shutdown complete"
    exit 0
}

trap cleanup SIGTERM SIGINT SIGQUIT

# Environment variables with defaults
export NYM_CONFIG_PATH="${NYM_CONFIG_PATH:-/app/config/config.yaml}"
export NYM_DATA_DIR="${NYM_DATA_DIR:-/app/data}"
export NYM_LOG_LEVEL="${NYM_LOG_LEVEL:-info}"
export NYM_LISTEN_ADDRESS="${NYM_LISTEN_ADDRESS:-0.0.0.0:1789}"
export NYM_METRICS_ADDRESS="${NYM_METRICS_ADDRESS:-0.0.0.0:9090}"
export NYM_API_ADDRESS="${NYM_API_ADDRESS:-0.0.0.0:8080}"
export NYM_REGION="${NYM_REGION:-global}"
export NYM_WORKER_THREADS="${NYM_WORKER_THREADS:-}"
export NYM_TARGET_PPS="${NYM_TARGET_PPS:-25000}"

# Initialization
init_directories() {
    log_info "Initializing directory structure..."
    
    # Create required directories
    mkdir -p "${NYM_DATA_DIR}"/{packets,routing,metrics,logs,backup}
    mkdir -p /app/keys
    
    # Set proper permissions
    chmod 755 "${NYM_DATA_DIR}"
    chmod 700 /app/keys
    
    log_debug "Directory structure created successfully"
}

generate_node_id() {
    if [[ ! -f "/app/keys/node_id" ]]; then
        log_info "Generating new node ID..."
        NODE_ID=$(openssl rand -hex 16)
        echo "$NODE_ID" > /app/keys/node_id
        chmod 600 /app/keys/node_id
        log_info "Generated node ID: $NODE_ID"
    else
        NODE_ID=$(cat /app/keys/node_id)
        log_info "Using existing node ID: $NODE_ID"
    fi
    export NYM_NODE_ID="$NODE_ID"
}

generate_keys() {
    if [[ ! -f "/app/keys/private.pem" ]]; then
        log_info "Generating new key pair..."
        
        # Generate Ed25519 private key
        openssl genpkey -algorithm Ed25519 -out /app/keys/private.pem
        
        # Extract public key
        openssl pkey -in /app/keys/private.pem -pubout -out /app/keys/public.pem
        
        # Set proper permissions
        chmod 600 /app/keys/private.pem
        chmod 644 /app/keys/public.pem
        
        log_info "Generated new Ed25519 key pair"
    else
        log_info "Using existing key pair"
    fi
}

create_config() {
    log_info "Creating configuration file..."
    
    cat > "$NYM_CONFIG_PATH" << EOF
# Nym Mixnode Configuration
# Generated automatically by Docker entrypoint

mixnode:
  node_id: "$NYM_NODE_ID"
  private_key_path: "/app/keys/private.pem"
  region: "$NYM_REGION"
  stake: 0
  capabilities: ["mixnode"]

network:
  listen_address: "$NYM_LISTEN_ADDRESS"
  max_connections: 1000
  connection_timeout: "30s"
  enable_compression: false

security:
  enable_tls: false
  rate_limiting:
    enable: true
    requests_per_second: 1000
    burst_size: 100
  allowed_ips: ["0.0.0.0/0"]

performance:
  target_packets_per_second: $NYM_TARGET_PPS
  worker_threads: ${NYM_WORKER_THREADS:-null}
  enable_simd: true
  batch_size: 1000

monitoring:
  enable_metrics: true
  metrics_endpoint: "/metrics"
  enable_health_checks: true
  prometheus:
    enable: true
    endpoint: "$NYM_METRICS_ADDRESS"
    scrape_interval: "15s"

logging:
  level: "$NYM_LOG_LEVEL"
  format: "text"
  output: "stdout"
EOF

    log_debug "Configuration file created at $NYM_CONFIG_PATH"
}

validate_config() {
    log_info "Validating configuration..."
    
    if ! nym-mixnode config validate --config "$NYM_CONFIG_PATH" 2>/dev/null; then
        log_error "Configuration validation failed"
        return 1
    fi
    
    log_info "Configuration validation successful"
}

check_dependencies() {
    log_info "Checking system dependencies..."
    
    # Check if required ports are available
    if ! nc -z localhost "${NYM_LISTEN_ADDRESS##*:}" 2>/dev/null; then
        log_debug "Listen port ${NYM_LISTEN_ADDRESS##*:} is available"
    else
        log_error "Listen port ${NYM_LISTEN_ADDRESS##*:} is already in use"
        return 1
    fi
    
    # Check disk space
    AVAILABLE_SPACE=$(df /app | tail -1 | awk '{print $4}')
    REQUIRED_SPACE=1048576  # 1GB in KB
    
    if [[ "$AVAILABLE_SPACE" -lt "$REQUIRED_SPACE" ]]; then
        log_warn "Low disk space: ${AVAILABLE_SPACE}KB available, ${REQUIRED_SPACE}KB recommended"
    fi
    
    log_info "Dependency check completed"
}

wait_for_network() {
    if [[ -n "${NYM_BOOTSTRAP_NODES:-}" ]]; then
        log_info "Waiting for bootstrap nodes..."
        
        IFS=',' read -ra BOOTSTRAP_ARRAY <<< "$NYM_BOOTSTRAP_NODES"
        for node in "${BOOTSTRAP_ARRAY[@]}"; do
            host="${node%%:*}"
            port="${node##*:}"
            
            log_debug "Checking connectivity to $host:$port"
            
            timeout 30 bash -c "
                until nc -z '$host' '$port'; do
                    sleep 1
                done
            " || {
                log_warn "Could not reach bootstrap node $host:$port"
            }
        done
        
        log_info "Bootstrap connectivity check completed"
    fi
}

start_mixnode() {
    log_info "Starting Nym Mixnode..."
    log_info "Node ID: $NYM_NODE_ID"
    log_info "Listen Address: $NYM_LISTEN_ADDRESS"
    log_info "Target PPS: $NYM_TARGET_PPS"
    log_info "Log Level: $NYM_LOG_LEVEL"
    
    # Start the mixnode in background
    nym-mixnode "$@" &
    MIXNODE_PID=$!
    
    log_info "Nym Mixnode started with PID: $MIXNODE_PID"
    
    # Wait for the process to finish
    wait "$MIXNODE_PID"
}

# Health check endpoint
setup_health_check() {
    log_info "Setting up health check endpoint..."
    
    # Create a simple health check script
    cat > /app/health_check.sh << 'EOF'
#!/bin/bash
# Simple health check for Nym Mixnode

# Check if the main process is running
if ! pgrep -f "nym-mixnode" > /dev/null; then
    echo "CRITICAL: Nym Mixnode process not running"
    exit 1
fi

# Check if the listen port is open
if ! nc -z localhost "${NYM_LISTEN_ADDRESS##*:}" 2>/dev/null; then
    echo "CRITICAL: Listen port not accessible"
    exit 1
fi

# Check metrics endpoint if enabled
if [[ "${NYM_METRICS_ADDRESS:-}" != "" ]]; then
    if ! curl -s "http://${NYM_METRICS_ADDRESS}/health" > /dev/null 2>&1; then
        echo "WARNING: Metrics endpoint not responding"
        exit 1
    fi
fi

echo "OK: Nym Mixnode is healthy"
exit 0
EOF

    chmod +x /app/health_check.sh
}

# Performance tuning
optimize_performance() {
    log_info "Applying performance optimizations..."
    
    # Set CPU governor to performance if available
    if [[ -f "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor" ]]; then
        echo "performance" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || true
    fi
    
    # Optimize network parameters
    if [[ -w "/proc/sys/net/core/rmem_max" ]]; then
        echo 134217728 > /proc/sys/net/core/rmem_max 2>/dev/null || true
        echo 134217728 > /proc/sys/net/core/wmem_max 2>/dev/null || true
    fi
    
    log_debug "Performance optimizations applied"
}

# Main execution flow
main() {
    log_info "Starting Nym Mixnode Docker container..."
    log_info "Version: $(nym-mixnode --version 2>/dev/null || echo 'unknown')"
    
    # Initialization steps
    init_directories
    generate_node_id
    generate_keys
    create_config
    validate_config
    check_dependencies
    setup_health_check
    optimize_performance
    wait_for_network
    
    # Start the mixnode
    start_mixnode "$@"
}

# Show help if requested
if [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    cat << EOF
Nym Mixnode Docker Entrypoint

Environment Variables:
  NYM_CONFIG_PATH       - Configuration file path (default: /app/config/config.yaml)
  NYM_DATA_DIR          - Data directory path (default: /app/data)
  NYM_LOG_LEVEL         - Log level (default: info)
  NYM_LISTEN_ADDRESS    - Listen address (default: 0.0.0.0:1789)
  NYM_METRICS_ADDRESS   - Metrics address (default: 0.0.0.0:9090)
  NYM_API_ADDRESS       - API address (default: 0.0.0.0:8080)
  NYM_REGION            - Node region (default: global)
  NYM_WORKER_THREADS    - Number of worker threads (default: auto)
  NYM_TARGET_PPS        - Target packets per second (default: 25000)
  NYM_BOOTSTRAP_NODES   - Comma-separated bootstrap nodes
  DEBUG                 - Enable debug logging (default: false)

Usage:
  docker run nym-mixnode [command] [args...]
  
Examples:
  docker run nym-mixnode start
  docker run nym-mixnode status
  docker run nym-mixnode config validate
EOF
    exit 0
fi

# Execute main function with all arguments
main "$@"
