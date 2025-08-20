#!/bin/bash

# Network monitoring script for mixnode testing
# Monitors real P2P connections, packet flow, and network health

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_info() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')] $1${NC}"
}

print_success() {
    echo -e "${GREEN}[$(date '+%H:%M:%S')] $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}[$(date '+%H:%M:%S')] $1${NC}"
}

print_error() {
    echo -e "${RED}[$(date '+%H:%M:%S')] $1${NC}"
}

# Monitor network connections
monitor_connections() {
    print_info "=== NETWORK CONNECTIONS ==="
    
    # Check TCP connections on port 8080
    print_info "TCP connections to mixnode ports:"
    netstat -tln | grep :8080 || echo "No TCP listeners on port 8080"
    
    # Check UDP connections
    print_info "UDP connections to mixnode ports:"
    netstat -uln | grep :8080 || echo "No UDP listeners on port 8080"
    
    echo ""
}

# Monitor packet statistics
monitor_packets() {
    print_info "=== PACKET STATISTICS ==="
    
    # Show network interface statistics
    print_info "Network interface statistics:"
    cat /proc/net/dev | grep -E "(eth0|lo)" || echo "No network interfaces found"
    
    echo ""
}

# Test connectivity to all nodes
test_connectivity() {
    print_info "=== CONNECTIVITY TESTS ==="
    
    local nodes=("172.25.0.10" "172.25.0.11" "172.25.0.12" "172.25.0.13")
    local node_names=("bootstrap" "mixnode-1" "mixnode-2" "mixnode-3")
    
    for i in "${!nodes[@]}"; do
        local ip="${nodes[$i]}"
        local name="${node_names[$i]}"
        
        # Test HTTP health endpoint
        if timeout 2 curl -s "http://$ip:9090/health" >/dev/null 2>&1; then
            print_success "$name ($ip) HTTP health: OK"
        else
            print_warning "$name ($ip) HTTP health: FAIL"
        fi
        
        # Test P2P port (UDP)
        if timeout 2 bash -c "echo test > /dev/udp/$ip/8080" 2>/dev/null; then
            print_success "$name ($ip) UDP P2P: OK"
        else
            print_warning "$name ($ip) UDP P2P: FAIL"
        fi
        
        # Ping test
        if timeout 2 ping -c 1 "$ip" >/dev/null 2>&1; then
            print_success "$name ($ip) Ping: OK"
        else
            print_error "$name ($ip) Ping: FAIL"
        fi
    done
    
    echo ""
}

# Monitor DNS resolution
monitor_dns() {
    print_info "=== DNS RESOLUTION ==="
    
    local hostnames=("bootstrap-node" "mixnode-1" "mixnode-2" "mixnode-3")
    
    for hostname in "${hostnames[@]}"; do
        if nslookup "$hostname" >/dev/null 2>&1; then
            local ip=$(nslookup "$hostname" | grep -A1 "Name:" | tail -1 | awk '{print $2}')
            print_success "$hostname resolves to $ip"
        else
            print_warning "$hostname DNS resolution failed"
        fi
    done
    
    echo ""
}

# Main monitoring loop
main_monitor() {
    print_info "Starting network monitoring..."
    
    while true; do
        clear
        echo "=== NYM MIXNODE NETWORK MONITOR ==="
        echo "Time: $(date)"
        echo "Monitoring network: 172.25.0.0/16"
        echo ""
        
        monitor_connections
        test_connectivity
        monitor_dns
        monitor_packets
        
        print_info "Press Ctrl+C to stop monitoring"
        sleep 10
    done
}

# If script is run with arguments, execute specific function
if [ $# -gt 0 ]; then
    case "$1" in
        "connections")
            monitor_connections
            ;;
        "connectivity")
            test_connectivity
            ;;
        "dns")
            monitor_dns
            ;;
        "packets")
            monitor_packets
            ;;
        *)
            echo "Usage: $0 [connections|connectivity|dns|packets]"
            echo "Or run without arguments for continuous monitoring"
            ;;
    esac
else
    main_monitor
fi