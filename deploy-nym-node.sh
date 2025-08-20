
#!/bin/bash

# Nym Mixnode Deployment Script for Ubuntu VPS
# Production-ready deployment script with VPS-specific optimizations

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configuration variables
SCRIPT_VERSION="1.0.1"
NYM_USER="nym"
NYM_HOME="/var/lib/nym"
NYM_CONFIG="/etc/nym"
NYM_LOGS="/var/log/nym"
NYM_BINARY="/usr/local/bin/nym-mixnode-rs"
GITHUB_REPO="https://github.com/vx0net/nym-mixnode-rs.git"
RUST_VERSION="1.80.0"
DOCKER_COMPOSE_VERSION="2.21.0"

# Global variables for configuration
NODE_TYPE=""
NODE_REGION=""
STAKE_AMOUNT=""
BOOTSTRAP_PEERS=""
WORKER_THREADS=4
MAX_PACKET_RATE=30000
PUBLIC_IP=""
HOSTNAME=""
ENABLE_SWAP=false
MEMORY_LIMIT="4G"
RUST_BUILD_JOBS=0

# Print functions
print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${CYAN}ℹ $1${NC}"
}

# Logging function
log_action() {
    local message="$1"
    local log_file="/tmp/nym-deploy-$(date +%Y%m%d-%H%M%S).log"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $message" | tee -a "$log_file"
}

# Check if running as root
check_root() {
    if [[ $EUID -eq 0 ]]; then
        print_error "This script should not be run as root for security reasons"
        print_info "Please run as a regular user with sudo privileges"
        print_info "If you need to create a user: adduser username && usermod -aG sudo username"
        exit 1
    fi

    # Test sudo access
    if ! sudo -n true 2>/dev/null; then
        print_warning "Testing sudo access..."
        if ! sudo true; then
            print_error "This script requires sudo privileges"
            print_info "Ensure your user is in the sudo group: sudo usermod -aG sudo \$USER"
            exit 1
        fi
    fi

    log_action "Root check passed - running as non-root user with sudo"
}

# Detect VPS provider and get public IP
detect_vps_environment() {
    print_info "Detecting VPS environment..."

    # Try multiple methods to get public IP
    PUBLIC_IP=""

    # Method 1: curl ipinfo.io
    if command -v curl >/dev/null 2>&1; then
        PUBLIC_IP=$(curl -s --connect-timeout 5 ipinfo.io/ip 2>/dev/null || echo "")
    fi

    # Method 2: curl ifconfig.me as fallback
    if [[ -z "$PUBLIC_IP" ]] && command -v curl >/dev/null 2>&1; then
        PUBLIC_IP=$(curl -s --connect-timeout 5 ifconfig.me 2>/dev/null || echo "")
    fi

    # Method 3: dig as second fallback
    if [[ -z "$PUBLIC_IP" ]] && command -v dig >/dev/null 2>&1; then
        PUBLIC_IP=$(dig +short myip.opendns.com @resolver1.opendns.com 2>/dev/null || echo "")
    fi

    # Method 4: wget as final fallback
    if [[ -z "$PUBLIC_IP" ]] && command -v wget >/dev/null 2>&1; then
        PUBLIC_IP=$(wget -qO- --timeout=5 ipinfo.io/ip 2>/dev/null || echo "")
    fi

    if [[ -n "$PUBLIC_IP" ]]; then
        print_success "Detected public IP: $PUBLIC_IP"
    else
        print_warning "Could not auto-detect public IP"
        while true; do
            read -p "Please enter your VPS public IP: " PUBLIC_IP
            if [[ "$PUBLIC_IP" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
                print_success "Using IP: $PUBLIC_IP"
                break
            else
                print_error "Please enter a valid IPv4 address"
            fi
        done
    fi

    # Get hostname
    HOSTNAME=$(hostname -f 2>/dev/null || hostname 2>/dev/null || echo "nym-node")
    print_info "System hostname: $HOSTNAME"

    # Detect VPS provider
    local provider="Unknown"
    if [[ -f /sys/class/dmi/id/sys_vendor ]]; then
        local vendor=$(cat /sys/class/dmi/id/sys_vendor 2>/dev/null)
        case "$vendor" in
            "DigitalOcean") provider="DigitalOcean" ;;
            "Amazon EC2") provider="AWS EC2" ;;
            "Google") provider="Google Cloud" ;;
            "Microsoft Corporation") provider="Azure" ;;
            "Linode") provider="Linode" ;;
            "Vultr") provider="Vultr" ;;
        esac
    fi

    print_info "VPS Provider: $provider"
    log_action "VPS Environment detected - Provider: $provider, IP: $PUBLIC_IP"
}

# Create swap file for low memory systems
setup_swap_space() {
    if [[ "$ENABLE_SWAP" == "true" ]]; then
        print_info "Setting up swap space for low memory system..."
        
        # Check if swap is already active
        if swapon --show | grep -q "/swapfile"; then
            print_info "Swap file already exists"
            return 0
        fi
        
        # Determine swap size based on RAM
        local ram_mb=$(free -m | awk '/^Mem:/{print $2}')
        local swap_size_mb
        
        if [[ $ram_mb -lt 900 ]]; then
            swap_size_mb=2048  # 2GB swap for <1GB RAM
        elif [[ $ram_mb -lt 2048 ]]; then
            swap_size_mb=1536  # 1.5GB swap for 1-2GB RAM
        else
            swap_size_mb=1024  # 1GB swap for 2-4GB RAM
        fi
        
        print_info "Creating ${swap_size_mb}MB swap file..."
        
        # Create swap file
        if ! sudo fallocate -l "${swap_size_mb}M" /swapfile; then
            print_warning "fallocate failed, trying dd..."
            if ! sudo dd if=/dev/zero of=/swapfile bs=1M count="$swap_size_mb" 2>/dev/null; then
                print_error "Failed to create swap file"
                return 1
            fi
        fi
        
        # Set permissions
        sudo chmod 600 /swapfile
        
        # Set up swap
        if sudo mkswap /swapfile && sudo swapon /swapfile; then
            print_success "Swap space created and activated (${swap_size_mb}MB)"
            
            # Make swap permanent
            if ! grep -q "/swapfile" /etc/fstab; then
                echo "/swapfile none swap sw 0 0" | sudo tee -a /etc/fstab
            fi
            
            # Optimize swappiness for better performance
            echo "vm.swappiness=10" | sudo tee -a /etc/sysctl.conf
            sudo sysctl vm.swappiness=10
            
            log_action "Swap space created: ${swap_size_mb}MB"
        else
            print_error "Failed to activate swap"
            return 1
        fi
    fi
}

