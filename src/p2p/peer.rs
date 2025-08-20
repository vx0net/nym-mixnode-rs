// Peer management and reputation system
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, IpAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use curve25519_dalek::ristretto::RistrettoPoint;

use crate::p2p::transport::PeerId;
// use crate::crypto::ristretto::RistrettoIdentityPoint;

/// Comprehensive peer information and management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub address: SocketAddr,
    pub public_key: RistrettoPoint,
    pub stake: u64,
    pub region: String,
    pub version: String,
    pub capabilities: PeerCapabilities,
    pub reputation: PeerReputation,
    pub connection_info: ConnectionInfo,
    pub last_seen: SystemTime,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_sphinx: bool,
    pub supports_cover_traffic: bool,
    pub max_throughput_pps: u32,
    pub supported_protocols: HashSet<String>,
    pub max_packet_size: usize,
    pub encryption_algorithms: Vec<String>,
}

impl Default for PeerCapabilities {
    fn default() -> Self {
        Self {
            supports_sphinx: true,
            supports_cover_traffic: true,
            max_throughput_pps: 25000,
            supported_protocols: ["nym-mixnet-1.0".to_string()].into_iter().collect(),
            max_packet_size: 1024,
            encryption_algorithms: vec!["ristretto".to_string(), "x25519".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReputation {
    pub reliability_score: f64,      // 0.0 - 1.0
    pub performance_score: f64,      // 0.0 - 1.0  
    pub uptime_percentage: f64,      // 0.0 - 100.0
    pub packets_processed: u64,
    pub packets_dropped: u64,
    pub average_latency_ms: f64,
    pub last_updated: SystemTime,
    pub violations: Vec<ReputationViolation>,
}

impl Default for PeerReputation {
    fn default() -> Self {
        Self {
            reliability_score: 1.0,
            performance_score: 1.0,
            uptime_percentage: 100.0,
            packets_processed: 0,
            packets_dropped: 0,
            average_latency_ms: 0.0,
            last_updated: SystemTime::now(),
            violations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationViolation {
    pub violation_type: ViolationType,
    pub timestamp: SystemTime,
    pub severity: ViolationSeverity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    PacketDrop,
    HighLatency,
    ConnectionFailure,
    ProtocolViolation,
    SecurityBreach,
    Downtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Low,
    Medium, 
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub connections_successful: u64,
    pub connections_failed: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_connection: Option<SystemTime>,
    pub average_connection_time: Duration,
    pub is_trusted: bool,
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        Self {
            connections_successful: 0,
            connections_failed: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_connection: None,
            average_connection_time: Duration::from_millis(100),
            is_trusted: false,
        }
    }
}

/// Peer registry and management system
pub struct PeerRegistry {
    peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    peer_groups: Arc<RwLock<HashMap<String, HashSet<PeerId>>>>, // Region-based groups
    trusted_peers: Arc<RwLock<HashSet<PeerId>>>,
    blocked_peers: Arc<RwLock<HashSet<PeerId>>>,
    reputation_thresholds: ReputationThresholds,
    config: PeerRegistryConfig,
    stats: Arc<RwLock<PeerRegistryStats>>,
}

#[derive(Debug, Clone)]
pub struct ReputationThresholds {
    pub min_reliability_score: f64,
    pub min_performance_score: f64,
    pub min_uptime_percentage: f64,
    pub max_violation_count: usize,
    pub trust_score_threshold: f64,
}

impl Default for ReputationThresholds {
    fn default() -> Self {
        Self {
            min_reliability_score: 0.8,
            min_performance_score: 0.75,
            min_uptime_percentage: 95.0,
            max_violation_count: 5,
            trust_score_threshold: 0.9,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRegistryConfig {
    pub max_peers: usize,
    pub reputation_decay_rate: f64,
    pub cleanup_interval: Duration,
    pub peer_timeout: Duration,
    pub enable_auto_trust: bool,
}

impl Default for PeerRegistryConfig {
    fn default() -> Self {
        Self {
            max_peers: 10000,
            reputation_decay_rate: 0.001, // Per hour
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            peer_timeout: Duration::from_secs(3600), // 1 hour
            enable_auto_trust: true,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct PeerRegistryStats {
    pub total_peers: usize,
    pub online_peers: usize,
    pub trusted_peers: usize,
    pub blocked_peers: usize,
    pub average_reputation: f64,
    pub peer_groups: HashMap<String, usize>,
}

impl PeerRegistry {
    pub fn new(config: PeerRegistryConfig) -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            peer_groups: Arc::new(RwLock::new(HashMap::new())),
            trusted_peers: Arc::new(RwLock::new(HashSet::new())),
            blocked_peers: Arc::new(RwLock::new(HashSet::new())),
            reputation_thresholds: ReputationThresholds::default(),
            config,
            stats: Arc::new(RwLock::new(PeerRegistryStats::default())),
        }
    }

    /// Register a new peer or update existing peer information
    pub async fn register_peer(&self, mut peer_info: PeerInfo) -> Result<(), String> {
        // Check if peer is blocked
        {
            let blocked = self.blocked_peers.read().await;
            if blocked.contains(&peer_info.peer_id) {
                return Err("Peer is blocked".to_string());
            }
        }

        // Validate peer information
        if peer_info.stake == 0 {
            return Err("Peer must have non-zero stake".to_string());
        }

        peer_info.last_seen = SystemTime::now();

        // Store peer info
        {
            let mut peers = self.peers.write().await;
            
            // Check max peers limit
            if peers.len() >= self.config.max_peers && !peers.contains_key(&peer_info.peer_id) {
                return Err("Maximum peer limit reached".to_string());
            }
            
            peers.insert(peer_info.peer_id.clone(), peer_info.clone());
        }

        // Update region groups
        {
            let mut groups = self.peer_groups.write().await;
            let region_peers = groups.entry(peer_info.region.clone()).or_insert_with(HashSet::new);
            region_peers.insert(peer_info.peer_id.clone());
        }

        // Auto-trust if conditions are met
        if self.config.enable_auto_trust {
            self.evaluate_trust(&peer_info.peer_id).await;
        }

        self.update_stats().await;
        Ok(())
    }

    /// Get peer information
    pub async fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }

    /// Get all peers in a specific region
    pub async fn get_peers_by_region(&self, region: &str) -> Vec<PeerInfo> {
        let groups = self.peer_groups.read().await;
        let peers = self.peers.read().await;
        
        if let Some(peer_ids) = groups.get(region) {
            peer_ids.iter()
                .filter_map(|id| peers.get(id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get trusted peers only
    pub async fn get_trusted_peers(&self) -> Vec<PeerInfo> {
        let trusted = self.trusted_peers.read().await;
        let peers = self.peers.read().await;
        
        trusted.iter()
            .filter_map(|id| peers.get(id))
            .cloned()
            .collect()
    }

    /// Get high-reputation peers for routing
    pub async fn get_routing_peers(&self, min_stake: u64) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        
        peers.values()
            .filter(|peer| {
                peer.is_online &&
                peer.stake >= min_stake &&
                peer.reputation.reliability_score >= self.reputation_thresholds.min_reliability_score &&
                peer.reputation.performance_score >= self.reputation_thresholds.min_performance_score &&
                peer.reputation.uptime_percentage >= self.reputation_thresholds.min_uptime_percentage &&
                peer.reputation.violations.len() <= self.reputation_thresholds.max_violation_count
            })
            .cloned()
            .collect()
    }

    /// Update peer reputation based on performance
    pub async fn update_reputation(&self, peer_id: &PeerId, update: ReputationUpdate) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            let reputation = &mut peer.reputation;
            
            match update {
                ReputationUpdate::PacketProcessed(latency_ms) => {
                    reputation.packets_processed += 1;
                    
                    // Update average latency with exponential moving average
                    let alpha = 0.1;
                    reputation.average_latency_ms = 
                        alpha * latency_ms + (1.0 - alpha) * reputation.average_latency_ms;
                    
                    // Improve performance score for low latency
                    if latency_ms < 50.0 {
                        reputation.performance_score = 
                            (reputation.performance_score * 0.99 + 0.01).min(1.0);
                    }
                }
                ReputationUpdate::PacketDropped => {
                    reputation.packets_dropped += 1;
                    
                    // Decrease reliability score
                    reputation.reliability_score = 
                        (reputation.reliability_score * 0.99).max(0.0);
                }
                ReputationUpdate::ConnectionSuccessful(duration) => {
                    peer.connection_info.connections_successful += 1;
                    peer.connection_info.last_connection = Some(SystemTime::now());
                    
                    // Update average connection time
                    let new_time = duration.as_millis() as f64;
                    let old_avg = peer.connection_info.average_connection_time.as_millis() as f64;
                    let new_avg = (old_avg * 0.9 + new_time * 0.1);
                    peer.connection_info.average_connection_time = 
                        Duration::from_millis(new_avg as u64);
                    
                    // Improve reliability score
                    reputation.reliability_score = 
                        (reputation.reliability_score * 0.99 + 0.01).min(1.0);
                }
                ReputationUpdate::ConnectionFailed => {
                    peer.connection_info.connections_failed += 1;
                    
                    // Decrease reliability score
                    reputation.reliability_score = 
                        (reputation.reliability_score * 0.98).max(0.0);
                }
                ReputationUpdate::Violation(violation) => {
                    reputation.violations.push(violation.clone());
                    
                    // Decrease scores based on violation severity
                    let penalty = match violation.severity {
                        ViolationSeverity::Low => 0.01,
                        ViolationSeverity::Medium => 0.05,
                        ViolationSeverity::High => 0.1,
                        ViolationSeverity::Critical => 0.25,
                    };
                    
                    reputation.reliability_score = 
                        (reputation.reliability_score - penalty).max(0.0);
                    reputation.performance_score = 
                        (reputation.performance_score - penalty * 0.5).max(0.0);
                }
                ReputationUpdate::UptimeReport(is_online, uptime_percentage) => {
                    peer.is_online = is_online;
                    reputation.uptime_percentage = uptime_percentage;
                    
                    // Adjust reliability based on uptime
                    if uptime_percentage > 99.0 {
                        reputation.reliability_score = 
                            (reputation.reliability_score * 0.99 + 0.01).min(1.0);
                    } else if uptime_percentage < 90.0 {
                        reputation.reliability_score = 
                            (reputation.reliability_score * 0.95).max(0.0);
                    }
                }
            }
            
            reputation.last_updated = SystemTime::now();
            peer.last_seen = SystemTime::now();
        }

        // Re-evaluate trust status
        self.evaluate_trust(peer_id).await;
    }

    /// Block a peer
    pub async fn block_peer(&self, peer_id: &PeerId, reason: String) {
        {
            let mut blocked = self.blocked_peers.write().await;
            blocked.insert(peer_id.clone());
        }
        
        // Remove from trusted peers
        {
            let mut trusted = self.trusted_peers.write().await;
            trusted.remove(peer_id);
        }
        
        // Add violation record
        if let Some(mut peer) = self.get_peer(peer_id).await {
            let violation = ReputationViolation {
                violation_type: ViolationType::SecurityBreach,
                timestamp: SystemTime::now(),
                severity: ViolationSeverity::Critical,
                description: reason,
            };
            
            self.update_reputation(peer_id, ReputationUpdate::Violation(violation)).await;
        }

        self.update_stats().await;
    }

    /// Unblock a peer (requires manual intervention)
    pub async fn unblock_peer(&self, peer_id: &PeerId) -> Result<(), String> {
        let mut blocked = self.blocked_peers.write().await;
        if blocked.remove(peer_id) {
            self.update_stats().await;
            Ok(())
        } else {
            Err("Peer was not blocked".to_string())
        }
    }

    /// Check if peer is trusted
    pub async fn is_trusted(&self, peer_id: &PeerId) -> bool {
        let trusted = self.trusted_peers.read().await;
        trusted.contains(peer_id)
    }

    /// Check if peer is blocked
    pub async fn is_blocked(&self, peer_id: &PeerId) -> bool {
        let blocked = self.blocked_peers.read().await;
        blocked.contains(peer_id)
    }

    /// Evaluate whether a peer should be trusted
    async fn evaluate_trust(&self, peer_id: &PeerId) {
        if let Some(peer) = self.get_peer(peer_id).await {
            let reputation = &peer.reputation;
            
            let trust_score = (
                reputation.reliability_score * 0.4 +
                reputation.performance_score * 0.3 +
                (reputation.uptime_percentage / 100.0) * 0.3
            ) * (1.0 - (reputation.violations.len() as f64 * 0.1).min(0.5));

            let mut trusted = self.trusted_peers.write().await;
            
            if trust_score >= self.reputation_thresholds.trust_score_threshold {
                trusted.insert(peer_id.clone());
                
                // Update connection info
                let mut peers = self.peers.write().await;
                if let Some(peer_info) = peers.get_mut(peer_id) {
                    peer_info.connection_info.is_trusted = true;
                }
            } else {
                trusted.remove(peer_id);
                
                // Update connection info
                let mut peers = self.peers.write().await;
                if let Some(peer_info) = peers.get_mut(peer_id) {
                    peer_info.connection_info.is_trusted = false;
                }
            }
        }
    }

    /// Cleanup stale peers and decay reputation scores
    pub async fn cleanup_and_decay(&self) {
        let now = SystemTime::now();
        let mut peers_to_remove = Vec::new();

        // Decay reputation scores and identify stale peers
        {
            let mut peers = self.peers.write().await;
            for (peer_id, peer) in peers.iter_mut() {
                // Check if peer is stale
                if let Ok(duration) = now.duration_since(peer.last_seen) {
                    if duration > self.config.peer_timeout {
                        peers_to_remove.push(peer_id.clone());
                        continue;
                    }
                }

                // Decay reputation scores over time
                let hours_elapsed = peer.reputation.last_updated
                    .elapsed()
                    .unwrap_or_default()
                    .as_secs() as f64 / 3600.0;
                
                let decay_factor = self.config.reputation_decay_rate * hours_elapsed;
                
                peer.reputation.reliability_score = 
                    (peer.reputation.reliability_score - decay_factor).max(0.0);
                peer.reputation.performance_score = 
                    (peer.reputation.performance_score - decay_factor).max(0.0);
            }
        }

        // Remove stale peers
        for peer_id in peers_to_remove {
            self.remove_peer(&peer_id).await;
        }

        self.update_stats().await;
    }

    /// Remove a peer from the registry
    async fn remove_peer(&self, peer_id: &PeerId) {
        // Remove from main registry
        let peer_info = {
            let mut peers = self.peers.write().await;
            peers.remove(peer_id)
        };

        // Remove from region groups
        if let Some(peer) = peer_info {
            let mut groups = self.peer_groups.write().await;
            if let Some(region_peers) = groups.get_mut(&peer.region) {
                region_peers.remove(peer_id);
                if region_peers.is_empty() {
                    groups.remove(&peer.region);
                }
            }
        }

        // Remove from trusted and blocked sets
        {
            let mut trusted = self.trusted_peers.write().await;
            trusted.remove(peer_id);
        }
        {
            let mut blocked = self.blocked_peers.write().await;
            blocked.remove(peer_id);
        }
    }

    /// Update internal statistics
    async fn update_stats(&self) {
        let peers = self.peers.read().await;
        let trusted = self.trusted_peers.read().await;
        let blocked = self.blocked_peers.read().await;
        let groups = self.peer_groups.read().await;

        let mut stats = self.stats.write().await;
        stats.total_peers = peers.len();
        stats.online_peers = peers.values().filter(|p| p.is_online).count();
        stats.trusted_peers = trusted.len();
        stats.blocked_peers = blocked.len();
        
        // Calculate average reputation
        if !peers.is_empty() {
            let total_reputation: f64 = peers.values()
                .map(|p| (p.reputation.reliability_score + p.reputation.performance_score) / 2.0)
                .sum();
            stats.average_reputation = total_reputation / peers.len() as f64;
        }
        
        // Update peer groups stats
        stats.peer_groups = groups.iter()
            .map(|(region, peers)| (region.clone(), peers.len()))
            .collect();
    }

    /// Get registry statistics
    pub async fn get_stats(&self) -> PeerRegistryStats {
        self.stats.read().await.clone()
    }

    /// Start periodic cleanup and maintenance
    pub async fn start_maintenance(&self) {
        let registry = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(registry.config.cleanup_interval);
            
            loop {
                interval.tick().await;
                registry.cleanup_and_decay().await;
            }
        });
    }
}

impl Clone for PeerRegistry {
    fn clone(&self) -> Self {
        Self {
            peers: self.peers.clone(),
            peer_groups: self.peer_groups.clone(),
            trusted_peers: self.trusted_peers.clone(),
            blocked_peers: self.blocked_peers.clone(),
            reputation_thresholds: self.reputation_thresholds.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReputationUpdate {
    PacketProcessed(f64), // latency in ms
    PacketDropped,
    ConnectionSuccessful(Duration),
    ConnectionFailed,
    Violation(ReputationViolation),
    UptimeReport(bool, f64), // is_online, uptime_percentage
}