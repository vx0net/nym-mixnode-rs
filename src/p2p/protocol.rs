// P2P protocol handlers and message routing
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, mpsc};
use serde::{Serialize, Deserialize};
use blake3::Hash;

use crate::p2p::transport::{P2PTransport, PeerId, TransportEvent};
use crate::p2p::peer::PeerRegistry;
use crate::sphinx::packet::SphinxPacket;
use crate::metrics::collector::MetricsCollector;

/// P2P protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub protocol_version: String,
    pub max_message_size: usize,
    pub request_timeout: Duration,
    pub max_concurrent_requests: usize,
    pub enable_message_compression: bool,
    pub enable_message_encryption: bool,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            protocol_version: "nym-mixnet-1.0".to_string(),
            max_message_size: 10 * 1024 * 1024, // 10MB
            request_timeout: Duration::from_secs(30),
            max_concurrent_requests: 1000,
            enable_message_compression: false, // Keep simple for mixnet
            enable_message_encryption: true,
        }
    }
}

/// P2P Protocol handler
pub struct P2PProtocol {
    config: ProtocolConfig,
    transport: Arc<P2PTransport>,
    peer_registry: Arc<PeerRegistry>,
    metrics: Arc<MetricsCollector>,
    
    // Protocol state
    message_handlers: Arc<RwLock<HashMap<MessageType, Box<dyn MessageHandler + Send + Sync>>>>,
    pending_requests: Arc<RwLock<HashMap<RequestId, PendingRequest>>>,
    
    // Communication channels
    protocol_events: mpsc::UnboundedSender<ProtocolEvent>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

/// Core P2P message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    // Mixnet packet forwarding
    SphinxForward {
        packet: SphinxPacket,
        next_hop: Option<PeerId>,
        path_info: PathInfo,
    },
    SphinxResponse {
        request_id: RequestId,
        success: bool,
        error: Option<String>,
    },
    
    // Network topology and routing
    TopologySync {
        nodes: Vec<NetworkNode>,
        timestamp: SystemTime,
    },
    RouteDiscovery {
        target: PeerId,
        max_hops: u8,
        path: Vec<PeerId>,
    },
    RouteResponse {
        request_id: RequestId,
        path: Vec<PeerId>,
        latency_estimate: Duration,
    },
    
    // Peer management
    PeerInfo {
        peer_data: crate::p2p::peer::PeerInfo,
        signature: Vec<u8>,
    },
    PeerQuery {
        criteria: PeerQueryCriteria,
    },
    PeerQueryResponse {
        request_id: RequestId,
        peers: Vec<crate::p2p::peer::PeerInfo>,
    },
    
    // Cover traffic and mixing
    CoverTraffic {
        dummy_packet: Vec<u8>,
        timestamp: SystemTime,
    },
    
    // Network health and monitoring
    HealthCheck {
        timestamp: SystemTime,
        metrics_snapshot: Option<Vec<u8>>, // Serialized metrics
    },
    HealthCheckResponse {
        request_id: RequestId,
        status: NodeStatus,
        metrics: Option<Vec<u8>>,
    },
    