# Check system requirements with VPS optimizations
check_system() {
    print_info "Checking system requirements..."

    # Check Ubuntu version
    if ! grep -q "Ubuntu" /etc/os-release; then
        print_error "This script is designed for Ubuntu systems"
        exit 1
    fi

    local ubuntu_version=$(lsb_release -rs 2>/dev/null || echo "unknown")
    if [[ "$ubuntu_version" != "unknown" ]]; then
        if [[ $(echo "$ubuntu_version < 20.04" | bc -l 2>/dev/null || echo "1") -eq 1 ]] && [[ "$ubuntu_version" != "unknown" ]]; then
            print_error "Ubuntu 20.04 or newer is required"
            exit 1
        fi
        print_success "Ubuntu $ubuntu_version detected"
    else
        print_warning "Could not determine Ubuntu version, proceeding with caution"
    fi

    # Check system resources
    local ram_mb=$(free -m | awk '/^Mem:/{print $2}')
    local ram_gb=$((ram_mb / 1024))
    local cpu_cores=$(nproc)
    local disk_gb=$(df / | awk 'NR==2{printf "%.0f", $4/1024/1024}')

    print_info "System Resources:"
    print_info "  RAM: ${ram_gb}GB (${ram_mb}MB)"
    print_info "  CPU Cores: $cpu_cores"
    print_info "  Available Disk: ${disk_gb}GB"

    # Optimize for low memory systems
    if [[ $ram_gb -lt 1 ]] || [[ $ram_mb -lt 900 ]]; then
        print_warning "Less than 1GB RAM detected - applying extreme memory optimization"
        WORKER_THREADS=1
        MAX_PACKET_RATE=5000
        ENABLE_SWAP=true
        MEMORY_LIMIT="800M"
        RUST_BUILD_JOBS=1
        print_info "Ultra-low memory configuration applied"
    elif [[ $ram_gb -lt 2 ]]; then
        print_warning "Less than 2GB RAM - applying memory optimizations"
        WORKER_THREADS=1
        MAX_PACKET_RATE=8000
        ENABLE_SWAP=true
        MEMORY_LIMIT="1.5G"
        RUST_BUILD_JOBS=1
        print_info "Low memory optimization applied"
    elif [[ $ram_gb -lt 4 ]]; then
        print_warning "4GB+ RAM recommended for optimal performance"
        # Adjust worker threads for low memory systems
        WORKER_THREADS=2
        MAX_PACKET_RATE=15000
        MEMORY_LIMIT="3G"
        print_info "Adjusted settings for lower memory system"
    else
        MEMORY_LIMIT="4G"
    fi

    if [[ $cpu_cores -lt 2 ]]; then
        print_warning "2+ CPU cores recommended"
        WORKER_THREADS=1
    fi

    if [[ $disk_gb -lt 20 ]]; then
        print_error "Minimum 20GB disk space required"
        exit 1
    elif [[ $disk_gb -lt 50 ]]; then
        print_warning "50GB+ disk space recommended"
    fi

    # Check if running in a container
    if [[ -f /.dockerenv ]]; then
        print_warning "Running inside Docker container - some features may not work"
    fi

    # Check for virtualization
    local virt_type="none"
    if command -v systemd-detect-virt >/dev/null 2>&1; then
        virt_type=$(systemd-detect-virt 2>/dev/null || echo "none")
    fi
    print_info "Virtualization: $virt_type"

    log_action "System check completed - RAM: ${ram_gb}GB, CPU: $cpu_cores, Disk: ${disk_gb}GB"
}

# Install system dependencies with error handling
install_dependencies() {
    print_info "Installing system dependencies..."

    # Update package lists
    print_info "Updating package lists..."
    if ! sudo apt update; then
        print_error "Failed to update package lists"
        print_info "Trying with different mirror..."
        sudo apt update --fix-missing || {
            print_error "Package update failed completely"
            exit 1
        }
    fi

    # Install essential packages
    local essential_packages=(
        curl
        wget
        git
        build-essential
        pkg-config
        libssl-dev
        ufw
        jq
        netcat-openbsd
        sqlite3
        bc
        htop
        iotop
        net-tools
        dnsutils
        ca-certificates
        gnupg
        lsb-release
        software-properties-common
        apt-transport-https
    )

    print_info "Installing essential packages..."
    for package in "${essential_packages[@]}"; do
        if ! dpkg -l | grep -q "^ii.*$package "; then
            print_info "Installing $package..."
            if ! sudo apt install -y "$package"; then
                print_warning "Failed to install $package, trying to continue..."
            fi
        else
            print_info "$package already installed"
        fi
    done

    # Install Rust with specific version
    if ! command -v cargo &> /dev/null; then
        print_info "Installing Rust toolchain..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain "$RUST_VERSION"
        source "$HOME/.cargo/env"

        # Add cargo to PATH for current session
        export PATH="$HOME/.cargo/bin:$PATH"

        # Verify installation
        if command -v cargo &> /dev/null; then
            print_success "Rust $(rustc --version) installed"
        else
            print_error "Rust installation failed"
            exit 1
        fi
    else
        print_success "Rust already installed: $(rustc --version)"
    fi

    # Install Docker
    if ! command -v docker &> /dev/null; then
        print_info "Installing Docker..."

        # Remove old versions
        sudo apt remove -y docker docker-engine docker.io containerd runc 2>/dev/null || true

        # Add Docker's official GPG key
        curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg

        # Add the repository
        echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

        # Install Docker
        sudo apt update
        sudo apt install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin

        # Add user to docker group
        sudo usermod -aG docker "$USER"

        # Enable and start docker
        sudo systemctl enable docker
        sudo systemctl start docker

        print_success "Docker installed"
    else
        print_success "Docker already installed: $(docker --version)"
    fi

    # Install Docker Compose
    if ! command -v docker-compose &> /dev/null; then
        print_info "Installing Docker Compose..."

        # Download docker-compose
        sudo curl -L "https://github.com/docker/compose/releases/download/v${DOCKER_COMPOSE_VERSION}/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose

        # Make executable
        sudo chmod +x /usr/local/bin/docker-compose

        # Verify installation
        if command -v docker-compose &> /dev/null; then
            print_success "Docker Compose installed: $(docker-compose --version)"
        else
            print_error "Docker Compose installation failed"
            exit 1
        fi
    else
        print_success "Docker Compose already installed: $(docker-compose --version)"
    fi

    log_action "Dependencies installation completed"
}

