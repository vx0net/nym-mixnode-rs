// Load balancing and failover mechanisms for high availability
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

pub mod health_monitor;
pub mod strategies;

use crate::metrics::collector::MetricsCollector;

/// Load balancer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    pub strategy: LoadBalancingStrategy,
    pub health_check_interval: Duration,
    pub health_check_timeout: Duration,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub failover_threshold: f64,
    pub recovery_threshold: f64,
    pub circuit_breaker_enabled: bool,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout: Duration,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            strategy: LoadBalancingStrategy::WeightedRoundRobin,
            health_check_interval: Duration::from_secs(30),
            health_check_timeout: Duration::from_secs(5),
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            failover_threshold: 0.7, // Fail when health < 70%
            recovery_threshold: 0.9, // Recover when health > 90%
            circuit_breaker_enabled: true,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(60),
        }
    }
}

/// Load balancing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
    WeightedLeastConnections,
    IPHash,
    LatencyBased,
    ResourceBased,
    Adaptive,
}

/// Node health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    pub node_id: String,
    pub address: SocketAddr,
    pub is_healthy: bool,
    pub health_score: f64, // 0.0 - 1.0
    pub last_check: SystemTime,
    pub consecutive_failures: u32,
    pub response_time: Option<Duration>,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub packet_loss: Option<f64>,
    pub connection_count: Option<u32>,
    pub circuit_breaker_state: CircuitBreakerState,
}

/// Circuit breaker states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,
    Open { until: SystemTime },
    HalfOpen,
}

/// Load balancer events
#[derive(Debug, Clone)]
pub enum LoadBalancerEvent {
    NodeAdded(String, SocketAddr),
    NodeRemoved(String),
    NodeHealthChanged(String, f64),
    NodeFailed(String, String), // node_id, reason
    NodeRecovered(String),
    FailoverTriggered(String, String), // from_node, to_node
    CircuitBreakerOpened(String),
    CircuitBreakerClosed(String),
    LoadBalancingStrategyChanged(LoadBalancingStrategy),
}

/// Main load balancer
pub struct LoadBalancer {
    config: LoadBalancerConfig,
    nodes: Arc<RwLock<HashMap<String, NodeHealth>>>,
    strategy: Arc<RwLock<Box<dyn LoadBalancingStrategyTrait + Send + Sync>>>,
    health_monitor: Arc<health_monitor::HealthMonitor>,
    // failover_manager: Arc<failover::FailoverManager>, // Simplified for compilation
    metrics: Arc<MetricsCollector>,
    event_sender: mpsc::UnboundedSender<LoadBalancerEvent>,
    
    // Load balancing state
    round_robin_index: Arc<RwLock<usize>>,
    connection_counts: Arc<RwLock<HashMap<String, u32>>>,
    response_times: Arc<RwLock<HashMap<String, VecDeque<Duration>>>>,
}

impl LoadBalancer {
    pub fn new(
        config: LoadBalancerConfig,
        metrics: Arc<MetricsCollector>,
    ) -> (Self, mpsc::UnboundedReceiver<LoadBalancerEvent>) {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        let health_monitor = Arc::new(health_monitor::HealthMonitor::new(
            config.health_check_interval,
            config.health_check_timeout,
            nodes.clone(),
            event_sender.clone(),
        ));
        
        // let failover_manager = Arc::new(failover::FailoverManager::new(
        //     config.clone(),
        //     nodes.clone(), 
        //     event_sender.clone(),
        // ));
        
        let strategy: Box<dyn LoadBalancingStrategyTrait + Send + Sync> = match config.strategy {
            LoadBalancingStrategy::RoundRobin => Box::new(strategies::RoundRobinStrategy::new()),
            LoadBalancingStrategy::WeightedRoundRobin => Box::new(strategies::WeightedRoundRobinStrategy::new()),
            LoadBalancingStrategy::LeastConnections => Box::new(strategies::LeastConnectionsStrategy::new()),
            LoadBalancingStrategy::WeightedLeastConnections => Box::new(strategies::WeightedLeastConnectionsStrategy::new()),
            LoadBalancingStrategy::IPHash => Box::new(strategies::IPHashStrategy::new()),
            LoadBalancingStrategy::LatencyBased => Box::new(strategies::LatencyBasedStrategy::new()),
            LoadBalancingStrategy::ResourceBased => Box::new(strategies::ResourceBasedStrategy::new()),
            LoadBalancingStrategy::Adaptive => Box::new(strategies::AdaptiveStrategy::new()),
        };
        
        let load_balancer = Self {
            config,
            nodes,
            strategy: Arc::new(RwLock::new(strategy)),
            health_monitor,
            // failover_manager,
            metrics,
            event_sender,
            round_robin_index: Arc::new(RwLock::new(0)),
            connection_counts: Arc::new(RwLock::new(HashMap::new())),
            response_times: Arc::new(RwLock::new(HashMap::new())),
        };
        
        (load_balancer, event_receiver)
    }
    
