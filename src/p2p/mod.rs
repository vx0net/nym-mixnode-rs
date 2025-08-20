// P2P networking layer for mixnode connectivity
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

pub mod transport;
pub mod discovery;
pub mod connection;
pub mod protocol;
pub mod protocols;
pub mod peer;
pub mod network;

use crate::metrics::collector::MetricsCollector;

/// P2P network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfig {
    pub node_id: String,
    pub listen_address: SocketAddr,
    pub advertise_address: Option<SocketAddr>,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub discovery_interval: Duration,
    pub enable_upnp: bool,
    pub enable_mdns: bool,
    pub bootstrap_nodes: Vec<SocketAddr>,
    pub protocol_version: String,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            listen_address: "0.0.0.0:1789".parse().unwrap(),
            advertise_address: None,
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
            discovery_interval: Duration::from_secs(60),
            enable_upnp: false,
            enable_mdns: false,
            bootstrap_nodes: vec![
                "bootstrap1.nymtech.net:1789".parse().unwrap(),
                "bootstrap2.nymtech.net:1789".parse().unwrap(),
            ],
            protocol_version: "nym-mixnode/1.0.0".to_string(),
        }
    }
}

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerInfo {
    pub peer_id: String,
    pub addresses: Vec<SocketAddr>,
    pub protocol_version: String,
    pub last_seen: SystemTime,
    pub connection_status: ConnectionStatus,
    pub reputation: f64,
    pub capabilities: Vec<String>,
}

/// Connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
    Banned,
}

/// P2P network events
#[derive(Debug, Clone)]
pub enum P2PEvent {
    PeerConnected(String),
    PeerDisconnected(String),
    PeerDiscovered(PeerInfo),
    MessageReceived(String, Vec<u8>),
    ConnectionFailed(String, String),
    ProtocolError(String),
}

/// Main P2P network manager
pub struct P2PNetwork {
    config: P2PConfig,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    connections: Arc<RwLock<HashMap<String, transport::Connection>>>,
    transport: Arc<transport::Transport>,
    discovery: Arc<discovery::PeerDiscovery>,
    protocols: Arc<protocols::ProtocolHandler>,
    metrics: Arc<MetricsCollector>,
    event_sender: mpsc::UnboundedSender<P2PEvent>,
    is_running: Arc<RwLock<bool>>,
}

impl P2PNetwork {
    /// Create new P2P network
    pub fn new(config: P2PConfig) -> Result<Self, String> {
        let (event_sender, _event_receiver) = mpsc::unbounded_channel();
        
        let transport = Arc::new(transport::Transport::new(
            config.clone(),
            event_sender.clone(),
        ));
        
        let metrics = Arc::new(crate::metrics::collector::MetricsCollector::new(
            crate::metrics::collector::MetricsConfig::default()
        ));
        
        let peer_registry_config = peer::PeerRegistryConfig::default();
        let peer_registry = Arc::new(peer::PeerRegistry::new(peer_registry_config));
        
        let discovery_config = discovery::DiscoveryConfig {
            bootstrap_peers: vec![],
            discovery_interval: std::time::Duration::from_secs(30),
            peer_exchange_interval: std::time::Duration::from_secs(60),
            max_discovery_peers: 100,
            gossip_fanout: 3,
            enable_mdns: false,
            enable_dht: false,
            network_topology_refresh: std::time::Duration::from_secs(300),
        };
        
        let discovery = Arc::new(discovery::PeerDiscovery::new(
            discovery_config,
            Arc::new(transport::P2PTransport::new(
                transport::TransportConfig::default(), 
                metrics.clone()
            )),
            peer_registry.clone(),
            metrics.clone(),
        ));
        
        let protocols = Arc::new(protocols::ProtocolHandler::new(
            config.clone(),
            event_sender.clone(),
        ));
        
        let network = Self {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            transport,
            discovery,
            protocols,
            metrics,
            event_sender,
            is_running: Arc::new(RwLock::new(false)),
        };
        
        Ok(network)
    }
    
