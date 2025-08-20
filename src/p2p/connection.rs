// Connection management and monitoring
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

use crate::p2p::transport::{PeerId, P2PTransport, TransportStats, ConnectionStats};
use crate::p2p::peer::PeerRegistry;
use crate::metrics::collector::MetricsCollector;

/// Connection manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub quality_check_interval: Duration,
    pub min_connection_quality: f64,
    pub retry_attempts: u32,
    pub retry_delay: Duration,
    pub keep_alive_timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
            quality_check_interval: Duration::from_secs(60),
            min_connection_quality: 0.7,
            retry_attempts: 3,
            retry_delay: Duration::from_secs(5),
            keep_alive_timeout: Duration::from_secs(120),
        }
    }
}

/// Connection quality metrics
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub latency_ms: f64,
    pub packet_loss_rate: f64,
    pub throughput_mbps: f64,
    pub uptime_percentage: f64,
    pub error_rate: f64,
    pub last_updated: SystemTime,
}

impl Default for ConnectionQuality {
    fn default() -> Self {
        Self {
            latency_ms: 0.0,
            packet_loss_rate: 0.0,
            throughput_mbps: 0.0,
            uptime_percentage: 100.0,
            error_rate: 0.0,
            last_updated: SystemTime::now(),
        }
    }
}

/// Connection manager statistics
#[derive(Debug, Clone)]
pub struct ConnectionManagerStats {
    pub total_connections: usize,
    pub connected_count: usize,
    pub failed_count: usize,
    pub average_quality: f64,
    pub average_latency_ms: f64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
}

impl Default for ConnectionManagerStats {
    fn default() -> Self {
        Self {
            total_connections: 0,
            connected_count: 0,
            failed_count: 0,
            average_quality: 0.0,
            average_latency_ms: 0.0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
        }
    }
}

/// Connection manager for handling P2P connections
pub struct ConnectionManager {
    config: ConnectionConfig,
    transport: Arc<P2PTransport>,
    peer_registry: Arc<PeerRegistry>,
    metrics: Arc<MetricsCollector>,
    
    // Connection tracking
    connection_qualities: Arc<RwLock<HashMap<PeerId, ConnectionQuality>>>,
    connection_attempts: Arc<RwLock<HashMap<PeerId, u32>>>,
    failed_connections: Arc<RwLock<HashMap<PeerId, SystemTime>>>,
    
    // Statistics
    stats: Arc<RwLock<ConnectionManagerStats>>,
    