    // Generic request/response
    Request {
        request_id: RequestId,
        method: String,
        payload: Vec<u8>,
    },
    Response {
        request_id: RequestId,
        success: bool,
        payload: Vec<u8>,
    },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestId(pub [u8; 16]);

impl RequestId {
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    pub total_hops: u8,
    pub current_hop: u8,
    pub delay_schedule: Vec<Duration>,
    pub cover_traffic_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    pub peer_id: PeerId,
    pub stake: u64,
    pub location: String,
    pub capabilities: Vec<String>,
    pub reputation_score: f64,
    pub last_seen: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerQueryCriteria {
    pub min_stake: Option<u64>,
    pub regions: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
    pub min_reputation: Option<f64>,
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
    Offline,
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub peer_id: PeerId,
    pub sent_at: SystemTime,
    pub timeout: SystemTime,
    pub response_sender: mpsc::UnboundedSender<P2PMessage>,
}

#[derive(Debug)]
pub enum ProtocolEvent {
    MessageReceived {
        peer_id: PeerId,
        message: P2PMessage,
    },
    RequestTimedOut {
        request_id: RequestId,
        peer_id: PeerId,
    },
    PeerStatusChanged {
        peer_id: PeerId,
        status: NodeStatus,
    },
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum MessageType {
    SphinxForward,
    SphinxResponse,
    TopologySync,
    RouteDiscovery,
    RouteResponse,
    PeerInfo,
    PeerQuery,
    PeerQueryResponse,
    CoverTraffic,
    HealthCheck,
    HealthCheckResponse,
    Request,
    Response,
}

/// Message handler trait
#[async_trait::async_trait]
pub trait MessageHandler {
    async fn handle(
        &self,
        peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String>;
}

impl P2PProtocol {
    pub fn new(
        config: ProtocolConfig,
        transport: Arc<P2PTransport>,
        peer_registry: Arc<PeerRegistry>,
        metrics: Arc<MetricsCollector>,
    ) -> Self {
        let (protocol_events, _) = mpsc::unbounded_channel();
        
        Self {
            config,
            transport,
            peer_registry,
            metrics,
            message_handlers: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            protocol_events,
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Start the P2P protocol handler
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("🔗 Starting P2P protocol handler ({})", self.config.protocol_version);

        // Register default message handlers
        self.register_default_handlers().await;

        // Start message processing loop
        let message_processor = self.clone_for_tasks();
        tokio::spawn(async move {
            message_processor.process_messages().await;
        });

        // Start request timeout manager
        let timeout_manager = self.clone_for_tasks();
        tokio::spawn(async move {
            timeout_manager.manage_request_timeouts().await;
        });

        // Start periodic health checks
        let health_checker = self.clone_for_tasks();
        tokio::spawn(async move {
            health_checker.periodic_health_checks().await;
        });

        Ok(())
    }

    /// Register default message handlers
    async fn register_default_handlers(&self) {
        let mut handlers = self.message_handlers.write().await;
        
        handlers.insert(MessageType::SphinxForward, Box::new(SphinxForwardHandler::new()));
        handlers.insert(MessageType::TopologySync, Box::new(TopologySyncHandler::new()));
        handlers.insert(MessageType::RouteDiscovery, Box::new(RouteDiscoveryHandler::new()));
        handlers.insert(MessageType::PeerQuery, Box::new(PeerQueryHandler::new()));
        handlers.insert(MessageType::HealthCheck, Box::new(HealthCheckHandler::new()));
        handlers.insert(MessageType::CoverTraffic, Box::new(CoverTrafficHandler::new()));
    }

    /// Process incoming transport messages
    async fn process_messages(&self) {
        let mut event_receiver = self.transport.subscribe_events();
        
        while let Some(event) = event_receiver.recv().await {
            match event {
                TransportEvent::MessageReceived(peer_id, data) => {
                    self.handle_raw_message(&peer_id, &data).await;
                }
                TransportEvent::PeerConnected(peer_id, _addr) => {
                    // Send initial peer info exchange
                    self.send_peer_info(&peer_id).await;
                }
                TransportEvent::PeerDisconnected(peer_id) => {
                    self.handle_peer_disconnect(&peer_id).await;
                }
                _ => {} // Handle other events as needed
            }
        }
    }

    /// Handle raw message from transport
    async fn handle_raw_message(&self, peer_id: &PeerId, data: &[u8]) {
        // Deserialize P2P message
        let message: P2PMessage = match bincode::deserialize(data) {
            Ok(msg) => msg,
            Err(e) => {
                println!("Failed to deserialize P2P message from {}: {}", peer_id.to_hex(), e);
                return;
            }
        };

        // Validate message size
        if data.len() > self.config.max_message_size {
            println!("Message from {} exceeds size limit", peer_id.to_hex());
            return;
        }

        // Check if this is a response to a pending request
        if let Some(request_id) = self.extract_request_id(&message) {
            if self.handle_response(request_id, message.clone()).await {
                return; // Response handled, don't process further
            }
        }

        // Route to appropriate message handler
        let message_type = self.get_message_type(&message);
        
        let handlers = self.message_handlers.read().await;
        if let Some(handler) = handlers.get(&message_type) {
            match handler.handle(peer_id, message.clone(), self).await {
                Ok(Some(response)) => {
                    // Send response back to peer
                    if let Err(e) = self.send_message(peer_id, response).await {
                        println!("Failed to send response to {}: {}", peer_id.to_hex(), e);
                    }
                }
                Ok(None) => {
                    // Message handled, no response needed
                }
                Err(e) => {
                    println!("Error handling message from {}: {}", peer_id.to_hex(), e);
                }
            }
        } else {
            println!("No handler for message type {:?} from {}", message_type, peer_id.to_hex());
        }

        // Emit protocol event
        let _ = self.protocol_events.send(ProtocolEvent::MessageReceived {
            peer_id: peer_id.clone(),
            message,
        });
    }

    /// Send a message to a specific peer
    pub async fn send_message(&self, peer_id: &PeerId, message: P2PMessage) -> Result<(), String> {
        let data = bincode::serialize(&message)
            .map_err(|e| format!("Serialization error: {}", e))?;
        
        if data.len() > self.config.max_message_size {
            return Err("Message exceeds size limit".to_string());
        }

        self.transport.send_to_peer(peer_id, &data).await
    }

    /// Send a request and wait for response
    pub async fn send_request(
        &self,
        peer_id: &PeerId,
        method: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let request_id = RequestId::new();
        let (response_sender, mut response_receiver) = mpsc::unbounded_channel();
        
        // Store pending request
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), PendingRequest {
                peer_id: peer_id.clone(),
                sent_at: SystemTime::now(),
                timeout: SystemTime::now() + self.config.request_timeout,
                response_sender,
            });
        }