    /// Start P2P networking
    pub async fn start(&self) -> Result<(), String> {
        info!("Starting P2P network on {}", self.config.listen_address);
        
        *self.is_running.write().await = true;
        
        // Start transport layer
        self.transport.start().await.map_err(|e| e.to_string())?;
        
        // Start peer discovery
        self.discovery.start().await.map_err(|e| e.to_string())?;
        
        // Start protocol handler
        self.protocols.start().await.map_err(|e| e.to_string())?;
        
        // Start connection management
        self.start_connection_manager().await;
        
        // Start heartbeat
        self.start_heartbeat().await;
        
        // Bootstrap from known peers
        self.bootstrap().await?;
        
        info!("P2P network started successfully");
        Ok(())
    }
    
    /// Stop P2P networking
    pub async fn stop(&self) {
        info!("Stopping P2P network");
        
        *self.is_running.write().await = false;
        
        // Close all connections
        {
            let mut connections = self.connections.write().await;
            for (peer_id, connection) in connections.drain() {
                connection.close().await;
                info!("Closed connection to peer: {}", peer_id);
            }
        }
        
        // Stop components
        self.transport.stop().await;
        self.discovery.stop().await;
        self.protocols.stop().await;
        
        info!("P2P network stopped");
    }
    
    /// Connect to a peer
    pub async fn connect_to_peer(&self, peer_id: &str, address: SocketAddr) -> Result<(), String> {
        debug!("Connecting to peer {} at {}", peer_id, address);
        
        // Check if already connected
        if self.connections.read().await.contains_key(peer_id) {
            return Ok(());
        }
        
        // Create connection
        let connection = self.transport.connect(address).await?;
        
        // Perform protocol handshake
        self.protocols.handshake(&connection, peer_id).await?;
        
        // Store connection
        self.connections.write().await.insert(peer_id.to_string(), connection);
        
        // Update peer info
        {
            let mut peers = self.peers.write().await;
            if let Some(peer) = peers.get_mut(peer_id) {
                peer.connection_status = ConnectionStatus::Connected;
                peer.last_seen = SystemTime::now();
            } else {
                let peer_info = PeerInfo {
                    peer_id: peer_id.to_string(),
                    addresses: vec![address],
                    protocol_version: self.config.protocol_version.clone(),
                    last_seen: SystemTime::now(),
                    connection_status: ConnectionStatus::Connected,
                    reputation: 0.5, // Neutral starting reputation
                    capabilities: vec!["mixnode".to_string()],
                };
                peers.insert(peer_id.to_string(), peer_info);
            }
        }
        
        // Send event
        let _ = self.event_sender.send(P2PEvent::PeerConnected(peer_id.to_string()));
        
        info!("Connected to peer: {}", peer_id);
        Ok(())
    }
    
