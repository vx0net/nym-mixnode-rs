// Transport layer for P2P connections
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex, RwLock};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

use super::{P2PConfig, P2PEvent};
use crate::metrics::collector::MetricsCollector;

/// Peer identifier type
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
    
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    
    pub fn from_hex(hex_str: &str) -> Result<Self, String> {
        if hex_str.len() != 64 {
            return Err("Invalid hex string length".to_string());
        }
        
        let bytes = hex::decode(hex_str)
            .map_err(|e| format!("Invalid hex: {}", e))?;
        
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub listen_address: SocketAddr,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub buffer_size: usize,
    pub enable_keepalive: bool,
    pub keepalive_interval: Duration,
    pub max_message_size: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0:1789".parse().unwrap(),
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
            buffer_size: 8192,
            enable_keepalive: true,
            keepalive_interval: Duration::from_secs(60),
            max_message_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Transport events
#[derive(Debug, Clone)]
pub enum TransportEvent {
    PeerConnected(PeerId, SocketAddr),
    PeerDisconnected(PeerId),
    MessageReceived(PeerId, Vec<u8>),
    ConnectionError(PeerId, String),
    Heartbeat(PeerId),
    TransportStarted,
    TransportStopped,
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub connected_at: SystemTime,
    pub last_activity: SystemTime,
    pub latency_ms: Option<f64>,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        let now = SystemTime::now();
        Self {
            bytes_sent: 0,
            bytes_received: 0,
            messages_sent: 0,
            messages_received: 0,
            connected_at: now,
            last_activity: now,
            latency_ms: None,
        }
    }
}

/// Transport statistics
#[derive(Debug, Clone)]
pub struct TransportStats {
    pub total_connections: u64,
    pub active_connections: usize,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connection_errors: u64,
    pub uptime: Duration,
}

/// Advanced P2P transport layer
pub struct P2PTransport {
    config: TransportConfig,
    connections: Arc<RwLock<HashMap<PeerId, P2PConnection>>>,
    listener: Arc<Mutex<Option<TcpListener>>>,
    event_sender: mpsc::UnboundedSender<TransportEvent>,
    event_receivers: Arc<Mutex<Vec<mpsc::UnboundedSender<TransportEvent>>>>,
    metrics: Arc<MetricsCollector>,
    stats: Arc<RwLock<TransportStats>>,
    is_running: Arc<RwLock<bool>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

/// Enhanced connection wrapper
pub struct P2PConnection {
    peer_id: PeerId,
    stream: Arc<Mutex<TcpStream>>,
    peer_address: SocketAddr,
    stats: Arc<RwLock<ConnectionStats>>,
    is_connected: Arc<RwLock<bool>>,
    last_heartbeat: Arc<RwLock<SystemTime>>,
}

impl P2PConnection {
    pub fn new(peer_id: PeerId, stream: TcpStream, peer_address: SocketAddr) -> Self {
        Self {
            peer_id,
            stream: Arc::new(Mutex::new(stream)),
            peer_address,
            stats: Arc::new(RwLock::new(ConnectionStats::default())),
            is_connected: Arc::new(RwLock::new(true)),
            last_heartbeat: Arc::new(RwLock::new(SystemTime::now())),
        }
    }
    
    pub async fn send_message(&self, message: &[u8]) -> Result<(), String> {
        if !*self.is_connected.read().await {
            return Err("Connection is closed".to_string());
        }
        
        let mut stream = self.stream.lock().await;
        
        // Send message length first
        let len = message.len() as u32;
        stream.write_all(&len.to_be_bytes())
            .await
            .map_err(|e| format!("Failed to send message length: {}", e))?;
        
        // Send message data
        stream.write_all(message)
            .await
            .map_err(|e| format!("Failed to send message data: {}", e))?;
        
        stream.flush()
            .await
            .map_err(|e| format!("Failed to flush stream: {}", e))?;
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.bytes_sent += message.len() as u64 + 4; // +4 for length prefix
            stats.messages_sent += 1;
            stats.last_activity = SystemTime::now();
        }
        
        debug!("Sent {} bytes to {}", message.len(), self.peer_address);
        Ok(())
    }
    