        // Send request
        let request = P2PMessage::Request {
            request_id: request_id.clone(),
            method,
            payload,
        };

        self.send_message(peer_id, request).await?;

        // Wait for response with timeout
        tokio::select! {
            response = response_receiver.recv() => {
                match response {
                    Some(P2PMessage::Response { success, payload, .. }) => {
                        if success {
                            Ok(payload)
                        } else {
                            Err("Request failed".to_string())
                        }
                    }
                    _ => Err("Invalid response".to_string())
                }
            }
            _ = tokio::time::sleep(self.config.request_timeout) => {
                // Clean up pending request
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                
                Err("Request timeout".to_string())
            }
        }
    }

    /// Broadcast message to all connected peers
    pub async fn broadcast_message(&self, message: P2PMessage) -> usize {
        let data = match bincode::serialize(&message) {
            Ok(data) => data,
            Err(e) => {
                println!("Failed to serialize broadcast message: {}", e);
                return 0;
            }
        };

        self.transport.broadcast(&data).await
    }

    /// Send message to specific set of peers
    pub async fn multicast_message(&self, peer_ids: &[PeerId], message: P2PMessage) -> usize {
        let data = match bincode::serialize(&message) {
            Ok(data) => data,
            Err(e) => {
                println!("Failed to serialize multicast message: {}", e);
                return 0;
            }
        };

        let mut sent_count = 0;
        for peer_id in peer_ids {
            if self.transport.send_to_peer(peer_id, &data).await.is_ok() {
                sent_count += 1;
            }
        }
        sent_count
    }

    /// Handle peer disconnection
    async fn handle_peer_disconnect(&self, peer_id: &PeerId) {
        // Clean up any pending requests for this peer
        let mut pending = self.pending_requests.write().await;
        let mut to_remove = Vec::new();
        
        for (request_id, pending_request) in pending.iter() {
            if &pending_request.peer_id == peer_id {
                to_remove.push(request_id.clone());
            }
        }
        
        for request_id in to_remove {
            pending.remove(&request_id);
            let _ = self.protocol_events.send(ProtocolEvent::RequestTimedOut {
                request_id,
                peer_id: peer_id.clone(),
            });
        }
    }

    /// Send peer info to a connected peer
    async fn send_peer_info(&self, peer_id: &PeerId) {
        // This would send our current peer information
        // For now, we'll implement a basic version
        
        let health_check = P2PMessage::HealthCheck {
            timestamp: SystemTime::now(),
            metrics_snapshot: None, // Could include basic metrics
        };

        let _ = self.send_message(peer_id, health_check).await;
    }

    /// Handle response to pending request
    async fn handle_response(&self, request_id: RequestId, message: P2PMessage) -> bool {
        let pending_request = {
            let mut pending = self.pending_requests.write().await;
            pending.remove(&request_id)
        };

        if let Some(request) = pending_request {
            let _ = request.response_sender.send(message);
            true
        } else {
            false
        }
    }