    /// Send message to peer
    pub async fn send_message(&self, peer_id: &str, message: Vec<u8>) -> Result<(), String> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(peer_id) {
            connection.send_message(message).await?;
            Ok(())
        } else {
            Err(format!("No connection to peer: {}", peer_id))
        }
    }
    
    /// Broadcast message to all connected peers
    pub async fn broadcast_message(&self, message: Vec<u8>) -> Result<usize, String> {
        let connections = self.connections.read().await;
        let mut sent_count = 0;
        
        for (peer_id, connection) in connections.iter() {
            if let Err(e) = connection.send_message(message.clone()).await {
                warn!("Failed to send message to peer {}: {}", peer_id, e);
            } else {
                sent_count += 1;
            }
        }
        
        Ok(sent_count)
    }
    
    /// Get connected peers
    pub async fn get_connected_peers(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }
    
    /// Get peer information
    pub async fn get_peer_info(&self, peer_id: &str) -> Option<PeerInfo> {
        self.peers.read().await.get(peer_id).cloned()
    }
    
    /// Get all known peers
    pub async fn get_all_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }
    
    /// Bootstrap from known peers
    async fn bootstrap(&self) -> Result<(), String> {
        info!("Bootstrapping from {} known nodes", self.config.bootstrap_nodes.len());
        
        for bootstrap_addr in &self.config.bootstrap_nodes {
            let peer_id = format!("bootstrap-{}", bootstrap_addr);
            
            if let Err(e) = self.connect_to_peer(&peer_id, *bootstrap_addr).await {
                warn!("Failed to connect to bootstrap node {}: {}", bootstrap_addr, e);
            }
        }
        
        Ok(())
    }
    
    /// Start connection manager
    async fn start_connection_manager(&self) {
        let peers = self.peers.clone();
        let connections = self.connections.clone();
        let event_sender = self.event_sender.clone();
        let is_running = self.is_running.clone();
        let max_connections = self.config.max_connections;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            while *is_running.read().await {
                interval.tick().await;
                
                // Check connection health
                let mut failed_connections = Vec::new();
                
                {
                    let connections_guard = connections.read().await;
                    for (peer_id, connection) in connections_guard.iter() {
                        if !connection.is_healthy().await {
                            failed_connections.push(peer_id.clone());
                        }
                    }
                }
                
                // Remove failed connections
                for peer_id in failed_connections {
                    {
                        let mut connections_guard = connections.write().await;
                        if let Some(connection) = connections_guard.remove(&peer_id) {
                            connection.close().await;
                        }
                    }
                    
                    // Update peer status
                    {
                        let mut peers_guard = peers.write().await;
                        if let Some(peer) = peers_guard.get_mut(&peer_id) {
                            peer.connection_status = ConnectionStatus::Disconnected;
                        }
                    }
                    
                    let _ = event_sender.send(P2PEvent::PeerDisconnected(peer_id));
                }
                
                // Enforce connection limits
                let connection_count = connections.read().await.len();
                if connection_count > max_connections {
                    warn!("Too many connections ({}), need to drop some", connection_count);
                    // TODO: Implement connection pruning logic
                }
            }
        });
    }
    
    /// Start heartbeat
    async fn start_heartbeat(&self) {
        let connections = self.connections.clone();
        let is_running = self.is_running.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            while *is_running.read().await {
                interval.tick().await;
                
                let connections_guard = connections.read().await;
                for (peer_id, connection) in connections_guard.iter() {
                    if let Err(e) = connection.send_heartbeat().await {
                        warn!("Failed to send heartbeat to peer {}: {}", peer_id, e);
                    }
                }
            }
        });
    }
    
    /// Forward packet to next hop
    pub async fn forward_packet(&self, packet: &[u8], next_hop: &crate::sphinx::MixNodeId) -> Result<(), String> {
        let next_hop_str = format!("{:?}", next_hop);
        
        // Find the peer corresponding to the next hop
        let peer_address = {
            let peers = self.peers.read().await;
            peers.values()
                .find(|peer| peer.peer_id == next_hop_str)
                .and_then(|peer| peer.addresses.first().copied())
        };
        
        if let Some(address) = peer_address {
            // Ensure we're connected to this peer
            if !self.connections.read().await.contains_key(&next_hop_str) {
                if let Err(e) = self.connect_to_peer(&next_hop_str, address).await {
                    return Err(format!("Failed to connect to next hop: {}", e));
                }
            }
            
            // Send the packet
            self.send_message(&next_hop_str, packet.to_vec()).await
        } else {
            Err(format!("Next hop not found in peer list: {}", next_hop_str))
        }
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> P2PStats {
        let peers = self.peers.read().await;
        let connections = self.connections.read().await;
        
        let total_peers = peers.len();
        let connected_peers = connections.len();
        let discovered_peers = peers.values()
            .filter(|p| p.connection_status != ConnectionStatus::Connected)
            .count();
        
        P2PStats {
            total_peers,
            connected_peers,
            discovered_peers,
            bootstrap_nodes: self.config.bootstrap_nodes.len(),
            protocol_version: self.config.protocol_version.clone(),
        }
    }
}

/// P2P network statistics
#[derive(Debug, Clone, Serialize)]
pub struct P2PStats {
    pub total_peers: usize,
    pub connected_peers: usize,
    pub discovered_peers: usize,
    pub bootstrap_nodes: usize,
    pub protocol_version: String,
}