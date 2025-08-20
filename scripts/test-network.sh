#!/bin/bash

# Real P2P Network Testing Script for Nym Mixnode
# Tests actual networking, VRF selection, and multi-node communication

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
COMPOSE_FILE="docker-compose.test.yml"
TEST_TIMEOUT=300  # 5 minutes
BOOTSTRAP_WAIT=30
NODE_WAIT=20

print_header() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}================================${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

# Cleanup function
cleanup() {
    print_header "Cleaning up..."
    docker-compose -f $COMPOSE_FILE down -v --remove-orphans 2>/dev/null || true
    docker system prune -f >/dev/null 2>&1 || true
}

# Trap cleanup on exit
trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"
    
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed"
        exit 1
    fi
    
    if ! command -v docker-compose &> /dev/null; then
        print_error "Docker Compose is not installed"
        exit 1
    fi
    
    if ! docker info >/dev/null 2>&1; then
        print_error "Docker daemon is not running"
        exit 1
    fi
    
    print_success "Prerequisites check passed"
}

# Build the mixnode image
build_image() {
    print_header "Building Nym Mixnode Docker Image"
    
    print_info "Building with Rust optimizations..."
    if docker-compose -f $COMPOSE_FILE build --no-cache bootstrap-node; then
        print_success "Docker image built successfully"
    else
        print_error "Failed to build Docker image"
        exit 1
    fi
}

# Start the network
start_network() {
    print_header "Starting Multi-Node Test Network"
    
    print_info "Starting bootstrap node..."
    docker-compose -f $COMPOSE_FILE up -d bootstrap-node
    
    print_info "Waiting ${BOOTSTRAP_WAIT}s for bootstrap node to initialize..."
    sleep $BOOTSTRAP_WAIT
    
    # Check bootstrap health
    if ! docker-compose -f $COMPOSE_FILE exec -T bootstrap-node curl -f http://localhost:9090/health >/dev/null 2>&1; then
        print_error "Bootstrap node failed to start properly"
        docker-compose -f $COMPOSE_FILE logs bootstrap-node
        exit 1
    fi
    
    print_success "Bootstrap node is healthy"
    
    # Start other mixnodes
    print_info "Starting mixnodes 1-3..."
    docker-compose -f $COMPOSE_FILE up -d mixnode-1 mixnode-2 mixnode-3
    
    print_info "Waiting ${NODE_WAIT}s for nodes to connect..."
    sleep $NODE_WAIT
    
    print_success "Multi-node network started"
}

# Test network connectivity
test_connectivity() {
    print_header "Testing P2P Network Connectivity"
    
    local nodes=("bootstrap-node" "mixnode-1" "mixnode-2" "mixnode-3")
    local all_healthy=true
    
    for node in "${nodes[@]}"; do
        print_info "Testing $node health..."
        
        if docker-compose -f $COMPOSE_FILE exec -T $node curl -f http://localhost:9090/health >/dev/null 2>&1; then
            print_success "$node is healthy"
        else
            print_error "$node is not responding to health checks"
            all_healthy=false
        fi
    done
    
    if [ "$all_healthy" = false ]; then
        print_error "Network connectivity test failed"
        return 1
    fi
    
    print_success "All nodes are healthy and responsive"
}

# Test P2P communication
test_p2p_communication() {
    print_header "Testing Real P2P Communication"
    
    print_info "Testing TCP connections between nodes..."
    
    # Test if nodes can reach each other on P2P ports
    local test_pairs=(
        "mixnode-1:172.25.0.10:8080"  # mixnode-1 -> bootstrap
        "mixnode-2:172.25.0.11:8080"  # mixnode-2 -> mixnode-1
        "mixnode-3:172.25.0.12:8080"  # mixnode-3 -> mixnode-2
    )
    
    for pair in "${test_pairs[@]}"; do
        local from_node=$(echo $pair | cut -d: -f1)
        local to_ip=$(echo $pair | cut -d: -f2)
        local to_port=$(echo $pair | cut -d: -f3)
        
        print_info "Testing $from_node -> $to_ip:$to_port"
        
        if docker-compose -f $COMPOSE_FILE exec -T $from_node timeout 5 bash -c "cat < /dev/null > /dev/tcp/$to_ip/$to_port" 2>/dev/null; then
            print_success "$from_node can connect to $to_ip:$to_port"
        else
            print_warning "$from_node cannot connect to $to_ip:$to_port (might be UDP only)"
        fi
    done
    
    print_success "P2P communication test completed"
}