    pub async fn receive_message(&self) -> Result<Vec<u8>, String> {
        if !*self.is_connected.read().await {
            return Err("Connection is closed".to_string());
        }
        
        let mut stream = self.stream.lock().await;
        
        // Read message length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)
            .await
            .map_err(|e| format!("Failed to read message length: {}", e))?;
        
        let len = u32::from_be_bytes(len_buf) as usize;
        
        // Prevent excessive memory allocation
        if len > 10 * 1024 * 1024 { // 10MB limit
            return Err("Message too large".to_string());
        }
        
        // Read message data
        let mut message = vec![0u8; len];
        stream.read_exact(&mut message)
            .await
            .map_err(|e| format!("Failed to read message data: {}", e))?;
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.bytes_received += len as u64 + 4; // +4 for length prefix
            stats.messages_received += 1;
            stats.last_activity = SystemTime::now();
        }
        
        debug!("Received {} bytes from {}", len, self.peer_address);
        Ok(message)
    }
    
    pub async fn send_heartbeat(&self) -> Result<(), String> {
        let heartbeat = HeartbeatMessage {
            timestamp: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            peer_id: self.peer_id.clone(),
        };
        
        let message = serde_json::to_vec(&heartbeat)
            .map_err(|e| format!("Failed to serialize heartbeat: {}", e))?;
        
        self.send_message(&message).await?;
        
        *self.last_heartbeat.write().await = SystemTime::now();
        Ok(())
    }
    
    pub async fn is_healthy(&self) -> bool {
        if !*self.is_connected.read().await {
            return false;
        }
        
        let last_heartbeat = *self.last_heartbeat.read().await;
        let since_heartbeat = SystemTime::now()
            .duration_since(last_heartbeat)
            .unwrap_or_default();
        
        since_heartbeat < Duration::from_secs(120) // Consider healthy if heartbeat within 2 minutes
    }
    
    pub async fn close(&self) {
        *self.is_connected.write().await = false;
        debug!("Closed connection to {}", self.peer_address);
    }
    
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }
    
    pub fn peer_address(&self) -> SocketAddr {
        self.peer_address
    }
    
    pub async fn get_stats(&self) -> ConnectionStats {
        self.stats.read().await.clone()
    }
}