    /// Extract request ID from message if it has one
    fn extract_request_id(&self, message: &P2PMessage) -> Option<RequestId> {
        match message {
            P2PMessage::Response { request_id, .. } |
            P2PMessage::RouteResponse { request_id, .. } |
            P2PMessage::PeerQueryResponse { request_id, .. } |
            P2PMessage::HealthCheckResponse { request_id, .. } => Some(request_id.clone()),
            _ => None,
        }
    }

    /// Get message type for routing to handlers
    fn get_message_type(&self, message: &P2PMessage) -> MessageType {
        match message {
            P2PMessage::SphinxForward { .. } => MessageType::SphinxForward,
            P2PMessage::SphinxResponse { .. } => MessageType::SphinxResponse,
            P2PMessage::TopologySync { .. } => MessageType::TopologySync,
            P2PMessage::RouteDiscovery { .. } => MessageType::RouteDiscovery,
            P2PMessage::RouteResponse { .. } => MessageType::RouteResponse,
            P2PMessage::PeerInfo { .. } => MessageType::PeerInfo,
            P2PMessage::PeerQuery { .. } => MessageType::PeerQuery,
            P2PMessage::PeerQueryResponse { .. } => MessageType::PeerQueryResponse,
            P2PMessage::CoverTraffic { .. } => MessageType::CoverTraffic,
            P2PMessage::HealthCheck { .. } => MessageType::HealthCheck,
            P2PMessage::HealthCheckResponse { .. } => MessageType::HealthCheckResponse,
            P2PMessage::Request { .. } => MessageType::Request,
            P2PMessage::Response { .. } => MessageType::Response,
        }
    }

    /// Manage request timeouts
    async fn manage_request_timeouts(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.cleanup_timed_out_requests().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }

    /// Clean up requests that have timed out
    async fn cleanup_timed_out_requests(&self) {
        let now = SystemTime::now();
        let mut timed_out = Vec::new();
        
        {
            let pending = self.pending_requests.read().await;
            for (request_id, request) in pending.iter() {
                if now >= request.timeout {
                    timed_out.push((request_id.clone(), request.peer_id.clone()));
                }
            }
        }
        
        if !timed_out.is_empty() {
            let mut pending = self.pending_requests.write().await;
            for (request_id, peer_id) in timed_out {
                pending.remove(&request_id);
                let _ = self.protocol_events.send(ProtocolEvent::RequestTimedOut {
                    request_id,
                    peer_id,
                });
            }
        }
    }

    /// Perform periodic health checks with peers
    async fn periodic_health_checks(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.perform_health_checks().await;
                }
                _ = self.shutdown_signal.notified() => {
                    break;
                }
            }
        }
    }

    /// Send health check to all connected peers
    async fn perform_health_checks(&self) {
        let connected_peers = self.transport.get_connected_peers().await;
        
        let health_check = P2PMessage::HealthCheck {
            timestamp: SystemTime::now(),
            metrics_snapshot: None,
        };

        for peer_id in connected_peers {
            let _ = self.send_message(&peer_id, health_check.clone()).await;
        }
    }

    /// Subscribe to protocol events
    pub fn subscribe_events(&self) -> mpsc::UnboundedReceiver<ProtocolEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        // In a real implementation, you'd maintain a list of subscribers
        receiver
    }

    /// Shutdown the protocol handler
    pub async fn shutdown(&self) {
        self.shutdown_signal.notify_waiters();
        println!("🔗 P2P protocol handler shutting down");
    }

    fn clone_for_tasks(&self) -> P2PProtocolTask {
        P2PProtocolTask {
            config: self.config.clone(),
            transport: self.transport.clone(),
            peer_registry: self.peer_registry.clone(),
            metrics: self.metrics.clone(),
            message_handlers: self.message_handlers.clone(),
            pending_requests: self.pending_requests.clone(),
            protocol_events: self.protocol_events.clone(),
            shutdown_signal: self.shutdown_signal.clone(),
        }
    }
}

#[derive(Clone)]
struct P2PProtocolTask {
    config: ProtocolConfig,
    transport: Arc<P2PTransport>,
    peer_registry: Arc<PeerRegistry>,
    metrics: Arc<MetricsCollector>,
    message_handlers: Arc<RwLock<HashMap<MessageType, Box<dyn MessageHandler + Send + Sync>>>>,
    pending_requests: Arc<RwLock<HashMap<RequestId, PendingRequest>>>,
    protocol_events: mpsc::UnboundedSender<ProtocolEvent>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

impl P2PProtocolTask {
    async fn process_messages(&self) {
        // Implementation moved for async reasons
    }
    
