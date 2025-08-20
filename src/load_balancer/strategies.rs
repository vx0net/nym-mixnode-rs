// Load balancing strategy implementations
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use super::{LoadBalancingStrategyTrait, SelectionContext, NodeHealth};

/// Round Robin strategy
pub struct RoundRobinStrategy;

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl LoadBalancingStrategyTrait for RoundRobinStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let mut index = context.round_robin_index.write().unwrap();
        *index = (*index + 1) % context.healthy_nodes.len();
        
        Some(context.healthy_nodes[*index].node_id.clone())
    }
}

/// Weighted Round Robin strategy
pub struct WeightedRoundRobinStrategy {
    weights: Arc<RwLock<HashMap<String, f64>>>,
    current_weights: Arc<RwLock<HashMap<String, f64>>>,
}

impl WeightedRoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            weights: Arc::new(RwLock::new(HashMap::new())),
            current_weights: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn set_weight(&self, node_id: &str, weight: f64) {
        self.weights.write().unwrap().insert(node_id.to_string(), weight);
        self.current_weights.write().unwrap().insert(node_id.to_string(), 0.0);
    }
}

impl LoadBalancingStrategyTrait for WeightedRoundRobinStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let weights = self.weights.read().unwrap();
        let mut current_weights = self.current_weights.write().unwrap();
        
        let mut best_node = None;
        let mut max_current_weight = f64::NEG_INFINITY;
        
        // Calculate total weight of all healthy nodes
        let total_weight: f64 = context.healthy_nodes.iter()
            .map(|node| weights.get(&node.node_id).copied().unwrap_or(1.0))
            .sum();
        
        if total_weight == 0.0 {
            // Fallback to round robin
            let mut index = context.round_robin_index.write().unwrap();
            *index = (*index + 1) % context.healthy_nodes.len();
            return Some(context.healthy_nodes[*index].node_id.clone());
        }
        
        // Weighted round robin algorithm
        for node in context.healthy_nodes {
            let weight = weights.get(&node.node_id).copied().unwrap_or(1.0);
            let current_weight = current_weights.entry(node.node_id.clone()).or_insert(0.0);
            
            *current_weight += weight;
            
            if *current_weight > max_current_weight {
                max_current_weight = *current_weight;
                best_node = Some(node.node_id.clone());
            }
        }
        
        // Decrease current weight of selected node
        if let Some(ref node_id) = best_node {
            if let Some(current_weight) = current_weights.get_mut(node_id) {
                *current_weight -= total_weight;
            }
        }
        
        best_node
    }
}

/// Least Connections strategy
pub struct LeastConnectionsStrategy;

impl LeastConnectionsStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl LoadBalancingStrategyTrait for LeastConnectionsStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let mut min_connections = u32::MAX;
        let mut best_node = None;
        
        for node in context.healthy_nodes {
            let connections = context.connection_counts.get(&node.node_id).copied().unwrap_or(0);
            if connections < min_connections {
                min_connections = connections;
                best_node = Some(node.node_id.clone());
            }
        }
        
        best_node
    }
}

/// Weighted Least Connections strategy
pub struct WeightedLeastConnectionsStrategy {
    weights: Arc<RwLock<HashMap<String, f64>>>,
}

impl WeightedLeastConnectionsStrategy {
    pub fn new() -> Self {
        Self {
            weights: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn set_weight(&self, node_id: &str, weight: f64) {
        self.weights.write().unwrap().insert(node_id.to_string(), weight);
    }
}

impl LoadBalancingStrategyTrait for WeightedLeastConnectionsStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let weights = self.weights.read().unwrap();
        let mut best_ratio = f64::INFINITY;
        let mut best_node = None;
        
        for node in context.healthy_nodes {
            let connections = context.connection_counts.get(&node.node_id).copied().unwrap_or(0);
            let weight = weights.get(&node.node_id).copied().unwrap_or(1.0);
            
            let ratio = if weight > 0.0 {
                connections as f64 / weight
            } else {
                f64::INFINITY
            };
            
            if ratio < best_ratio {
                best_ratio = ratio;
                best_node = Some(node.node_id.clone());
            }
        }
        
        best_node
    }
}

/// IP Hash strategy (consistent hashing)
pub struct IPHashStrategy;

impl IPHashStrategy {
    pub fn new() -> Self {
        Self
    }
    
    fn hash_ip(&self, ip: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        ip.hash(&mut hasher);
        hasher.finish()
    }
}

impl LoadBalancingStrategyTrait for IPHashStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let client_ip = context.client_ip.unwrap_or("unknown");
        let hash = self.hash_ip(client_ip);
        let index = (hash as usize) % context.healthy_nodes.len();
        
        Some(context.healthy_nodes[index].node_id.clone())
    }
}

/// Latency-based strategy (select node with lowest average response time)
pub struct LatencyBasedStrategy;

impl LatencyBasedStrategy {
    pub fn new() -> Self {
        Self
    }
    
    fn calculate_avg_response_time(&self, times: &VecDeque<Duration>) -> Duration {
        if times.is_empty() {
            Duration::from_millis(1000) // Default high latency for new nodes
        } else {
            let total: Duration = times.iter().sum();
            total / times.len() as u32
        }
    }
}