# Configure firewall with VPS-specific rules
configure_firewall() {
    print_info "Configuring firewall for VPS environment..."

    # Reset UFW to default state
    sudo ufw --force reset

    # Set default policies
    sudo ufw default deny incoming
    sudo ufw default allow outgoing

    # SSH access - be careful not to lock ourselves out
    local ssh_port=22
    if sudo netstat -tlnp | grep -q ":2222 "; then
        ssh_port=2222
        print_info "Detected SSH on port 2222"
    fi

    sudo ufw allow "$ssh_port/tcp" comment "SSH"

    # Nym node ports
    sudo ufw allow 8080/udp comment "Nym P2P"
    sudo ufw allow 9090/tcp comment "Nym HTTP API"
    sudo ufw allow 9091/tcp comment "Nym Metrics"

    # Additional VPS-specific rules
    sudo ufw allow out 53 comment "DNS"
    sudo ufw allow out 80/tcp comment "HTTP out"
    sudo ufw allow out 443/tcp comment "HTTPS out"

    # Rate limiting for SSH
    sudo ufw limit "$ssh_port/tcp" comment "SSH rate limit"

    # Enable firewall
    sudo ufw --force enable

    # Verify firewall status
    print_info "Firewall rules configured:"
    sudo ufw status numbered

    log_action "Firewall configured successfully"
}

# Create system user and directories with proper permissions
create_user_and_dirs() {
    print_info "Creating nym user and directories..."

    # Create nym user if it doesn't exist
    if ! id "$NYM_USER" &>/dev/null; then
        sudo useradd -r -s /bin/false -d "$NYM_HOME" -c "Nym Mixnode Service" "$NYM_USER"
        print_success "Created nym user"
    else
        print_info "Nym user already exists"
    fi

    # Create directories with proper permissions
    local dirs=("$NYM_HOME" "$NYM_CONFIG" "$NYM_LOGS")
    for dir in "${dirs[@]}"; do
        if [[ ! -d "$dir" ]]; then
            sudo mkdir -p "$dir"
            print_info "Created directory: $dir"
        fi
    done

    # Set ownership and permissions
    sudo chown -R "$NYM_USER:$NYM_USER" "$NYM_HOME" "$NYM_CONFIG" "$NYM_LOGS"
    sudo chmod 750 "$NYM_HOME" "$NYM_CONFIG"
    sudo chmod 755 "$NYM_LOGS"

    # Create subdirectories for node type (will be set later)
    print_success "Directories created and configured"
    log_action "User and directories created successfully"
}

# Get user configuration with VPS-specific options
get_user_config() {
    print_header "Node Configuration"

    # Node type selection
    echo -e "${CYAN}Select node type:${NC}"
    echo "1) Bootstrap Node (Network entry point - requires high availability)"
    echo "2) Regular Mixnode (Standard node - participates in mixing)"
    echo ""
    while true; do
        read -p "Enter choice [1-2]: " node_type
        case $node_type in
            1)
                NODE_TYPE="bootstrap"
                print_success "Selected: Bootstrap Node"
                print_warning "Bootstrap nodes require high uptime and reliable connectivity"
                break
                ;;
            2)
                NODE_TYPE="mixnode"
                print_success "Selected: Regular Mixnode"
                break
                ;;
            *)
                print_error "Please enter 1 or 2"
                ;;
        esac
    done

    # Get node region based on VPS location
    echo ""
    echo -e "${CYAN}Select your geographic region (choose closest to your VPS location):${NC}"
    echo "1) North America (US, Canada)"
    echo "2) Europe (EU, UK, etc.)"
    echo "3) Asia (China, Japan, India, etc.)"
    echo "4) Oceania (Australia, New Zealand)"
    echo "5) South America (Brazil, Argentina, etc.)"
    echo "6) Africa (South Africa, etc.)"
    echo ""
    print_info "Your VPS IP: $PUBLIC_IP"

    while true; do
        read -p "Enter choice [1-6]: " region_choice
        case $region_choice in
            1) NODE_REGION="NorthAmerica"; break ;;
            2) NODE_REGION="Europe"; break ;;
            3) NODE_REGION="Asia"; break ;;
            4) NODE_REGION="Oceania"; break ;;
            5) NODE_REGION="SouthAmerica"; break ;;
            6) NODE_REGION="Africa"; break ;;
            *) print_error "Please enter 1-6" ;;
        esac
    done
    print_success "Selected region: $NODE_REGION"

    # Get stake amount for regular nodes
    if [[ "$NODE_TYPE" == "mixnode" ]]; then
        echo ""
        print_info "Stake amount determines your node's selection probability"
        while true; do
            read -p "Enter your stake amount (minimum 1000): " STAKE_AMOUNT
            if [[ "$STAKE_AMOUNT" =~ ^[0-9]+$ ]] && [[ "$STAKE_AMOUNT" -ge 1000 ]]; then
                print_success "Stake amount: $STAKE_AMOUNT"
                break
            else
                print_error "Please enter a valid number >= 1000"
            fi
        done
    fi

    # Bootstrap peers for regular nodes
    if [[ "$NODE_TYPE" == "mixnode" ]]; then
        echo ""
        print_info "Bootstrap peers are required for network discovery"
        print_info "Use well-known bootstrap nodes or trusted community nodes"
        echo ""
        echo "Common bootstrap peers:"
        echo "  • bootstrap1.nymtech.net:8080 (Official)"
        echo "  • bootstrap2.nymtech.net:8080 (Official backup)"
        echo ""

        while true; do
            read -p "Enter bootstrap peers (comma-separated): " BOOTSTRAP_PEERS
            if [[ -n "$BOOTSTRAP_PEERS" ]] && [[ "$BOOTSTRAP_PEERS" =~ :[0-9]+$ ]]; then
                print_success "Bootstrap peers configured"
                break
            else
                print_error "Please enter valid bootstrap peers (host:port format)"
                print_info "Example: bootstrap1.nymtech.net:8080,bootstrap2.nymtech.net:8080"
            fi
        done
    fi

    # Performance settings based on VPS specs
    echo ""
    echo -e "${CYAN}Performance Configuration:${NC}"
    print_info "Current auto-detected settings based on your VPS specs:"
    print_info "  Worker threads: $WORKER_THREADS"
    print_info "  Max packet rate: $MAX_PACKET_RATE"

    echo ""
    read -p "Keep auto-detected performance settings? [Y/n]: " keep_auto
    if [[ "$keep_auto" =~ ^[Nn]$ ]]; then
        read -p "Worker threads [$WORKER_THREADS]: " input_threads
        WORKER_THREADS=${input_threads:-$WORKER_THREADS}

        read -p "Max packet rate [$MAX_PACKET_RATE]: " input_rate
        MAX_PACKET_RATE=${input_rate:-$MAX_PACKET_RATE}
    fi

    print_success "Performance settings: $WORKER_THREADS threads, $MAX_PACKET_RATE packets/sec"

    # Confirm configuration
    echo ""
    print_header "Configuration Summary"
    echo -e "${CYAN}Node Configuration:${NC}"
    echo "  Type: $NODE_TYPE"
    echo "  Region: $NODE_REGION"
    echo "  VPS IP: $PUBLIC_IP"
    [[ "$NODE_TYPE" == "mixnode" ]] && echo "  Stake: $STAKE_AMOUNT"
    [[ "$NODE_TYPE" == "mixnode" ]] && echo "  Bootstrap Peers: $BOOTSTRAP_PEERS"
    echo "  Worker Threads: $WORKER_THREADS"
    echo "  Max Packet Rate: $MAX_PACKET_RATE"
    echo "  Memory Limit: $MEMORY_LIMIT"
    [[ "$ENABLE_SWAP" == "true" ]] && echo "  Swap Space: Enabled (for low memory optimization)"

    echo ""
    read -p "Proceed with this configuration? [Y/n]: " confirm_config
    if [[ "$confirm_config" =~ ^[Nn]$ ]]; then
        print_info "Configuration cancelled. Please restart the script."
        exit 0
    fi

    log_action "Configuration completed - Type: $NODE_TYPE, Region: $NODE_REGION"
}