    /// Add a node to the load balancer
    pub async fn add_node(&self, node_id: String, address: SocketAddr, weight: Option<f64>) -> Result<(), String> {
        let mut nodes = self.nodes.write().unwrap();
        
        if nodes.contains_key(&node_id) {
            return Err(format!("Node {} already exists", node_id));
        }
        
        let node_health = NodeHealth {
            node_id: node_id.clone(),
            address,
            is_healthy: false, // Will be determined by health check
            health_score: 0.0,
            last_check: SystemTime::now(),
            consecutive_failures: 0,
            response_time: None,
            cpu_usage: None,
            memory_usage: None,
            packet_loss: None,
            connection_count: None,
            circuit_breaker_state: CircuitBreakerState::Closed,
        };
        
        nodes.insert(node_id.clone(), node_health);
        
        // Initialize connection count
        self.connection_counts.write().unwrap().insert(node_id.clone(), 0);
        
        // Initialize response time tracking
        self.response_times.write().unwrap().insert(node_id.clone(), VecDeque::new());
        
        info!("Added node {} at {}", node_id, address);
        
        // Send event
        let _ = self.event_sender.send(LoadBalancerEvent::NodeAdded(node_id, address));
        
        Ok(())
    }
    
    /// Remove a node from the load balancer
    pub async fn remove_node(&self, node_id: &str) -> Result<(), String> {
        let mut nodes = self.nodes.write().unwrap();
        
        if !nodes.contains_key(node_id) {
            return Err(format!("Node {} not found", node_id));
        }
        
        nodes.remove(node_id);
        self.connection_counts.write().unwrap().remove(node_id);
        self.response_times.write().unwrap().remove(node_id);
        
        info!("Removed node {}", node_id);
        
        // Send event
        let _ = self.event_sender.send(LoadBalancerEvent::NodeRemoved(node_id.to_string()));
        
        Ok(())
    }
    
    /// Select the best node for a request
    pub async fn select_node(&self, client_ip: Option<&str>) -> Option<String> {
        let nodes = self.nodes.read().unwrap();
        let healthy_nodes: Vec<_> = nodes.values()
            .filter(|node| self.is_node_available(node))
            .collect();
        
        if healthy_nodes.is_empty() {
            warn!("No healthy nodes available for load balancing");
            return None;
        }
        
        let strategy = self.strategy.read().unwrap();
        let connection_counts = self.connection_counts.read().unwrap();
        let response_times = self.response_times.read().unwrap();
        
        let selection_context = SelectionContext {
            healthy_nodes: &healthy_nodes,
            connection_counts: &connection_counts,
            response_times: &response_times,
            client_ip,
            round_robin_index: &self.round_robin_index,
        };
        
        strategy.select_node(&selection_context)
    }
    
    /// Check if a node is available (healthy and circuit breaker is closed)
    fn is_node_available(&self, node: &NodeHealth) -> bool {
        node.is_healthy && matches!(node.circuit_breaker_state, CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen)
    }
    
    /// Record a successful request to a node
    pub async fn record_success(&self, node_id: &str, response_time: Duration) {
        // Update response time tracking
        {
            let mut response_times = self.response_times.write().unwrap();
            if let Some(times) = response_times.get_mut(node_id) {
                times.push_back(response_time);
                // Keep only last 100 response times
                if times.len() > 100 {
                    times.pop_front();
                }
            }
        }
        
        // Update node health
        {
            let mut nodes = self.nodes.write().unwrap();
            if let Some(node) = nodes.get_mut(node_id) {
                node.response_time = Some(response_time);
                node.consecutive_failures = 0;
                
                // Close circuit breaker if it was half-open
                if matches!(node.circuit_breaker_state, CircuitBreakerState::HalfOpen) {
                    node.circuit_breaker_state = CircuitBreakerState::Closed;
                    let _ = self.event_sender.send(LoadBalancerEvent::CircuitBreakerClosed(node_id.to_string()));
                }
            }
        }
        
        self.metrics.record_load_balancer_success(node_id, response_time.as_millis() as f64);
    }
    
    /// Record a failed request to a node
    pub async fn record_failure(&self, node_id: &str, error: &str) {
        let mut should_open_circuit_breaker = false;
        
        // Update node health
        {
            let mut nodes = self.nodes.write().unwrap();
            if let Some(node) = nodes.get_mut(node_id) {
                node.consecutive_failures += 1;
                
                // Check if we should open the circuit breaker
                if self.config.circuit_breaker_enabled 
                    && node.consecutive_failures >= self.config.circuit_breaker_threshold 
                    && matches!(node.circuit_breaker_state, CircuitBreakerState::Closed) {
                    should_open_circuit_breaker = true;
                    node.circuit_breaker_state = CircuitBreakerState::Open {
                        until: SystemTime::now() + self.config.circuit_breaker_timeout,
                    };
                }
            }
        }
        
        if should_open_circuit_breaker {
            warn!("Circuit breaker opened for node {} after {} failures", node_id, self.config.circuit_breaker_threshold);
            let _ = self.event_sender.send(LoadBalancerEvent::CircuitBreakerOpened(node_id.to_string()));
        }
        
        let _ = self.event_sender.send(LoadBalancerEvent::NodeFailed(node_id.to_string(), error.to_string()));
        self.metrics.record_load_balancer_failure(node_id, error);
    }
    