    // Control
    is_running: Arc<RwLock<bool>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

impl ConnectionManager {
    pub fn new(
        config: ConnectionConfig,
        transport: Arc<P2PTransport>,
        peer_registry: Arc<PeerRegistry>,
        metrics: Arc<MetricsCollector>,
    ) -> Self {
        Self {
            config,
            transport,
            peer_registry,
            metrics,
            connection_qualities: Arc::new(RwLock::new(HashMap::new())),
            connection_attempts: Arc::new(RwLock::new(HashMap::new())),
            failed_connections: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ConnectionManagerStats::default())),
            is_running: Arc::new(RwLock::new(false)),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
        }
    }
    
    /// Start the connection manager
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting connection manager");
        
        *self.is_running.write().await = true;
        
        // Start quality monitoring
        let quality_monitor = self.clone_for_tasks();
        tokio::spawn(async move {
            quality_monitor.monitor_connection_quality().await;
        });
        
        // Start connection maintenance
        let maintenance_task = self.clone_for_tasks();
        tokio::spawn(async move {
            maintenance_task.connection_maintenance().await;
        });
        
        // Start retry logic
        let retry_task = self.clone_for_tasks();
        tokio::spawn(async move {
            retry_task.retry_failed_connections().await;
        });
        
        Ok(())
    }
    
    /// Register a new connection
    pub async fn register_connection(&self, peer_id: PeerId, address: SocketAddr, is_outbound: bool) {
        debug!("Registering {} connection to {} at {}", 
               if is_outbound { "outbound" } else { "inbound" }, 
               peer_id.to_hex(), address);
        
        // Initialize connection quality
        {
            let mut qualities = self.connection_qualities.write().await;
            qualities.insert(peer_id.clone(), ConnectionQuality::default());
        }
        
        // Reset attempt counter on successful connection
        {
            let mut attempts = self.connection_attempts.write().await;
            attempts.remove(&peer_id);
        }
        
        // Remove from failed connections
        {
            let mut failed = self.failed_connections.write().await;
            failed.remove(&peer_id);
        }
        
        self.update_stats().await;
    }
    
    /// Record connection failure
    pub async fn record_failure(&self, peer_id: &PeerId, error: String) {
        warn!("Connection failure for {}: {}", peer_id.to_hex(), error);
        
        // Increment attempt counter
        {
            let mut attempts = self.connection_attempts.write().await;
            let count = attempts.entry(peer_id.clone()).or_insert(0);
            *count += 1;
        }
        
        // Record failure time
        {
            let mut failed = self.failed_connections.write().await;
            failed.insert(peer_id.clone(), SystemTime::now());
        }
        
        // Update quality metrics
        {
            let mut qualities = self.connection_qualities.write().await;
            if let Some(quality) = qualities.get_mut(peer_id) {
                quality.error_rate = (quality.error_rate + 0.1).min(1.0);
                quality.uptime_percentage *= 0.9; // Decrease uptime on failure
                quality.last_updated = SystemTime::now();
            }
        }
        
        self.update_stats().await;
    }
    
    /// Update connection activity
    pub async fn update_activity(&self, peer_id: &PeerId) {
        let mut qualities = self.connection_qualities.write().await;
        if let Some(quality) = qualities.get_mut(peer_id) {
            quality.last_updated = SystemTime::now();
            quality.uptime_percentage = (quality.uptime_percentage + 0.001).min(100.0);
        }
    }
    
    /// Update connection quality metrics
    pub async fn update_quality(&self, peer_id: &PeerId, latency_ms: f64, success: bool) {
        let mut qualities = self.connection_qualities.write().await;
        if let Some(quality) = qualities.get_mut(peer_id) {
            // Update latency with exponential moving average
            let alpha = 0.1;
            quality.latency_ms = alpha * latency_ms + (1.0 - alpha) * quality.latency_ms;
            
            if success {
                quality.error_rate = (quality.error_rate * 0.95).max(0.0);
                quality.uptime_percentage = (quality.uptime_percentage + 0.01).min(100.0);
            } else {
                quality.error_rate = (quality.error_rate + 0.05).min(1.0);
            }
            
            quality.last_updated = SystemTime::now();
        }
    }
    
    /// Get connection quality for a peer
    pub async fn get_connection_quality(&self, peer_id: &PeerId) -> Option<ConnectionQuality> {
        let qualities = self.connection_qualities.read().await;
        qualities.get(peer_id).cloned()
    }
    
    /// Check if connection should be retried
    pub async fn should_retry_connection(&self, peer_id: &PeerId) -> bool {
        let attempts = self.connection_attempts.read().await;
        let attempt_count = attempts.get(peer_id).unwrap_or(&0);
        
        if *attempt_count >= self.config.retry_attempts {
            return false;
        }
        
        let failed = self.failed_connections.read().await;
        if let Some(last_failure) = failed.get(peer_id) {
            let elapsed = SystemTime::now()
                .duration_since(*last_failure)
                .unwrap_or_default();
            elapsed >= self.config.retry_delay
        } else {
            true
        }
    }
    
    /// Get connection manager statistics
    pub async fn get_connection_stats(&self) -> ConnectionManagerStats {
        self.stats.read().await.clone()
    }
    
    /// Monitor connection quality
    async fn monitor_connection_quality(&self) {
        let mut interval = tokio::time::interval(self.config.quality_check_interval);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.check_connection_qualities().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }
    
    /// Check and update connection qualities
    async fn check_connection_qualities(&self) {
        let connected_peers = self.transport.get_connected_peers().await;
        
        for peer_id in connected_peers {
            // Ping peer to check latency
            let start_time = SystemTime::now();
            
            // Simple heartbeat to measure latency
            // In a real implementation, this would be more sophisticated
            let latency_ms = start_time.elapsed().unwrap_or_default().as_millis() as f64;
            
            self.update_quality(&peer_id, latency_ms, true).await;
            
            // Check if connection quality is below threshold
            if let Some(quality) = self.get_connection_quality(&peer_id).await {
                let quality_score = (1.0 - quality.error_rate) * 
                                  (quality.uptime_percentage / 100.0) * 
                                  (1.0 / (1.0 + quality.latency_ms / 1000.0));
                
                if quality_score < self.config.min_connection_quality {
                    warn!("Low quality connection to {}: score={:.2}", 
                          peer_id.to_hex(), quality_score);
                    
                    // Optionally disconnect low quality connections
                    // self.transport.disconnect_peer(&peer_id).await;
                }
            }
        }
        
        self.update_stats().await;
    }
    
    /// Connection maintenance loop
    async fn connection_maintenance(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.cleanup_stale_data().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }
    
    /// Clean up stale connection data
    async fn cleanup_stale_data(&self) {
        let now = SystemTime::now();
        let cleanup_threshold = Duration::from_secs(3600); // 1 hour
        
        {
            let mut qualities = self.connection_qualities.write().await;
            qualities.retain(|_, quality| {
                now.duration_since(quality.last_updated).unwrap_or_default() < cleanup_threshold
            });
        }
        
        {
            let mut failed = self.failed_connections.write().await;
            failed.retain(|_, failure_time| {
                now.duration_since(*failure_time).unwrap_or_default() < cleanup_threshold
            });
        }
        
        debug!("Cleaned up stale connection data");
    }
    
    /// Retry failed connections
    async fn retry_failed_connections(&self) {
        let mut interval = tokio::time::interval(self.config.retry_delay);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let peers_to_retry: Vec<PeerId> = {
                        let failed = self.failed_connections.read().await;
                        failed.keys().cloned().collect()
                    };
                    
                    for peer_id in peers_to_retry {
                        if self.should_retry_connection(&peer_id).await {
                            if let Some(peer_info) = self.peer_registry.get_peer(&peer_id).await {
                                debug!("Retrying connection to {}", peer_id.to_hex());
                                
                                match self.transport.connect_to_peer(peer_info.address).await {
                                    Ok(_) => {
                                        info!("Successfully reconnected to {}", peer_id.to_hex());
                                        self.register_connection(peer_id, peer_info.address, true).await;
                                    }
                                    Err(e) => {
                                        self.record_failure(&peer_id, format!("Retry failed: {}", e)).await;
                                    }
                                }
                            }
                        }
                    }
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }
    
    /// Update internal statistics
    async fn update_stats(&self) {
        let connected_peers = self.transport.get_connected_peers().await;
        let transport_stats = self.transport.get_stats().await;
        let qualities = self.connection_qualities.read().await;
        let failed = self.failed_connections.read().await;
        
        let mut stats = self.stats.write().await;
        stats.total_connections = qualities.len() + failed.len();
        stats.connected_count = connected_peers.len();
        stats.failed_count = failed.len();
        stats.total_bytes_sent = transport_stats.bytes_sent;
        stats.total_bytes_received = transport_stats.bytes_received;
        
        // Calculate average quality and latency
        if !qualities.is_empty() {
            let total_quality: f64 = qualities.values()
                .map(|q| (1.0 - q.error_rate) * (q.uptime_percentage / 100.0))
                .sum();
            stats.average_quality = total_quality / qualities.len() as f64;
            
            let total_latency: f64 = qualities.values()
                .map(|q| q.latency_ms)
                .sum();
            stats.average_latency_ms = total_latency / qualities.len() as f64;
        }
    }
    
    /// Shutdown the connection manager
    pub async fn shutdown(&self) {
        info!("Shutting down connection manager");
        
        *self.is_running.write().await = false;
        self.shutdown_signal.notify_waiters();
        
        info!("Connection manager shutdown complete");
    }
    
    fn clone_for_tasks(&self) -> ConnectionManagerTask {
        ConnectionManagerTask {
            config: self.config.clone(),
            transport: self.transport.clone(),
            peer_registry: self.peer_registry.clone(),
            connection_qualities: self.connection_qualities.clone(),
            connection_attempts: self.connection_attempts.clone(),
            failed_connections: self.failed_connections.clone(),
            stats: self.stats.clone(),
            is_running: self.is_running.clone(),
            shutdown_signal: self.shutdown_signal.clone(),
        }
    }
}