# Download and build Nym node with better error handling
build_nym_node() {
    print_info "Building Nym node from source..."

    # Ensure we have rust in PATH
    export PATH="$HOME/.cargo/bin:$PATH"
    source "$HOME/.cargo/env" 2>/dev/null || true

    # Clean up any existing build directory
    if [[ -d "nym-mixnode-rs" ]]; then
        print_info "Removing existing build directory..."
        rm -rf nym-mixnode-rs
    fi

    # Clone repository with depth 1 to save bandwidth
    print_info "Cloning repository..."
    if ! git clone --depth 1 "$GITHUB_REPO"; then
        print_error "Failed to clone repository"
        print_info "Please check your internet connection and repository URL"
        exit 1
    fi

    cd nym-mixnode-rs

    # Check if Cargo.toml exists
    if [[ ! -f "Cargo.toml" ]]; then
        print_error "Invalid repository - missing Cargo.toml"
        exit 1
    fi

    # Build release binary with memory optimizations
    print_info "Compiling binary (this may take 10-45 minutes depending on VPS specs)..."
    print_info "You can monitor progress in another terminal with: htop"

    # Set compilation flags optimized for memory usage
    export RUSTFLAGS="-C target-cpu=native -C link-arg=-s"
    export CARGO_INCREMENTAL=0
    
    # Set build jobs based on memory availability
    if [[ "$RUST_BUILD_JOBS" -gt 0 ]]; then
        export CARGO_BUILD_JOBS="$RUST_BUILD_JOBS"
        print_info "Using $RUST_BUILD_JOBS parallel build job(s) for low memory system"
    fi
    
    # Additional memory-saving environment variables
    export CARGO_TARGET_DIR="./target"
    export CARGO_HOME="$HOME/.cargo"
    
    # Clear memory before build
    sync && echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null 2>&1 || true

    if ! timeout 3600 cargo build --release --bin nym-mixnode-rs; then
        print_error "Compilation failed or timed out"
        
        if [[ "$ENABLE_SWAP" == "false" ]]; then
            print_info "Retrying with swap space enabled..."
            setup_swap_space
            
            # Retry build with swap
            if ! timeout 3600 cargo build --release --bin nym-mixnode-rs; then
                print_error "Compilation failed even with swap enabled"
                print_info "Your system may not have enough resources for compilation"
                print_info "Consider using a VPS with at least 1GB RAM + 2GB swap"
                exit 1
            fi
        else
            print_info "Build failed even with optimizations. Try:"
            print_info "1. Restart the VPS and run this script again"
            print_info "2. Use a VPS with more memory"
            print_info "3. Consider using pre-compiled binaries if available"
            exit 1
        fi
    fi

    # Verify binary was created
    if [[ ! -f "target/release/nym-mixnode-rs" ]]; then
        print_error "Binary not found after compilation"
        exit 1
    fi

    # Install binary
    sudo cp target/release/nym-mixnode-rs "$NYM_BINARY"
    sudo chmod +x "$NYM_BINARY"
    sudo chown root:root "$NYM_BINARY"

    # Verify installation
    if "$NYM_BINARY" --version >/dev/null 2>&1; then
        print_success "Nym node built and installed successfully"
    else
        print_error "Binary installation verification failed"
        exit 1
    fi

    # Clean up build directory to save space
    cd ..
    rm -rf nym-mixnode-rs

    log_action "Build completed successfully"
}