impl LoadBalancingStrategyTrait for LatencyBasedStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let mut best_latency = Duration::from_secs(u64::MAX);
        let mut best_node = None;
        
        for node in context.healthy_nodes {
            let avg_latency = if let Some(times) = context.response_times.get(&node.node_id) {
                self.calculate_avg_response_time(times)
            } else {
                Duration::from_millis(1000) // Default for new nodes
            };
            
            if avg_latency < best_latency {
                best_latency = avg_latency;
                best_node = Some(node.node_id.clone());
            }
        }
        
        best_node
    }
}

/// Resource-based strategy (select node with best resource utilization)
pub struct ResourceBasedStrategy;

impl ResourceBasedStrategy {
    pub fn new() -> Self {
        Self
    }
    
    fn calculate_resource_score(&self, node: &NodeHealth) -> f64 {
        let mut score = 1.0;
        
        // Factor in CPU usage (lower is better)
        if let Some(cpu) = node.cpu_usage {
            score *= 1.0 - (cpu / 100.0).min(1.0);
        }
        
        // Factor in memory usage (lower is better)
        if let Some(memory) = node.memory_usage {
            score *= 1.0 - (memory / 100.0).min(1.0);
        }
        
        // Factor in packet loss (lower is better)
        if let Some(packet_loss) = node.packet_loss {
            score *= 1.0 - packet_loss.min(1.0);
        }
        
        // Factor in response time (lower is better)
        if let Some(response_time) = node.response_time {
            let response_ms = response_time.as_millis() as f64;
            score *= 1.0 / (1.0 + response_ms / 1000.0); // Normalize to 0-1 range
        }
        
        score.max(0.0)
    }
}

impl LoadBalancingStrategyTrait for ResourceBasedStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        if context.healthy_nodes.is_empty() {
            return None;
        }
        
        let mut best_score = f64::NEG_INFINITY;
        let mut best_node = None;
        
        for node in context.healthy_nodes {
            let score = self.calculate_resource_score(node);
            
            if score > best_score {
                best_score = score;
                best_node = Some(node.node_id.clone());
            }
        }
        
        best_node
    }
}

/// Adaptive strategy (combines multiple strategies based on conditions)
pub struct AdaptiveStrategy {
    round_robin: RoundRobinStrategy,
    least_connections: LeastConnectionsStrategy,
    latency_based: LatencyBasedStrategy,
    resource_based: ResourceBasedStrategy,
}

impl AdaptiveStrategy {
    pub fn new() -> Self {
        Self {
            round_robin: RoundRobinStrategy::new(),
            least_connections: LeastConnectionsStrategy::new(),
            latency_based: LatencyBasedStrategy::new(),
            resource_based: ResourceBasedStrategy::new(),
        }
    }
    
    fn determine_best_strategy(&self, context: &SelectionContext) -> AdaptiveStrategyChoice {
        let total_connections: u32 = context.connection_counts.values().sum();
        let avg_connections = if context.healthy_nodes.is_empty() {
            0.0
        } else {
            total_connections as f64 / context.healthy_nodes.len() as f64
        };
        
        // High load scenario - prioritize least connections
        if avg_connections > 100.0 {
            return AdaptiveStrategyChoice::LeastConnections;
        }
        
        // Check if we have latency data for most nodes
        let nodes_with_latency = context.healthy_nodes.iter()
            .filter(|node| context.response_times.get(&node.node_id)
                .map_or(false, |times| !times.is_empty()))
            .count();
        
        let latency_coverage = nodes_with_latency as f64 / context.healthy_nodes.len() as f64;
        
        // Good latency data available - use latency-based selection
        if latency_coverage > 0.7 {
            return AdaptiveStrategyChoice::LatencyBased;
        }
        
        // Check resource utilization data availability
        let nodes_with_resources = context.healthy_nodes.iter()
            .filter(|node| node.cpu_usage.is_some() || node.memory_usage.is_some())
            .count();
        
        let resource_coverage = nodes_with_resources as f64 / context.healthy_nodes.len() as f64;
        
        // Good resource data available - use resource-based selection
        if resource_coverage > 0.5 {
            return AdaptiveStrategyChoice::ResourceBased;
        }
        
        // Default to round robin for simplicity
        AdaptiveStrategyChoice::RoundRobin
    }
}

#[derive(Debug)]
enum AdaptiveStrategyChoice {
    RoundRobin,
    LeastConnections,
    LatencyBased,
    ResourceBased,
}

impl LoadBalancingStrategyTrait for AdaptiveStrategy {
    fn select_node(&self, context: &SelectionContext) -> Option<String> {
        let strategy = self.determine_best_strategy(context);
        
        match strategy {
            AdaptiveStrategyChoice::RoundRobin => self.round_robin.select_node(context),
            AdaptiveStrategyChoice::LeastConnections => self.least_connections.select_node(context),
            AdaptiveStrategyChoice::LatencyBased => self.latency_based.select_node(context),
            AdaptiveStrategyChoice::ResourceBased => self.resource_based.select_node(context),
        }
    }
}
