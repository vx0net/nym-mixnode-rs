# Node Deployment Guide

  ## Prerequisites

  **System Requirements:**
  - Linux server (Ubuntu 20.04+ or CentOS 8+)
  - Minimum 2 CPU cores, 4GB RAM, 50GB storage
  - Static IP address with ports 8080/UDP and 9090/TCP accessible
  - Docker and Docker Compose installed

  **Network Configuration:**
  ```bash
  # Configure firewall rules
  sudo ufw allow 8080/udp comment "Mixnode P2P"
  sudo ufw allow 9090/tcp comment "Mixnode HTTP API"
  sudo ufw enable

  # Verify port accessibility
  ss -tuln | grep -E "(8080|9090)"

  Bootstrap Node Setup

  Initial Configuration

  1. Clone Repository and Build
  git clone https://github.com/your-org/nym-mixnode-rs.git
  cd nym-mixnode-rs

  # Build production binary
  cargo build --release --bin nym-mixnode-rs

  # Copy binary to system location
  sudo cp target/release/nym-mixnode-rs /usr/local/bin/

  2. Generate Bootstrap Node Keys
  # Create bootstrap configuration directory
  sudo mkdir -p /etc/nym/bootstrap
  sudo chown $(whoami):$(whoami) /etc/nym/bootstrap

  # Generate cryptographic identity
  nym-mixnode-rs keys generate \
    --output-dir /etc/nym/bootstrap \
    --key-type ed25519 \
    --bootstrap-mode

  3. Bootstrap Configuration File

  Create /etc/nym/bootstrap/config.yaml:
  # Bootstrap Node Configuration
  node:
    mode: bootstrap
    node_id_file: "/etc/nym/bootstrap/node.key"
    listen_address: "0.0.0.0:8080"
    http_api_address: "0.0.0.0:9090"
    log_level: "info"

  bootstrap:
    authority_level: primary
    max_registered_peers: 50000
    peer_timeout: 300
    cleanup_interval: 3600
    topology_update_interval: 60

  network:
    max_concurrent_connections: 10000
    connection_timeout: 30
    heartbeat_interval: 15
    message_timeout: 10

  rate_limiting:
    requests_per_minute: 1000
    burst_capacity: 100
    cleanup_interval: 300

  validation:
    require_signature: true
    minimum_stake: 1000
    geographic_validation: true
    capability_verification: true

  storage:
    peer_database_path: "/var/lib/nym/bootstrap/peers.db"
    topology_cache_path: "/var/lib/nym/bootstrap/topology.cache"
    backup_interval: 3600

  metrics:
    enable_prometheus: true
    prometheus_bind_address: "0.0.0.0:9091"
    collection_interval: 30

  4. Bootstrap Service Configuration

  Create /etc/systemd/system/nym-bootstrap.service:
  [Unit]
  Description=Nym Bootstrap Node
  After=network.target
  Wants=network.target

  [Service]
  Type=simple
  User=nym
  Group=nym
  ExecStart=/usr/local/bin/nym-mixnode-rs start --config /etc/nym/bootstrap/config.yaml
  Restart=always
  RestartSec=5
  LimitNOFILE=65536

  # Security settings
  NoNewPrivileges=true
  ProtectSystem=strict
  ProtectHome=true
  ReadWritePaths=/var/lib/nym/bootstrap /var/log/nym
  PrivateTmp=true

  [Install]
  WantedBy=multi-user.target

  5. Bootstrap Node Startup
  # Create nym user
  sudo useradd -r -s /bin/false -d /var/lib/nym nym
  sudo mkdir -p /var/lib/nym/bootstrap /var/log/nym
  sudo chown -R nym:nym /var/lib/nym /var/log/nym /etc/nym

  # Enable and start service
  sudo systemctl daemon-reload
  sudo systemctl enable nym-bootstrap
  sudo systemctl start nym-bootstrap

  # Verify bootstrap node status
  sudo systemctl status nym-bootstrap
  curl http://localhost:9090/status

  Docker Bootstrap Deployment

  Bootstrap Docker Configuration

  Create docker-compose-bootstrap.yml:
  version: '3.8'

  services:
    nym-bootstrap:
      build:
        context: .
        dockerfile: Dockerfile
      container_name: nym-bootstrap-primary
      hostname: bootstrap-node
      restart: unless-stopped

      ports:
        - "8080:8080/udp"
        - "9090:9090/tcp"
        - "9091:9091/tcp"  # Prometheus metrics

      environment:
        - NODE_MODE=bootstrap
        - RUST_LOG=info
        - BOOTSTRAP_AUTHORITY_LEVEL=primary
        - MAX_REGISTERED_PEERS=50000
        - CLEANUP_INTERVAL=3600
        - RATE_LIMIT_PER_MINUTE=1000

      volumes:
        - bootstrap-data:/var/lib/nym/bootstrap
        - bootstrap-logs:/var/log/nym
        - ./config/bootstrap-config.yaml:/etc/nym/bootstrap/config.yaml:ro

      networks:
        nym-network:
          ipv4_address: 172.20.0.10

      healthcheck:
        test: ["CMD", "curl", "-f", "http://localhost:9090/health"]
        interval: 30s
        timeout: 10s
        retries: 3
        start_period: 30s

    prometheus:
      image: prom/prometheus:latest
      container_name: nym-prometheus
      restart: unless-stopped
      ports:
        - "9092:9090"
      volumes:
        - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro
        - prometheus-data:/prometheus
      command:
        - '--config.file=/etc/prometheus/prometheus.yml'
        - '--storage.tsdb.path=/prometheus'
        - '--web.console.libraries=/etc/prometheus/console_libraries'
        - '--web.console.templates=/etc/prometheus/consoles'
      networks:
        nym-network:
          ipv4_address: 172.20.0.20

  volumes:
    bootstrap-data:
      driver: local
    bootstrap-logs:
      driver: local
    prometheus-data:
      driver: local

  networks:
    nym-network:
      driver: bridge
      ipam:
        driver: default
        config:
          - subnet: 172.20.0.0/16
            gateway: 172.20.0.1

  Start Bootstrap Node:
  docker-compose -f docker-compose-bootstrap.yml up -d
  docker logs nym-bootstrap-primary

  Regular Mixnode Setup

  Node Configuration

  1. Generate Node Identity
  # Create node configuration directory
  sudo mkdir -p /etc/nym/mixnode
  sudo chown $(whoami):$(whoami) /etc/nym/mixnode

  # Generate node keys
  nym-mixnode-rs keys generate \
    --output-dir /etc/nym/mixnode \
    --key-type ed25519

  2. Mixnode Configuration File

  Create /etc/nym/mixnode/config.yaml:
  # Mixnode Configuration
  node:
    mode: mixnode
    node_id_file: "/etc/nym/mixnode/node.key"
    listen_address: "0.0.0.0:8080"
    http_api_address: "0.0.0.0:9090"
    log_level: "info"
    region: "NorthAmerica"  # Set appropriate region
    stake_amount: 10000

  # P2P networking configuration
  p2p:
    bootstrap_peers:
      - "bootstrap.nymtech.net:8080"
      - "your-bootstrap-node.com:8080"  # Add your bootstrap nodes
    max_outbound_connections: 50
    max_inbound_connections: 100
    connection_timeout: 30
    heartbeat_interval: 15
    peer_exchange_interval: 120

  # VRF configuration for node selection
  vrf:
    signing_key_file: "/etc/nym/mixnode/vrf.key"
    selection_cache_size: 1000
    path_length: 3

  # Discovery settings
  discovery:
    discovery_interval: 30
    peer_exchange_interval: 120
    topology_refresh_interval: 300
    max_discovery_peers: 100
    enable_gossip: true
    gossip_fanout: 3

  # Sphinx packet processing
  sphinx:
    max_packet_rate: 30000
    worker_threads: 4
    memory_pool_size: 1000000
    enable_simd: true
    cover_traffic_ratio: 0.1

  # Load balancing
  load_balancer:
    strategy: "weighted_round_robin"
    health_check_interval: 30
    circuit_breaker_threshold: 0.5
    circuit_breaker_timeout: 60

  # Rate limiting
  rate_limiting:
    packets_per_second: 1000
    burst_capacity: 5000
    violation_threshold: 10
    ban_duration: 300

  # Security settings
  security:
    enable_threat_detection: true
    ddos_protection: true
    intrusion_detection: true
    audit_logging: true

  # Metrics and monitoring
  metrics:
    enable_prometheus: true
    prometheus_bind_address: "0.0.0.0:9091"
    collection_interval: 30
    telemetry_endpoint: "https://telemetry.nymtech.net"

  # Storage configuration
  storage:
    database_path: "/var/lib/nym/mixnode/node.db"
    backup_interval: 3600
    log_retention_days: 7

  3. Mixnode Service Configuration

  Create /etc/systemd/system/nym-mixnode.service:
  [Unit]
  Description=Nym Mixnode
  After=network.target
  Wants=network.target

  [Service]
  Type=simple
  User=nym
  Group=nym
  ExecStart=/usr/local/bin/nym-mixnode-rs start --config /etc/nym/mixnode/config.yaml
  Restart=always
  RestartSec=5
  LimitNOFILE=65536

  # Performance settings
  CPUWeight=100
  MemoryHigh=2G
  MemoryMax=4G

  # Security settings
  NoNewPrivileges=true
  ProtectSystem=strict
  ProtectHome=true
  ReadWritePaths=/var/lib/nym/mixnode /var/log/nym
  PrivateTmp=true

  [Install]
  WantedBy=multi-user.target

  4. Start Mixnode
  # Create directories and set permissions
  sudo mkdir -p /var/lib/nym/mixnode
  sudo chown -R nym:nym /var/lib/nym /etc/nym

  # Enable and start service
  sudo systemctl daemon-reload
  sudo systemctl enable nym-mixnode
  sudo systemctl start nym-mixnode

  # Verify node status
  sudo systemctl status nym-mixnode
  curl http://localhost:9090/status

  Docker Mixnode Deployment

  Mixnode Docker Configuration

  Create docker-compose-mixnode.yml:
  version: '3.8'

  services:
    nym-mixnode:
      build:
        context: .
        dockerfile: Dockerfile
      container_name: nym-mixnode-1
      hostname: mixnode-1
      restart: unless-stopped

      ports:
        - "8080:8080/udp"
        - "9090:9090/tcp"
        - "9091:9091/tcp"

      environment:
        - NODE_MODE=mixnode
        - RUST_LOG=info
        - NODE_REGION=NorthAmerica
        - STAKE_AMOUNT=10000
        - BOOTSTRAP_PEERS=bootstrap.nymtech.net:8080,your-bootstrap.com:8080
        - MAX_PACKET_RATE=30000
        - WORKER_THREADS=4
        - COVER_TRAFFIC_RATIO=0.1

      volumes:
        - mixnode-data:/var/lib/nym/mixnode
        - mixnode-logs:/var/log/nym
        - ./config/mixnode-config.yaml:/etc/nym/mixnode/config.yaml:ro

      networks:
        nym-network:
          ipv4_address: 172.20.0.11

      healthcheck:
        test: ["CMD", "curl", "-f", "http://localhost:9090/health"]
        interval: 30s
        timeout: 10s
        retries: 3
        start_period: 30s

  volumes:
    mixnode-data:
      driver: local
    mixnode-logs:
      driver: local

  networks:
    nym-network:
      driver: bridge
      ipam:
        driver: default
        config:
          - subnet: 172.20.0.0/16
            gateway: 172.20.0.1

  Multi-Node Network Deployment

  Local Testing Environment

  Complete Test Network Setup:
  # Start test network with multiple nodes
  docker-compose -f docker-compose.test.yml up -d

  # Monitor network formation
  ./scripts/monitor.sh connectivity

  # Check node discovery
  curl http://localhost:9090/peers | jq '.[] | {id, region, last_seen}'

  # Test packet forwarding
  docker-compose -f docker-compose.test.yml logs test-client | grep -i packet

  Production Multi-Node Setup

  Deploy Multiple Mixnodes:
  # Deploy across multiple servers
  for i in {1..5}; do
    ssh node-$i.example.com "
      docker pull your-registry/nym-mixnode:latest
      docker run -d \
        --name nym-mixnode \
        -p 8080:8080/udp \
        -p 9090:9090/tcp \
        -e NODE_REGION=Region$i \
        -e BOOTSTRAP_PEERS=bootstrap.nymtech.net:8080 \
        -e STAKE_AMOUNT=$((1000 * i)) \
        your-registry/nym-mixnode:latest
    "
  done

  Monitoring and Maintenance

  Health Monitoring

  Node Health Checks:
  #!/bin/bash
  # health-check.sh

  NODES=("node1.example.com" "node2.example.com" "bootstrap.example.com")

  for node in "${NODES[@]}"; do
    echo "Checking $node..."

    # Health endpoint check
    if curl -f -s "http://$node:9090/health" > /dev/null; then
      echo "  ✓ Health endpoint responsive"
    else
      echo "  ✗ Health endpoint failed"
    fi

    # P2P connectivity check
    if nc -u -z "$node" 8080; then
      echo "  ✓ P2P port accessible"
    else
      echo "  ✗ P2P port blocked"
    fi

    # Peer count check
    peer_count=$(curl -s "http://$node:9090/peers" | jq '. | length')
    echo "  Peers: $peer_count"

    echo ""
  done

  Metrics Collection:
  # Collect metrics from all nodes
  curl http://node1.example.com:9090/metrics | grep packets_processed_total
  curl http://bootstrap.example.com:9090/metrics | grep bootstrap_requests_total

  Log Analysis

  Centralized Logging Setup:
  # Install log aggregation
  sudo apt install rsyslog-elasticsearch

  # Configure log forwarding
  echo "*.* @@log-server.example.com:514" | sudo tee -a /etc/rsyslog.conf
  sudo systemctl restart rsyslog

  Backup and Recovery

  Node State Backup:
  #!/bin/bash
  # backup-node.sh

  NODE_NAME=${1:-nym-mixnode}
  BACKUP_DIR="/backup/$(date +%Y%m%d)"

  mkdir -p "$BACKUP_DIR"

  # Backup node keys
  cp -r /etc/nym/ "$BACKUP_DIR/config/"

  # Backup node database
  docker exec "$NODE_NAME" sqlite3 /var/lib/nym/node.db ".backup /tmp/node-backup.db"
  docker cp "$NODE_NAME:/tmp/node-backup.db" "$BACKUP_DIR/"

  # Backup metrics data
  docker exec "$NODE_NAME" tar -czf /tmp/metrics-backup.tar.gz /var/lib/nym/metrics/
  docker cp "$NODE_NAME:/tmp/metrics-backup.tar.gz" "$BACKUP_DIR/"

  echo "Backup completed: $BACKUP_DIR"

  Troubleshooting

  Common Issues

  Bootstrap Connection Problems:
  # Test UDP connectivity to bootstrap
  echo "BOOTSTRAP_REQUEST" | nc -u bootstrap.nymtech.net 8080

  # Check firewall rules
  sudo iptables -L | grep 8080
  sudo ufw status | grep 8080

  # Verify DNS resolution
  dig +short bootstrap.nymtech.net

  Peer Discovery Issues:
  # Check discovery logs
  docker logs nym-mixnode | grep -i "discovery\|bootstrap"

  # Test peer connectivity
  ./scripts/monitor.sh connections

  # Verify configuration
  curl http://localhost:9090/config | jq '.p2p.bootstrap_peers'

  Performance Problems:
  # Check packet processing rate
  curl http://localhost:9090/metrics | grep packet_processing_rate

  # Monitor CPU and memory usage
  docker stats nym-mixnode

  # Check connection pool status
  curl http://localhost:9090/connection/stats

  Log Analysis Commands

  Search for specific events:
  # Connection events
  journalctl -u nym-mixnode | grep -i "connection\|peer"

  # VRF selection events
  docker logs nym-mixnode | grep -i "vrf\|selection"

  # Security events
  docker logs nym-mixnode | grep -i "security\|threat\|attack"

  This deployment guide provides complete instructions for setting up both bootstrap nodes and regular mixnodes in the Nym network.
  Follow the appropriate sections based on your deployment requirements and ensure all security configurations are properly
  implemented before production deployment.