# Generate node keys with backup
generate_keys() {
    print_info "Generating cryptographic keys..."

    local config_dir="$NYM_CONFIG/$NODE_TYPE"
    sudo mkdir -p "$config_dir"

    # Generate keys based on node type
    local key_args="--output-dir $config_dir --key-type ed25519"
    if [[ "$NODE_TYPE" == "bootstrap" ]]; then
        key_args="$key_args --bootstrap-mode"
    fi

    # Generate keys as nym user
    if ! sudo -u "$NYM_USER" "$NYM_BINARY" keys generate $key_args; then
        print_error "Key generation failed"
        exit 1
    fi

    # Verify keys were created
    local required_keys=("node.key" "node.pub")
    for key_file in "${required_keys[@]}"; do
        if [[ ! -f "$config_dir/$key_file" ]]; then
            print_error "Required key file missing: $key_file"
            exit 1
        fi
    done

    # Create backup of keys
    local backup_dir="$NYM_CONFIG/backups/$(date +%Y%m%d-%H%M%S)"
    sudo mkdir -p "$backup_dir"
    sudo cp -r "$config_dir"/*.key "$config_dir"/*.pub "$backup_dir/" 2>/dev/null || true
    sudo chown -R "$NYM_USER:$NYM_USER" "$backup_dir"
    sudo chmod 600 "$backup_dir"/*

    print_success "Cryptographic keys generated and backed up"
    print_warning "IMPORTANT: Keep your keys secure and create additional backups!"
    print_info "Keys location: $config_dir"
    print_info "Backup location: $backup_dir"

    log_action "Keys generated and backed up successfully"
}

# Create configuration file with VPS optimizations
create_config() {
    print_info "Creating configuration file..."

    local config_dir="$NYM_CONFIG/$NODE_TYPE"
    local config_file="$config_dir/config.yaml"

    if [[ "$NODE_TYPE" == "bootstrap" ]]; then
        sudo tee "$config_file" > /dev/null << EOF
# Bootstrap Node Configuration - VPS Optimized
node:
    mode: bootstrap
    node_id_file: "$config_dir/node.key"
    listen_address: "0.0.0.0:8080"
    http_api_address: "0.0.0.0:9090"
    log_level: "info"
    public_ip: "$PUBLIC_IP"
    hostname: "$HOSTNAME"

bootstrap:
    authority_level: primary
    max_registered_peers: 50000
    peer_timeout: 300
    cleanup_interval: 3600
    topology_update_interval: 60
    enable_peer_validation: true

network:
    max_concurrent_connections: 5000
    connection_timeout: 30
    heartbeat_interval: 15
    message_timeout: 10
    keep_alive_interval: 300
    tcp_nodelay: true

rate_limiting:
    requests_per_minute: 2000
    burst_capacity: 200
    cleanup_interval: 300
    enable_adaptive_limiting: true
    ddos_protection: true

validation:
    require_signature: true
    minimum_stake: 1000
    geographic_validation: true
    capability_verification: true
    reputation_threshold: 0.5

storage:
    peer_database_path: "$NYM_HOME/$NODE_TYPE/peers.db"
    topology_cache_path: "$NYM_HOME/$NODE_TYPE/topology.cache"
    backup_interval: 3600
    database_pool_size: 10

metrics:
    enable_prometheus: true
    prometheus_bind_address: "0.0.0.0:9091"
    collection_interval: 30
    enable_telemetry: true

performance:
    worker_threads: $WORKER_THREADS
    io_threads: 1
    max_memory_usage: "$MEMORY_LIMIT"
    gc_interval: 300
    enable_memory_optimization: true

security:
    enable_tls: false
    rate_limiting: true
    connection_limits: true
    audit_logging: true
EOF
    else
        sudo tee "$config_file" > /dev/null << EOF
# Mixnode Configuration - VPS Optimized
node:
    mode: mixnode
    node_id_file: "$config_dir/node.key"
    listen_address: "0.0.0.0:8080"
    http_api_address: "0.0.0.0:9090"
    log_level: "info"
    region: "$NODE_REGION"
    stake_amount: $STAKE_AMOUNT
    public_ip: "$PUBLIC_IP"
    hostname: "$HOSTNAME"

p2p:
    bootstrap_peers:
$(echo "$BOOTSTRAP_PEERS" | tr ',' '\n' | sed 's/^/    - "/' | sed 's/$/"/')
    max_outbound_connections: 100
    max_inbound_connections: 200
    connection_timeout: 30
    heartbeat_interval: 15
    peer_exchange_interval: 120
    connection_retry_attempts: 3
    connection_retry_delay: 5

vrf:
    signing_key_file: "$config_dir/vrf.key"
    selection_cache_size: 1000
    path_length: 3
    enable_fast_selection: true

discovery:
    discovery_interval: 30
    peer_exchange_interval: 120
    topology_refresh_interval: 300
    max_discovery_peers: 200
    enable_gossip: true
    gossip_fanout: 3
    enable_mdns: false

sphinx:
    max_packet_rate: $MAX_PACKET_RATE
    worker_threads: $WORKER_THREADS
    memory_pool_size: 1000000
    enable_simd: true
    cover_traffic_ratio: 0.1
    packet_buffer_size: 2048
    processing_timeout: 5

load_balancer:
    strategy: "weighted_round_robin"
    health_check_interval: 30
    circuit_breaker_threshold: 0.5
    circuit_breaker_timeout: 60
    enable_adaptive_routing: true

rate_limiting:
    packets_per_second: 2000
    burst_capacity: 10000
    violation_threshold: 10
    ban_duration: 300
    enable_whitelist: true

security:
    enable_threat_detection: true
    ddos_protection: true
    intrusion_detection: true
    audit_logging: true
    enable_firewall_integration: true
    max_connections_per_ip: 50

metrics:
    enable_prometheus: true
    prometheus_bind_address: "0.0.0.0:9091"
    collection_interval: 30
    enable_telemetry: true
    detailed_metrics: true

storage:
    database_path: "$NYM_HOME/$NODE_TYPE/node.db"
    backup_interval: 3600
    log_retention_days: 7
    database_pool_size: 5

performance:
    io_threads: 1
    max_memory_usage: "$MEMORY_LIMIT"
    gc_interval: 300
    enable_optimizations: true
    enable_memory_optimization: true
EOF
    fi

    # Set proper ownership and permissions
    sudo chown "$NYM_USER:$NYM_USER" "$config_file"
    sudo chmod 640 "$config_file"

    print_success "Configuration file created: $config_file"
    log_action "Configuration file created successfully"
}

# Create systemd service with VPS optimizations
create_service() {
    print_info "Creating systemd service..."

    local service_name="nym-$NODE_TYPE"
    local service_file="/etc/systemd/system/$service_name.service"

    sudo tee "$service_file" > /dev/null << EOF
[Unit]
Description=Nym ${NODE_TYPE^} Node
Documentation=https://nymtech.net/docs
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=0

[Service]
Type=simple
User=$NYM_USER
Group=$NYM_USER
ExecStart=$NYM_BINARY start --config $NYM_CONFIG/$NODE_TYPE/config.yaml
ExecReload=/bin/kill -HUP \$MAINPID
Restart=always
RestartSec=10
TimeoutStartSec=60
TimeoutStopSec=30
KillMode=mixed
KillSignal=SIGTERM

# Resource limits
LimitNOFILE=65536
LimitNPROC=32768

# Performance settings  
CPUWeight=100
MemoryHigh=$MEMORY_LIMIT
MemoryMax=$MEMORY_LIMIT
IOWeight=100

# Security settings
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$NYM_HOME $NYM_LOGS $NYM_CONFIG
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictRealtime=true
RestrictNamespaces=true
LockPersonality=true
MemoryDenyWriteExecute=false
RemoveIPC=true
PrivateDevices=true
ProtectClock=true
ProtectHostname=true

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=nym-$NODE_TYPE

[Install]
WantedBy=multi-user.target
EOF

    # Reload systemd and enable service
    sudo systemctl daemon-reload
    sudo systemctl enable "$service_name"

    print_success "Systemd service created and enabled: $service_name"
    log_action "Systemd service created successfully"
}

# Start the node with health checks
start_node() {
    print_info "Starting Nym node..."

    local service_name="nym-$NODE_TYPE"

    # Start the service
    if ! sudo systemctl start "$service_name"; then
        print_error "Failed to start service"
        print_info "Checking logs..."
        sudo journalctl -u "$service_name" --no-pager -n 20
        exit 1
    fi

    # Wait for startup with progress indication
    print_info "Waiting for node to start up..."
    local max_wait=60
    local wait_time=0

    while [[ $wait_time -lt $max_wait ]]; do
        if sudo systemctl is-active --quiet "$service_name"; then
            print_success "Service started successfully"
            break
        fi
        sleep 2
        wait_time=$((wait_time + 2))
        echo -n "."
    done
    echo ""

    if [[ $wait_time -ge $max_wait ]]; then
        print_error "Service startup timeout"
        sudo journalctl -u "$service_name" --no-pager -n 20
        exit 1
    fi

    # Health checks
    print_info "Performing health checks..."
    sleep 5  # Additional time for full initialization

    # Test health endpoint
    local health_checks=0
    local max_health_checks=10

    while [[ $health_checks -lt $max_health_checks ]]; do
        if curl -s -f --connect-timeout 5 "http://localhost:9090/health" >/dev/null 2>&1; then
            print_success "Health endpoint responding"
            break
        fi
        sleep 3
        health_checks=$((health_checks + 1))
        echo -n "."
    done
    echo ""

    if [[ $health_checks -ge $max_health_checks ]]; then
        print_warning "Health endpoint not responding yet"
        print_info "This may be normal during initial startup"
    fi

    # Test metrics endpoint
    if curl -s -f --connect-timeout 5 "http://localhost:9091/metrics" >/dev/null 2>&1; then
        print_success "Metrics endpoint responding"
    else
        print_warning "Metrics endpoint not responding yet"
    fi

    # Show service status
    echo ""
    print_info "Service Status:"
    sudo systemctl status "$service_name" --no-pager -l

    # Show recent logs
    echo ""
    print_info "Recent Logs:"
    sudo journalctl -u "$service_name" --no-pager -n 10

    log_action "Node started successfully"
}

# Create comprehensive monitoring and management scripts
create_monitoring_script() {
    print_info "Creating monitoring and management tools..."

    # Main monitoring script
    local monitor_script="/usr/local/bin/nym-monitor"
    sudo tee "$monitor_script" > /dev/null << 'EOF'
#!/bin/bash

# Nym Node Monitoring Script - VPS Optimized

NODE_TYPE=$(ls /etc/nym/ | grep -v backups | head -n1)
SERVICE_NAME="nym-$NODE_TYPE"

print_header() {
    echo "============================================"
    echo "$1"
    echo "============================================"
}

print_status() {
    local status="$1"
    local message="$2"
    if [[ "$status" == "ok" ]]; then
        echo "✓ $message"
    elif [[ "$status" == "warn" ]]; then
        echo "⚠ $message"
    else
        echo "✗ $message"
    fi
}

# Main status check
print_header "Nym Node Status Monitor"
echo "Node Type: $NODE_TYPE"
echo "Service: $SERVICE_NAME"
echo "Time: $(date)"
echo ""

# Service status
print_header "Service Status"
if systemctl is-active --quiet "$SERVICE_NAME"; then
    uptime=$(systemctl show "$SERVICE_NAME" --property=ActiveEnterTimestamp --value)
    print_status "ok" "Service is running since $uptime"
else
    print_status "error" "Service is not running"
    echo "Last 5 log entries:"
    journalctl -u "$SERVICE_NAME" -n 5 --no-pager
fi

# Network connectivity
print_header "Network Status"
if curl -s -f --connect-timeout 5 http://localhost:9090/health >/dev/null 2>&1; then
    print_status "ok" "Health endpoint responding"
else
    print_status "error" "Health endpoint not responding"
fi

if curl -s -f --connect-timeout 5 http://localhost:9091/metrics >/dev/null 2>&1; then
    print_status "ok" "Metrics endpoint responding"
else
    print_status "warn" "Metrics endpoint not responding"
fi

# Port status
print_header "Port Status"
if ss -tulnp | grep -q ":8080.*udp"; then
    print_status "ok" "P2P port 8080/UDP is listening"
else
    print_status "error" "P2P port 8080/UDP not listening"
fi

if ss -tulnp | grep -q ":9090.*tcp"; then
    print_status "ok" "API port 9090/TCP is listening"
else
    print_status "error" "API port 9090/TCP not listening"
fi

# Peer connections (for mixnodes)
if [[ "$NODE_TYPE" == "mixnode" ]]; then
    print_header "Peer Connections"
    peer_count=$(curl -s --connect-timeout 5 http://localhost:9090/peers 2>/dev/null | jq '. | length' 2>/dev/null || echo
"unknown")
    if [[ "$peer_count" != "unknown" ]] && [[ "$peer_count" -gt 0 ]]; then
        print_status "ok" "Connected to $peer_count peers"
    elif [[ "$peer_count" == "0" ]]; then
        print_status "warn" "No peer connections"
    else
        print_status "warn" "Could not determine peer count"
    fi
fi

# Resource usage
print_header "Resource Usage"
if pgrep -f nym-mixnode-rs >/dev/null; then
    mem_usage=$(ps aux | grep nym-mixnode-rs | grep -v grep | awk '{print $6}')
    cpu_usage=$(ps aux | grep nym-mixnode-rs | grep -v grep | awk '{print $3}')
    if [[ -n "$mem_usage" ]]; then
        mem_mb=$((mem_usage / 1024))
        print_status "ok" "Memory: ${mem_mb}MB, CPU: ${cpu_usage}%"
    fi
else
    print_status "error" "Process not running"
fi

# Disk usage
disk_usage=$(df /var/lib/nym | awk 'NR==2{print $5}')
print_status "ok" "Disk usage: $disk_usage"

# System load
load_avg=$(uptime | awk -F'load average:' '{print $2}')
print_status "ok" "System load:$load_avg"

# Firewall status
print_header "Security Status"
if ufw status | grep -q "Status: active"; then
    print_status "ok" "Firewall is active"
else
    print_status "warn" "Firewall is not active"
fi

echo ""
print_header "Quick Commands"
echo "View logs: sudo journalctl -u $SERVICE_NAME -f"
echo "Restart node: sudo systemctl restart $SERVICE_NAME"
echo "Stop node: sudo systemctl stop $SERVICE_NAME"
echo "Node config: /etc/nym/$NODE_TYPE/config.yaml"
echo "Node data: /var/lib/nym/$NODE_TYPE/"
EOF

    # Management script
    local manage_script="/usr/local/bin/nym-manage"
    sudo tee "$manage_script" > /dev/null << 'EOF'
#!/bin/bash

# Nym Node Management Script

NODE_TYPE=$(ls /etc/nym/ | grep -v backups | head -n1)
SERVICE_NAME="nym-$NODE_TYPE"

case "${1:-}" in
    "start")
        echo "Starting Nym node..."
        sudo systemctl start "$SERVICE_NAME"
        ;;
    "stop")
        echo "Stopping Nym node..."
        sudo systemctl stop "$SERVICE_NAME"
        ;;
    "restart")
        echo "Restarting Nym node..."
        sudo systemctl restart "$SERVICE_NAME"
        ;;
    "status")
        systemctl status "$SERVICE_NAME"
        ;;
    "logs")
        sudo journalctl -u "$SERVICE_NAME" -f
        ;;
    "health")
        curl -s http://localhost:9090/health | jq . || echo "Health endpoint not responding"
        ;;
    "peers")
        if [[ "$NODE_TYPE" == "mixnode" ]]; then
            curl -s http://localhost:9090/peers | jq . || echo "Peers endpoint not responding"
        else
            echo "Peer information not available for bootstrap nodes"
        fi
        ;;
    "metrics")
        curl -s http://localhost:9091/metrics | head -20
        ;;
    "backup")
        backup_dir="/var/lib/nym/backups/$(date +%Y%m%d-%H%M%S)"
        echo "Creating backup in $backup_dir..."
        sudo mkdir -p "$backup_dir"
        sudo cp -r "/etc/nym/$NODE_TYPE" "$backup_dir/"
        sudo cp -r "/var/lib/nym/$NODE_TYPE" "$backup_dir/" 2>/dev/null || true
        echo "Backup completed"
        ;;
    *)
        echo "Nym Node Management"
        echo "Usage: $0 {start|stop|restart|status|logs|health|peers|metrics|backup}"
        ;;
esac
EOF

    # Make scripts executable
    sudo chmod +x "$monitor_script" "$manage_script"

    print_success "Monitoring tools created:"
    print_info "  nym-monitor - comprehensive status check"
    print_info "  nym-manage - node management commands"

    log_action "Monitoring scripts created successfully"
}

# Setup automatic updates and maintenance
setup_maintenance() {
    print_info "Setting up automatic maintenance..."

    # Create maintenance script
    local maint_script="/usr/local/bin/nym-maintenance"
    sudo tee "$maint_script" > /dev/null << 'EOF'
#!/bin/bash

# Nym Node Maintenance Script

NODE_TYPE=$(ls /etc/nym/ | grep -v backups | head -n1)
SERVICE_NAME="nym-$NODE_TYPE"
LOG_FILE="/var/log/nym/maintenance.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Rotate logs
log "Starting maintenance tasks"

# Clean old log files
find /var/log/nym -name "*.log" -mtime +7 -delete 2>/dev/null || true

# Clean old backups (keep last 7 days)
find /var/lib/nym/backups -type d -mtime +7 -exec rm -rf {} \; 2>/dev/null || true

# Database maintenance
if [[ -f "/var/lib/nym/$NODE_TYPE/node.db" ]]; then
    sqlite3 "/var/lib/nym/$NODE_TYPE/node.db" "VACUUM;" 2>/dev/null || true
    log "Database maintenance completed"
fi

# Check disk space
disk_usage=$(df /var/lib/nym | awk 'NR==2{print $5}' | tr -d '%')
if [[ "$disk_usage" -gt 80 ]]; then
    log "WARNING: Disk usage is ${disk_usage}%"
fi

# Service health check
if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    log "WARNING: Service $SERVICE_NAME is not running"
fi

log "Maintenance tasks completed"
EOF

    sudo chmod +x "$maint_script"

    # Create cron job for maintenance
    local cron_job="0 2 * * * /usr/local/bin/nym-maintenance"
    (sudo crontab -l 2>/dev/null | grep -v nym-maintenance; echo "$cron_job") | sudo crontab -

    print_success "Automatic maintenance configured (runs daily at 2 AM)"
    log_action "Maintenance setup completed"
}

# Display final information with VPS-specific details
show_final_info() {
    print_header "Deployment Complete"

    print_success "Nym $NODE_TYPE node deployed successfully on your VPS!"

    echo ""
    echo -e "${CYAN}Node Information:${NC}"
    echo "  Type: $NODE_TYPE"
    echo "  Region: $NODE_REGION"
    echo "  Public IP: $PUBLIC_IP"
    echo "  Hostname: $HOSTNAME"
    [[ "$NODE_TYPE" == "mixnode" ]] && echo "  Stake: $STAKE_AMOUNT"
    echo ""
    echo "  P2P Port: 8080/UDP"
    echo "  API Port: 9090/TCP"
    echo "  Metrics Port: 9091/TCP"

    echo ""
    echo -e "${CYAN}VPS Network Endpoints:${NC}"
    echo "  Health Check: http://$PUBLIC_IP:9090/health"
    echo "  Metrics: http://$PUBLIC_IP:9091/metrics"
    [[ "$NODE_TYPE" == "mixnode" ]] && echo "  Peers: http://$PUBLIC_IP:9090/peers"

    echo ""
    echo -e "${CYAN}Management Commands:${NC}"
    echo "  Monitor node: nym-monitor"
    echo "  Manage node: nym-manage {start|stop|restart|status|logs|health}"
    echo "  View logs: sudo journalctl -u nym-$NODE_TYPE -f"
    echo "  Service status: sudo systemctl status nym-$NODE_TYPE"

    echo ""
    echo -e "${CYAN}File Locations:${NC}"
    echo "  Configuration: $NYM_CONFIG/$NODE_TYPE/config.yaml"
    echo "  Keys: $NYM_CONFIG/$NODE_TYPE/"
    echo "  Data: $NYM_HOME/$NODE_TYPE/"
    echo "  Logs: $NYM_LOGS/"
    echo "  Backups: $NYM_CONFIG/backups/"

    echo ""
    echo -e "${CYAN}VPS Firewall Status:${NC}"
    sudo ufw status numbered

    echo ""
    echo -e "${YELLOW}Important Security Reminders:${NC}"
    echo "  • Your private keys are in: $NYM_CONFIG/$NODE_TYPE/"
    echo "  • Create secure backups of your keys immediately"
    echo "  • Monitor your VPS for security events"
    echo "  • Keep your system updated: sudo apt update && sudo apt upgrade"
    echo "  • Monitor resource usage regularly"

    echo ""
    echo -e "${YELLOW}VPS Maintenance:${NC}"
    echo "  • Automatic maintenance runs daily at 2 AM"
    echo "  • Monitor disk space: df -h"
    echo "  • Check memory usage: free -h"
    echo "  • Monitor network: ss -tulnp"
    [[ "$ENABLE_SWAP" == "true" ]] && echo "  • Swap space is configured for memory optimization"
    
    echo ""
    echo -e "${CYAN}Memory Optimization Status:${NC}"
    echo "  • Worker Threads: $WORKER_THREADS"
    echo "  • Memory Limit: $MEMORY_LIMIT"
    [[ "$ENABLE_SWAP" == "true" ]] && echo "  • Swap Space: Active"
    [[ "$RUST_BUILD_JOBS" -gt 0 ]] && echo "  • Build was optimized for low memory ($RUST_BUILD_JOBS jobs)"

    if [[ "$NODE_TYPE" == "bootstrap" ]]; then
        echo ""
        echo -e "${CYAN}Bootstrap Node Specific:${NC}"
        print_warning "You are running a critical network infrastructure component"
        echo "  • Ensure maximum uptime and reliability"
        echo "  • Monitor for high connection loads"
        echo "  • Consider implementing monitoring alerts"
        echo "  • Your bootstrap endpoint: $PUBLIC_IP:8080"
        print_info "Other nodes can use your bootstrap with: $PUBLIC_IP:8080"
    fi

    echo ""
    echo -e "${GREEN}Next Steps:${NC}"
    echo "1. Verify node is running: nym-monitor"
    echo "2. Check logs: sudo journalctl -u nym-$NODE_TYPE -f"
    echo "3. Test connectivity: curl http://$PUBLIC_IP:9090/health"
    echo "4. Backup your keys securely"
    echo "5. Set up monitoring alerts (recommended for production)"

    print_header "Deployment Log"
    print_info "Complete deployment log saved to: /tmp/nym-deploy-*.log"

    log_action "Deployment completed successfully - Node type: $NODE_TYPE, IP: $PUBLIC_IP"
}

# Main execution with comprehensive error handling
main() {
    # Trap to handle script interruption
    trap 'print_error "Script interrupted"; exit 1' INT TERM

    print_header "Nym Node VPS Deployment Script v$SCRIPT_VERSION"

    print_info "This script will deploy a production-ready Nym mixnode on your Ubuntu VPS"
    print_warning "Ensure you have:"
    print_warning "  • A stable internet connection"
    print_warning "  • At least 512MB RAM and 20GB disk space"
    print_warning "  • Sudo privileges"
    print_warning "  • Ports 8080/UDP and 9090/TCP accessible"
    print_info "Note: Systems with less than 2GB RAM will use memory optimizations and swap space"

    echo ""
    read -p "Continue with VPS deployment? [y/N]: " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        print_info "Deployment cancelled"
        exit 0
    fi

    log_action "Starting Nym node deployment"

    # Pre-flight checks
    print_header "System Checks"
    check_root
    detect_vps_environment
    check_system

    # User configuration
    get_user_config

    # Main deployment
    print_header "VPS Deployment"
    install_dependencies
    setup_swap_space
    configure_firewall
    create_user_and_dirs
    build_nym_node
    generate_keys
    create_config
    create_service
    start_node
    create_monitoring_script
    setup_maintenance

    # Final steps
    show_final_info

    print_header "VPS Deployment Successful"
    print_success "Your Nym $NODE_TYPE node is now running on your VPS!"
    print_success "Public endpoint: $PUBLIC_IP:8080"

    log_action "VPS deployment completed successfully"
}

# Execute main function with all arguments
main "$@"