impl P2PTransport {
    pub fn new(config: TransportConfig, metrics: Arc<MetricsCollector>) -> Self {
        let (event_sender, _) = mpsc::unbounded_channel();
        
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            listener: Arc::new(Mutex::new(None)),
            event_sender,
            event_receivers: Arc::new(Mutex::new(Vec::new())),
            metrics,
            stats: Arc::new(RwLock::new(TransportStats {
                total_connections: 0,
                active_connections: 0,
                bytes_sent: 0,
                bytes_received: 0,
                connection_errors: 0,
                uptime: Duration::from_secs(0),
            })),
            is_running: Arc::new(RwLock::new(false)),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
        }
    }
    
    /// Start the transport layer
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting P2P transport on {}", self.config.listen_address);
        
        *self.is_running.write().await = true;
        
        let listener = TcpListener::bind(self.config.listen_address).await?;
        *self.listener.lock().await = Some(listener);
        
        // Start accepting connections
        let transport_task = self.clone_for_tasks();
        tokio::spawn(async move {
            transport_task.accept_connections().await;
        });
        
        // Start connection maintenance
        let maintenance_task = self.clone_for_tasks();
        tokio::spawn(async move {
            maintenance_task.connection_maintenance().await;
        });
        
        let _ = self.event_sender.send(TransportEvent::TransportStarted);
        
        info!("P2P transport started successfully");
        Ok(())
    }
    
    /// Connect to a peer
    pub async fn connect_to_peer(&self, address: SocketAddr) -> Result<PeerId, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Connecting to peer at {}", address);
        
        let stream = tokio::time::timeout(
            self.config.connection_timeout,
            TcpStream::connect(address)
        ).await??;
        
        let peer_id = PeerId::random(); // In real implementation, this would be derived from handshake
        let connection = P2PConnection::new(peer_id.clone(), stream, address);
        
        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(peer_id.clone(), connection);
            
            let mut stats = self.stats.write().await;
            stats.total_connections += 1;
            stats.active_connections = connections.len();
        }
        
        let _ = self.event_sender.send(TransportEvent::PeerConnected(peer_id.clone(), address));
        
        info!("Connected to peer {} at {}", peer_id.to_hex(), address);
        Ok(peer_id)
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: &PeerId) {
        if let Some(connection) = {
            let mut connections = self.connections.write().await;
            connections.remove(peer_id)
        } {
            connection.close().await;
            let _ = self.event_sender.send(TransportEvent::PeerDisconnected(peer_id.clone()));
            
            let mut stats = self.stats.write().await;
            stats.active_connections = stats.active_connections.saturating_sub(1);
            
            info!("Disconnected from peer {}", peer_id.to_hex());
        }
    }
    
    /// Send message to a specific peer
    pub async fn send_to_peer(&self, peer_id: &PeerId, data: &[u8]) -> Result<(), String> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(peer_id) {
            connection.send_message(data).await
        } else {
            Err(format!("No connection to peer {}", peer_id.to_hex()))
        }
    }
    
    /// Broadcast message to all connected peers
    pub async fn broadcast(&self, data: &[u8]) -> usize {
        let connections = self.connections.read().await;
        let mut sent_count = 0;
        
        for (peer_id, connection) in connections.iter() {
            if connection.send_message(data).await.is_ok() {
                sent_count += 1;
            } else {
                warn!("Failed to broadcast to peer {}", peer_id.to_hex());
            }
        }
        
        sent_count
    }
    
    /// Get connected peers
    pub async fn get_connected_peers(&self) -> Vec<PeerId> {
        let connections = self.connections.read().await;
        connections.keys().cloned().collect()
    }
    
    /// Subscribe to transport events
    pub fn subscribe_events(&self) -> mpsc::UnboundedReceiver<TransportEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        // In a real implementation, you'd maintain a list of subscribers
        // For now, just return an empty receiver
        receiver
    }
    
    /// Get transport statistics
    pub async fn get_stats(&self) -> TransportStats {
        self.stats.read().await.clone()
    }
    
    /// Shutdown the transport
    pub async fn shutdown(&self) {
        info!("Shutting down P2P transport");
        
        *self.is_running.write().await = false;
        self.shutdown_signal.notify_waiters();
        
        // Close all connections
        let connections = {
            let mut conns = self.connections.write().await;
            std::mem::take(&mut *conns)
        };
        
        for (peer_id, connection) in connections {
            connection.close().await;
            let _ = self.event_sender.send(TransportEvent::PeerDisconnected(peer_id));
        }
        
        *self.listener.lock().await = None;
        let _ = self.event_sender.send(TransportEvent::TransportStopped);
        
        info!("P2P transport shutdown complete");
    }
    
    /// Accept incoming connections
    async fn accept_connections(&self) {
        let listener = self.listener.clone();
        
        loop {
            if !*self.is_running.read().await {
                break;
            }
            
            let listener_guard = listener.lock().await;
            if let Some(ref listener) = *listener_guard {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        drop(listener_guard); // Release the lock early
                        
                        debug!("Accepted connection from {}", addr);
                        
                        let peer_id = PeerId::random(); // In real implementation, get from handshake
                        let connection = P2PConnection::new(peer_id.clone(), stream, addr);
                        
                        {
                            let mut connections = self.connections.write().await;
                            connections.insert(peer_id.clone(), connection);
                            
                            let mut stats = self.stats.write().await;
                            stats.total_connections += 1;
                            stats.active_connections = connections.len();
                        }
                        
                        let _ = self.event_sender.send(TransportEvent::PeerConnected(peer_id, addr));
                    }
                    Err(e) => {
                        drop(listener_guard);
                        error!("Failed to accept connection: {}", e);
                        
                        let mut stats = self.stats.write().await;
                        stats.connection_errors += 1;
                    }
                }
            } else {
                drop(listener_guard);
                break;
            }
        }
    }
    
    /// Connection maintenance loop
    async fn connection_maintenance(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.cleanup_unhealthy_connections().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }
    
    /// Clean up unhealthy connections
    async fn cleanup_unhealthy_connections(&self) {
        let mut unhealthy_peers = Vec::new();
        
        {
            let connections = self.connections.read().await;
            for (peer_id, connection) in connections.iter() {
                if !connection.is_healthy().await {
                    unhealthy_peers.push(peer_id.clone());
                }
            }
        }
        
        for peer_id in unhealthy_peers {
            self.disconnect_peer(&peer_id).await;
        }
    }
    
    fn clone_for_tasks(&self) -> P2PTransportTask {
        P2PTransportTask {
            config: self.config.clone(),
            connections: self.connections.clone(),
            listener: self.listener.clone(),
            event_sender: self.event_sender.clone(),
            stats: self.stats.clone(),
            is_running: self.is_running.clone(),
            shutdown_signal: self.shutdown_signal.clone(),
        }
    }
}