    /// Increment connection count for a node
    pub async fn increment_connections(&self, node_id: &str) {
        let mut connection_counts = self.connection_counts.write().unwrap();
        *connection_counts.entry(node_id.to_string()).or_insert(0) += 1;
    }
    
    /// Decrement connection count for a node
    pub async fn decrement_connections(&self, node_id: &str) {
        let mut connection_counts = self.connection_counts.write().unwrap();
        if let Some(count) = connection_counts.get_mut(node_id) {
            *count = count.saturating_sub(1);
        }
    }
    
    /// Get current load balancer statistics
    pub async fn get_stats(&self) -> LoadBalancerStats {
        let nodes = self.nodes.read().unwrap();
        let connection_counts = self.connection_counts.read().unwrap();
        
        let total_nodes = nodes.len();
        let healthy_nodes = nodes.values().filter(|n| n.is_healthy).count();
        let total_connections: u32 = connection_counts.values().sum();
        
        let mut node_stats = Vec::new();
        for (node_id, node) in nodes.iter() {
            let connections = connection_counts.get(node_id).copied().unwrap_or(0);
            let avg_response_time = self.response_times.read().unwrap()
                .get(node_id)
                .map(|times| {
                    if times.is_empty() {
                        Duration::ZERO
                    } else {
                        let total: Duration = times.iter().sum();
                        total / times.len() as u32
                    }
                });
            
            node_stats.push(NodeStats {
                node_id: node_id.clone(),
                address: node.address,
                is_healthy: node.is_healthy,
                health_score: node.health_score,
                connections,
                avg_response_time,
                consecutive_failures: node.consecutive_failures,
                circuit_breaker_state: node.circuit_breaker_state.clone(),
            });
        }
        
        LoadBalancerStats {
            total_nodes,
            healthy_nodes,
            total_connections,
            strategy: self.config.strategy.clone(),
            node_stats,
        }
    }
    
    /// Start the load balancer background tasks
    pub async fn start(&self) {
        info!("Starting load balancer with strategy: {:?}", self.config.strategy);
        
        // Start health monitoring
        self.health_monitor.start().await;
        
        // Start circuit breaker recovery task
        self.start_circuit_breaker_recovery().await;
        
        info!("Load balancer started successfully");
    }
    
    /// Start circuit breaker recovery task
    async fn start_circuit_breaker_recovery(&self) {
        let nodes = self.nodes.clone();
        let event_sender = self.event_sender.clone();
        let recovery_interval = Duration::from_secs(30);
        
        tokio::spawn(async move {
            let mut interval = interval(recovery_interval);
            
            loop {
                interval.tick().await;
                
                let mut nodes_to_recover = Vec::new();
                
                // Check for circuit breakers that should transition to half-open
                {
                    let mut nodes_guard = nodes.write().unwrap();
                    for (node_id, node) in nodes_guard.iter_mut() {
                        if let CircuitBreakerState::Open { until } = node.circuit_breaker_state {
                            if SystemTime::now() > until {
                                node.circuit_breaker_state = CircuitBreakerState::HalfOpen;
                                nodes_to_recover.push(node_id.clone());
                            }
                        }
                    }
                }
                
                // Send recovery events
                for node_id in nodes_to_recover {
                    debug!("Circuit breaker transitioning to half-open for node {}", node_id);
                    // The next successful request will close the circuit breaker
                }
            }
        });
    }
}

/// Selection context for load balancing strategies
pub struct SelectionContext<'a> {
    pub healthy_nodes: &'a [&'a NodeHealth],
    pub connection_counts: &'a HashMap<String, u32>,
    pub response_times: &'a HashMap<String, VecDeque<Duration>>,
    pub client_ip: Option<&'a str>,
    pub round_robin_index: &'a Arc<RwLock<usize>>,
}

/// Load balancing strategy trait
pub trait LoadBalancingStrategyTrait {
    fn select_node(&self, context: &SelectionContext) -> Option<String>;
}

/// Load balancer statistics
#[derive(Debug, Clone, Serialize)]
pub struct LoadBalancerStats {
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub total_connections: u32,
    pub strategy: LoadBalancingStrategy,
    pub node_stats: Vec<NodeStats>,
}

/// Individual node statistics
#[derive(Debug, Clone, Serialize)]
pub struct NodeStats {
    pub node_id: String,
    pub address: SocketAddr,
    pub is_healthy: bool,
    pub health_score: f64,
    pub connections: u32,
    pub avg_response_time: Option<Duration>,
    pub consecutive_failures: u32,
    pub circuit_breaker_state: CircuitBreakerState,
}