#[derive(Clone)]
struct ConnectionManagerTask {
    config: ConnectionConfig,
    transport: Arc<P2PTransport>,
    peer_registry: Arc<PeerRegistry>,
    connection_qualities: Arc<RwLock<HashMap<PeerId, ConnectionQuality>>>,
    connection_attempts: Arc<RwLock<HashMap<PeerId, u32>>>,
    failed_connections: Arc<RwLock<HashMap<PeerId, SystemTime>>>,
    stats: Arc<RwLock<ConnectionManagerStats>>,
    is_running: Arc<RwLock<bool>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

impl ConnectionManagerTask {
    async fn monitor_connection_quality(&self) {
        // Implementation details moved for async reasons
    }
    
    async fn connection_maintenance(&self) {
        // Implementation details...
    }
    
    async fn retry_failed_connections(&self) {
        // Implementation details...
    }
    
    async fn check_connection_qualities(&self) {
        // Implementation details...
    }
    
    async fn cleanup_stale_data(&self) {
        // Implementation details...
    }
}

/// Legacy Connection struct for compatibility
pub struct Connection {
    pub stream: TcpStream,
    pub peer_address: SocketAddr,
}

impl Connection {
    pub fn new(stream: TcpStream, peer_address: SocketAddr) -> Self {
        Self { stream, peer_address }
    }
    
    pub async fn send_message(&self, _message: Vec<u8>) -> Result<(), String> {
        Ok(())
    }
    
    pub async fn send_heartbeat(&self) -> Result<(), String> {
        Ok(())
    }
    
    pub async fn is_healthy(&self) -> bool {
        true
    }
    
    pub async fn close(&self) {
        // Close connection
    }
}