#[derive(Clone)]
struct P2PTransportTask {
    config: TransportConfig,
    connections: Arc<RwLock<HashMap<PeerId, P2PConnection>>>,
    listener: Arc<Mutex<Option<TcpListener>>>,
    event_sender: mpsc::UnboundedSender<TransportEvent>,
    stats: Arc<RwLock<TransportStats>>,
    is_running: Arc<RwLock<bool>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

impl P2PTransportTask {
    async fn accept_connections(&self) {
        // Implementation moved from P2PTransport for async reasons
        let listener = self.listener.clone();
        
        loop {
            if !*self.is_running.read().await {
                break;
            }
            
            let listener_guard = listener.lock().await;
            if let Some(ref listener) = *listener_guard {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        drop(listener_guard);
                        
                        debug!("Accepted connection from {}", addr);
                        
                        let peer_id = PeerId::random();
                        let connection = P2PConnection::new(peer_id.clone(), stream, addr);
                        
                        {
                            let mut connections = self.connections.write().await;
                            connections.insert(peer_id.clone(), connection);
                            
                            let mut stats = self.stats.write().await;
                            stats.total_connections += 1;
                            stats.active_connections = connections.len();
                        }
                        
                        let _ = self.event_sender.send(TransportEvent::PeerConnected(peer_id, addr));
                    }
                    Err(e) => {
                        drop(listener_guard);
                        error!("Failed to accept connection: {}", e);
                        
                        let mut stats = self.stats.write().await;
                        stats.connection_errors += 1;
                    }
                }
            } else {
                drop(listener_guard);
                break;
            }
        }
    }
    
    async fn connection_maintenance(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.cleanup_unhealthy_connections().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }
    
    async fn cleanup_unhealthy_connections(&self) {
        let mut unhealthy_peers = Vec::new();
        
        {
            let connections = self.connections.read().await;
            for (peer_id, connection) in connections.iter() {
                if !connection.is_healthy().await {
                    unhealthy_peers.push(peer_id.clone());
                }
            }
        }
        
        for peer_id in unhealthy_peers {
            if let Some(connection) = {
                let mut connections = self.connections.write().await;
                connections.remove(&peer_id)
            } {
                connection.close().await;
                let _ = self.event_sender.send(TransportEvent::PeerDisconnected(peer_id));
                
                let mut stats = self.stats.write().await;
                stats.active_connections = stats.active_connections.saturating_sub(1);
            }
        }
    }
}

/// Heartbeat message
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    pub timestamp: u64,
    pub peer_id: PeerId,
}

/// Legacy Transport layer (kept for compatibility)
pub struct Transport {
    config: P2PConfig,
    listener: Arc<Mutex<Option<TcpListener>>>,
    event_sender: mpsc::UnboundedSender<P2PEvent>,
}

impl Transport {
    pub fn new(config: P2PConfig, event_sender: mpsc::UnboundedSender<P2PEvent>) -> Self {
        Self {
            config,
            listener: Arc::new(Mutex::new(None)),
            event_sender,
        }
    }
    