    async fn manage_request_timeouts(&self) {
        // Implementation details...
    }
    
    async fn periodic_health_checks(&self) {
        // Implementation details...
    }
}

// Default message handlers
pub struct SphinxForwardHandler;
impl SphinxForwardHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for SphinxForwardHandler {
    async fn handle(
        &self,
        _peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::SphinxForward { packet, next_hop, path_info } = message {
            // Process Sphinx packet (this would integrate with the actual Sphinx mixer)
            println!("📦 Processing Sphinx packet with {} hops remaining", 
                     path_info.total_hops - path_info.current_hop);
            
            // For now, just acknowledge receipt
            Ok(Some(P2PMessage::SphinxResponse {
                request_id: RequestId::new(),
                success: true,
                error: None,
            }))
        } else {
            Err("Invalid message type for SphinxForwardHandler".to_string())
        }
    }
}

pub struct TopologySyncHandler;
impl TopologySyncHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for TopologySyncHandler {
    async fn handle(
        &self,
        peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::TopologySync { nodes, timestamp } = message {
            println!("🗺️ Received topology sync from {} with {} nodes", 
                     peer_id.to_hex(), nodes.len());
            // Process topology information
            Ok(None)
        } else {
            Err("Invalid message type for TopologySyncHandler".to_string())
        }
    }
}

pub struct RouteDiscoveryHandler;
impl RouteDiscoveryHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for RouteDiscoveryHandler {
    async fn handle(
        &self,
        _peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::RouteDiscovery { target, max_hops, path } = message {
            // Simple route discovery - in practice this would be more sophisticated
            let response = P2PMessage::RouteResponse {
                request_id: RequestId::new(),
                path: vec![target], // Simplified - direct route
                latency_estimate: Duration::from_millis(50),
            };
            Ok(Some(response))
        } else {
            Err("Invalid message type for RouteDiscoveryHandler".to_string())
        }
    }
}

pub struct PeerQueryHandler;
impl PeerQueryHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for PeerQueryHandler {
    async fn handle(
        &self,
        _peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::PeerQuery { criteria } = message {
            // Query our peer registry
            let peers = protocol.peer_registry.get_routing_peers(
                criteria.min_stake.unwrap_or(0)
            ).await;
            
            let filtered_peers: Vec<_> = peers.into_iter()
                .filter(|peer| {
                    if let Some(ref regions) = criteria.regions {
                        if !regions.contains(&peer.region) {
                            return false;
                        }
                    }
                    if let Some(min_rep) = criteria.min_reputation {
                        if peer.reputation.reliability_score < min_rep {
                            return false;
                        }
                    }
                    true
                })
                .take(criteria.max_results)
                .collect();

            let response = P2PMessage::PeerQueryResponse {
                request_id: RequestId::new(),
                peers: filtered_peers,
            };
            
            Ok(Some(response))
        } else {
            Err("Invalid message type for PeerQueryHandler".to_string())
        }
    }
}

pub struct HealthCheckHandler;
impl HealthCheckHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for HealthCheckHandler {
    async fn handle(
        &self,
        _peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::HealthCheck { timestamp, .. } = message {
            let response = P2PMessage::HealthCheckResponse {
                request_id: RequestId::new(),
                status: NodeStatus::Healthy, // Would check actual node status
                metrics: None, // Could include serialized metrics
            };
            Ok(Some(response))
        } else {
            Err("Invalid message type for HealthCheckHandler".to_string())
        }
    }
}

pub struct CoverTrafficHandler;
impl CoverTrafficHandler {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl MessageHandler for CoverTrafficHandler {
    async fn handle(
        &self,
        peer_id: &PeerId,
        message: P2PMessage,
        protocol: &P2PProtocol,
    ) -> Result<Option<P2PMessage>, String> {
        if let P2PMessage::CoverTraffic { dummy_packet, timestamp } = message {
            // Process cover traffic (usually just discard after validation)
            println!("🎭 Received cover traffic from {} ({} bytes)", 
                     peer_id.to_hex(), dummy_packet.len());
            Ok(None) // No response needed for cover traffic
        } else {
            Err("Invalid message type for CoverTrafficHandler".to_string())
        }
    }
}