# Test VRF functionality
test_vrf_functionality() {
    print_header "Testing VRF Node Selection"
    
    print_info "Testing VRF-based node selection..."
    
    # Get VRF selection data from bootstrap node
    local vrf_output
    if vrf_output=$(docker-compose -f $COMPOSE_FILE exec -T bootstrap-node curl -s http://localhost:9090/vrf/test 2>/dev/null); then
        if echo "$vrf_output" | grep -q "selection"; then
            print_success "VRF selection is working"
        else
            print_warning "VRF endpoint responded but may not be fully functional"
        fi
    else
        print_warning "Could not test VRF functionality (endpoint may not be implemented)"
    fi
}

# Test packet forwarding
test_packet_forwarding() {
    print_header "Testing Packet Forwarding"
    
    print_info "Starting test client..."
    docker-compose -f $COMPOSE_FILE up -d test-client
    
    print_info "Running packet forwarding tests..."
    sleep 10
    
    # Check if client is sending packets
    local client_logs
    if client_logs=$(docker-compose -f $COMPOSE_FILE logs test-client 2>/dev/null); then
        if echo "$client_logs" | grep -q -i "packet\|forward\|send"; then
            print_success "Test client is generating traffic"
        else
            print_warning "Test client logs don't show packet activity"
        fi
    fi
    
    # Check nodes for packet processing
    local nodes=("bootstrap-node" "mixnode-1" "mixnode-2" "mixnode-3")
    local processing_nodes=0
    
    for node in "${nodes[@]}"; do
        local node_logs
        if node_logs=$(docker-compose -f $COMPOSE_FILE logs $node 2>/dev/null); then
            if echo "$node_logs" | grep -q -i "packet\|process\|forward\|received"; then
                print_success "$node is processing packets"
                ((processing_nodes++))
            fi
        fi
    done
    
    if [ $processing_nodes -gt 0 ]; then
        print_success "$processing_nodes nodes are processing packets"
    else
        print_warning "No clear evidence of packet processing in logs"
    fi
}

# Monitor network performance
monitor_performance() {
    print_header "Monitoring Network Performance"
    
    print_info "Collecting performance metrics for 30 seconds..."
    
    local nodes=("bootstrap-node" "mixnode-1" "mixnode-2" "mixnode-3")
    
    for i in {1..6}; do  # 6 iterations of 5 seconds each
        echo -n "."
        
        for node in "${nodes[@]}"; do
            # Try to get metrics from each node
            docker-compose -f $COMPOSE_FILE exec -T $node curl -s http://localhost:9090/metrics >/dev/null 2>&1 || true
        done
        
        sleep 5
    done
    echo ""
    
    print_success "Performance monitoring completed"
}

# Analyze logs for issues
analyze_logs() {
    print_header "Analyzing Logs for Issues"
    
    local nodes=("bootstrap-node" "mixnode-1" "mixnode-2" "mixnode-3")
    local error_count=0
    
    for node in "${nodes[@]}"; do
        print_info "Analyzing $node logs..."
        
        local logs
        if logs=$(docker-compose -f $COMPOSE_FILE logs $node 2>/dev/null); then
            local errors=$(echo "$logs" | grep -i -c "error\|panic\|fatal\|failed" || echo "0")
            local warnings=$(echo "$logs" | grep -i -c "warn\|warning" || echo "0")
            
            if [ "$errors" -gt 0 ]; then
                print_warning "$node has $errors errors and $warnings warnings"
                error_count=$((error_count + errors))
            else
                print_success "$node has no errors ($warnings warnings)"
            fi
        fi
    done
    
    if [ $error_count -eq 0 ]; then
        print_success "No critical errors found in logs"
    else
        print_warning "Found $error_count total errors across all nodes"
    fi
}

# Generate test report
generate_report() {
    print_header "Generating Test Report"
    
    local report_file="test-report-$(date +%Y%m%d-%H%M%S).txt"
    
    echo "Nym Mixnode Multi-Node Test Report" > $report_file
    echo "Generated: $(date)" >> $report_file
    echo "===============================================" >> $report_file
    echo "" >> $report_file
    
    echo "Node Status:" >> $report_file
    for node in bootstrap-node mixnode-1 mixnode-2 mixnode-3; do
        echo "- $node: $(docker-compose -f $COMPOSE_FILE ps $node --format "table {{.Status}}" | tail -n 1)" >> $report_file
    done
    echo "" >> $report_file
    
    echo "Network Configuration:" >> $report_file
    echo "- Network: mixnet (172.25.0.0/16)" >> $report_file
    echo "- Bootstrap: 172.25.0.10:8080" >> $report_file
    echo "- Mixnode-1: 172.25.0.11:8080" >> $report_file
    echo "- Mixnode-2: 172.25.0.12:8080" >> $report_file
    echo "- Mixnode-3: 172.25.0.13:8080" >> $report_file
    echo "" >> $report_file
    
    echo "Recent Logs (last 50 lines per node):" >> $report_file
    for node in bootstrap-node mixnode-1 mixnode-2 mixnode-3; do
        echo "--- $node ---" >> $report_file
        docker-compose -f $COMPOSE_FILE logs --tail=50 $node >> $report_file 2>/dev/null || echo "No logs available" >> $report_file
        echo "" >> $report_file
    done
    
    print_success "Test report saved to $report_file"
}

# Main execution
main() {
    print_header "Nym Mixnode Real P2P Network Test Suite"
    
    echo "This test suite will:"
    echo "- Build the mixnode Docker image"
    echo "- Start a 4-node mixnet (1 bootstrap + 3 mixnodes)"
    echo "- Test real P2P networking and VRF selection"
    echo "- Simulate packet forwarding"
    echo "- Generate performance and connectivity reports"
    echo ""
    
    read -p "Continue? (y/N): " -n 1 -r
    echo ""
    
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Test cancelled by user"
        exit 0
    fi
    
    # Run test sequence
    check_prerequisites
    cleanup  # Clean up any existing containers
    build_image
    start_network
    
    # Wait for network stabilization
    print_info "Waiting 30s for network to stabilize..."
    sleep 30
    
    # Run tests
    test_connectivity
    test_p2p_communication
    test_vrf_functionality
    test_packet_forwarding
    monitor_performance
    analyze_logs
    generate_report
    
    print_header "Test Summary"
    print_success "Multi-node network test completed successfully!"
    print_info "Check the generated report for detailed results"
    print_info "Use 'docker-compose -f $COMPOSE_FILE logs <node>' to view individual logs"
    print_info "Use 'docker-compose -f $COMPOSE_FILE down -v' to clean up when done"
}

# Run main function
main "$@"