    /// Start transport layer
    pub async fn start(&self) -> Result<(), String> {
        let listener = TcpListener::bind(self.config.listen_address)
            .await
            .map_err(|e| format!("Failed to bind to {}: {}", self.config.listen_address, e))?;
        
        info!("Transport listening on {}", self.config.listen_address);
        
        *self.listener.lock().await = Some(listener);
        
        // Start accepting connections
        let listener = self.listener.clone();
        let event_sender = self.event_sender.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let listener = listener.lock().await;
            if let Some(ref listener) = *listener {
                loop {
                    match listener.accept().await {
                        Ok((stream, addr)) => {
                            debug!("Accepted connection from {}", addr);
                            
                            let event_sender = event_sender.clone();
                            let config = config.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_incoming_connection(stream, addr, config, event_sender).await {
                                    warn!("Failed to handle incoming connection from {}: {}", addr, e);
                                }
                            });
                        },
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Stop transport layer
    pub async fn stop(&self) {
        *self.listener.lock().await = None;
        info!("Transport layer stopped");
    }
    
    /// Connect to remote peer
    pub async fn connect(&self, address: SocketAddr) -> Result<Connection, String> {
        let stream = TcpStream::connect(address)
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", address, e))?;
        
        debug!("Connected to {}", address);
        
        Ok(Connection::new(stream, address))
    }
    
    /// Handle incoming connection
    async fn handle_incoming_connection(
        stream: TcpStream,
        addr: SocketAddr,
        _config: P2PConfig,
        _event_sender: mpsc::UnboundedSender<P2PEvent>,
    ) -> Result<(), String> {
        let _connection = Connection::new(stream, addr);
        
        // TODO: Implement protocol handshake and message handling
        debug!("Handling connection from {}", addr);
        
        Ok(())
    }
}

/// Network connection
pub struct Connection {
    stream: Arc<Mutex<TcpStream>>,
    peer_address: SocketAddr,
    is_connected: Arc<Mutex<bool>>,
}

impl Connection {
    pub fn new(stream: TcpStream, peer_address: SocketAddr) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            peer_address,
            is_connected: Arc::new(Mutex::new(true)),
        }
    }
    
    /// Send message to peer
    pub async fn send_message(&self, message: Vec<u8>) -> Result<(), String> {
        let mut stream = self.stream.lock().await;
        
        // Send message length first
        let len = message.len() as u32;
        stream.write_all(&len.to_be_bytes())
            .await
            .map_err(|e| format!("Failed to send message length: {}", e))?;
        
        // Send message data
        stream.write_all(&message)
            .await
            .map_err(|e| format!("Failed to send message data: {}", e))?;
        
        stream.flush()
            .await
            .map_err(|e| format!("Failed to flush stream: {}", e))?;
        
        debug!("Sent {} bytes to {}", message.len(), self.peer_address);
        Ok(())
    }
    
    /// Receive message from peer
    pub async fn receive_message(&self) -> Result<Vec<u8>, String> {
        let mut stream = self.stream.lock().await;
        
        // Read message length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)
            .await
            .map_err(|e| format!("Failed to read message length: {}", e))?;
        
        let len = u32::from_be_bytes(len_buf) as usize;
        
        // Prevent excessive memory allocation
        if len > 10 * 1024 * 1024 { // 10MB limit
            return Err("Message too large".to_string());
        }
        
        // Read message data
        let mut message = vec![0u8; len];
        stream.read_exact(&mut message)
            .await
            .map_err(|e| format!("Failed to read message data: {}", e))?;
        
        debug!("Received {} bytes from {}", len, self.peer_address);
        Ok(message)
    }
    
    /// Send heartbeat
    pub async fn send_heartbeat(&self) -> Result<(), String> {
        let heartbeat = HeartbeatMessage {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            peer_id: PeerId::random(), // TODO: Use actual node ID
        };
        
        let message = serde_json::to_vec(&heartbeat)
            .map_err(|e| format!("Failed to serialize heartbeat: {}", e))?;
        
        self.send_message(message).await
    }
    
    /// Check if connection is healthy
    pub async fn is_healthy(&self) -> bool {
        *self.is_connected.lock().await
    }
    
    /// Close connection
    pub async fn close(&self) {
        *self.is_connected.lock().await = false;
        // The TcpStream will be dropped when the Arc is dropped
        debug!("Closed connection to {}", self.peer_address);
    }
    
    /// Get peer address
    pub fn peer_address(&self) -> SocketAddr {
        self.peer_address
    }
}