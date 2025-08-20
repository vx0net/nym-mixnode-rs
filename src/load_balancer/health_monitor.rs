// Health monitoring for load balancer nodes
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, Instant};
use tokio::time::{interval, timeout};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

use super::{NodeHealth, LoadBalancerEvent, CircuitBreakerState};

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub tcp_timeout: Duration,
    pub http_timeout: Duration,
    pub ping_timeout: Duration,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub custom_health_endpoint: Option<String>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            tcp_timeout: Duration::from_secs(5),
            http_timeout: Duration::from_secs(10),
            ping_timeout: Duration::from_secs(3),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            custom_health_endpoint: None,
        }
    }
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    TcpConnect,
    HttpGet { endpoint: String, expected_status: u16 },
    HttpPost { endpoint: String, body: String, expected_status: u16 },
    Ping,
    Custom { command: String, expected_exit_code: i32 },
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub success: bool,
    pub response_time: Duration,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Health monitor
pub struct HealthMonitor {
    check_interval: Duration,
    check_timeout: Duration,
    nodes: Arc<RwLock<HashMap<String, NodeHealth>>>,
    event_sender: mpsc::UnboundedSender<LoadBalancerEvent>,
    config: HealthCheckConfig,
    http_client: reqwest::Client,
}

impl HealthMonitor {
    pub fn new(
        check_interval: Duration,
        check_timeout: Duration,
        nodes: Arc<RwLock<HashMap<String, NodeHealth>>>,
        event_sender: mpsc::UnboundedSender<LoadBalancerEvent>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(check_timeout)
            .build()
            .expect("Failed to create HTTP client for health monitoring");
        
        Self {
            check_interval,
            check_timeout,
            nodes,
            event_sender,
            config: HealthCheckConfig::default(),
            http_client,
        }
    }
    
    /// Start health monitoring
    pub async fn start(&self) {
        info!("Starting health monitor with interval: {:?}", self.check_interval);
        
        let nodes = self.nodes.clone();
        let event_sender = self.event_sender.clone();
        let check_interval = self.check_interval;
        let config = self.config.clone();
        let http_client = self.http_client.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(check_interval);
            
            loop {
                interval.tick().await;
                
                // Get snapshot of current nodes
                let node_list: Vec<(String, SocketAddr)> = {
                    let nodes_guard = nodes.read().unwrap();
                    nodes_guard.iter()
                        .map(|(id, health)| (id.clone(), health.address))
                        .collect()
                };
                
                // Perform health checks in parallel
                let mut check_tasks = Vec::new();
                
                for (node_id, address) in node_list {
                    let nodes_clone = nodes.clone();
                    let event_sender_clone = event_sender.clone();
                    let config_clone = config.clone();
                    let http_client_clone = http_client.clone();
                    
                    let task = tokio::spawn(async move {
                        Self::check_node_health(
                            &node_id,
                            address,
                            &nodes_clone,
                            &event_sender_clone,
                            &config_clone,
                            &http_client_clone,
                        ).await;
                    });
                    
                    check_tasks.push(task);
                }
                
                // Wait for all health checks to complete
                for task in check_tasks {
                    if let Err(e) = task.await {
                        error!("Health check task failed: {}", e);
                    }
                }
            }
        });
    }
    
    /// Check health of a single node
    async fn check_node_health(
        node_id: &str,
        address: SocketAddr,
        nodes: &Arc<RwLock<HashMap<String, NodeHealth>>>,
        event_sender: &mpsc::UnboundedSender<LoadBalancerEvent>,
        config: &HealthCheckConfig,
        http_client: &reqwest::Client,
    ) {
        let checks = vec![
            HealthCheckType::TcpConnect,
            HealthCheckType::HttpGet {
                endpoint: "/health".to_string(),
                expected_status: 200,
            },
            HealthCheckType::Ping,
        ];
        
        let mut overall_health = 0.0;
        let mut total_checks = 0;
        let mut fastest_response = Duration::from_secs(u64::MAX);
        let mut check_errors = Vec::new();
        
        for check_type in &checks {
            let result = Self::perform_health_check(
                address,
                check_type,
                config,
                http_client,
            ).await;
            
            total_checks += 1;
            
            if result.success {
                overall_health += 1.0;
                fastest_response = fastest_response.min(result.response_time);
            } else {
                if let Some(error) = result.error_message {
                    check_errors.push(error);
                }
            }
        }
        
        // Calculate health score (0.0 - 1.0)
        let health_score = if total_checks > 0 {
            overall_health / total_checks as f64
        } else {
            0.0
        };
        
        let is_healthy = health_score >= 0.7; // Require 70% of checks to pass
        let response_time = if fastest_response == Duration::from_secs(u64::MAX) {
            None
        } else {
            Some(fastest_response)
        };
        
        // Get resource metrics first (before holding lock)
        let resources = Self::get_node_resources(address, http_client).await;
        
        // Update node health
        let previous_health = {
            let mut nodes_guard = nodes.write().unwrap();
            if let Some(node) = nodes_guard.get_mut(node_id) {
                let previous_health = node.is_healthy;
                
                node.is_healthy = is_healthy;
                node.health_score = health_score;
                node.last_check = SystemTime::now();
                node.response_time = response_time;
                
                if !is_healthy {
                    node.consecutive_failures += 1;
                } else {
                    node.consecutive_failures = 0;
                }
                
                // Update resource metrics if available
                if let Some(resources) = resources {
                    node.cpu_usage = resources.cpu_usage;
                    node.memory_usage = resources.memory_usage;
                    node.packet_loss = resources.packet_loss;
                    node.connection_count = resources.connection_count;
                }
                
                Some(previous_health)
            } else {
                None
            }
        };
        
        // Send events for health changes
        if let Some(previous_health) = previous_health {
            if previous_health != is_healthy {
                if is_healthy {
                    info!("Node {} recovered (health score: {:.2})", node_id, health_score);
                    let _ = event_sender.send(LoadBalancerEvent::NodeRecovered(node_id.to_string()));
                } else {
                    let error_msg = if check_errors.is_empty() {
                        "Health checks failed".to_string()
                    } else {
                        check_errors.join(", ")
                    };
                    
                    warn!("Node {} marked unhealthy (health score: {:.2}): {}", 
                          node_id, health_score, error_msg);
                    let _ = event_sender.send(LoadBalancerEvent::NodeFailed(
                        node_id.to_string(),
                        error_msg,
                    ));
                }
            }
            
            // Always send health change event for monitoring
            let _ = event_sender.send(LoadBalancerEvent::NodeHealthChanged(
                node_id.to_string(),
                health_score,
            ));
        }
        
        debug!("Health check for {} completed: healthy={}, score={:.2}, response_time={:?}", 
               node_id, is_healthy, health_score, response_time);
    }
    
    /// Perform a specific type of health check
    async fn perform_health_check(
        address: SocketAddr,
        check_type: &HealthCheckType,
        config: &HealthCheckConfig,
        http_client: &reqwest::Client,
    ) -> HealthCheckResult {
        let start_time = Instant::now();
        
        let result = match check_type {
            HealthCheckType::TcpConnect => {
                Self::tcp_health_check(address, config).await
            },
            HealthCheckType::HttpGet { endpoint, expected_status } => {
                Self::http_health_check(address, endpoint, *expected_status, http_client).await
            },
            HealthCheckType::HttpPost { endpoint, body, expected_status } => {
                Self::http_post_health_check(address, endpoint, body, *expected_status, http_client).await
            },
            HealthCheckType::Ping => {
                Self::ping_health_check(address, config).await
            },
            HealthCheckType::Custom { command, expected_exit_code } => {
                Self::custom_health_check(command, *expected_exit_code).await
            },
        };
        
        let response_time = start_time.elapsed();
        
        HealthCheckResult {
            success: result.is_ok(),
            response_time,
            error_message: result.err(),
            metadata: HashMap::new(),
        }
    }
    
    /// TCP connection health check
    async fn tcp_health_check(address: SocketAddr, config: &HealthCheckConfig) -> Result<(), String> {
        timeout(config.tcp_timeout, TcpStream::connect(address))
            .await
            .map_err(|_| "TCP connection timeout".to_string())?
            .map_err(|e| format!("TCP connection failed: {}", e))?;
        
        Ok(())
    }
    
    /// HTTP GET health check
    async fn http_health_check(
        address: SocketAddr,
        endpoint: &str,
        expected_status: u16,
        http_client: &reqwest::Client,
    ) -> Result<(), String> {
        let url = format!("http://{}{}", address, endpoint);
        
        let response = http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        
        if response.status().as_u16() != expected_status {
            return Err(format!(
                "HTTP status mismatch: expected {}, got {}",
                expected_status,
                response.status().as_u16()
            ));
        }
        
        Ok(())
    }
    
    /// HTTP POST health check
    async fn http_post_health_check(
        address: SocketAddr,
        endpoint: &str,
        body: &str,
        expected_status: u16,
        http_client: &reqwest::Client,
    ) -> Result<(), String> {
        let url = format!("http://{}{}", address, endpoint);
        
        let response = http_client
            .post(&url)
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("HTTP POST request failed: {}", e))?;
        
        if response.status().as_u16() != expected_status {
            return Err(format!(
                "HTTP status mismatch: expected {}, got {}",
                expected_status,
                response.status().as_u16()
            ));
        }
        
        Ok(())
    }
    
    /// Ping health check
    async fn ping_health_check(address: SocketAddr, config: &HealthCheckConfig) -> Result<(), String> {
        let output = timeout(
            config.ping_timeout,
            tokio::process::Command::new("ping")
                .arg("-c")
                .arg("1")
                .arg("-W")
                .arg("2")
                .arg(address.ip().to_string())
                .output()
        )
        .await
        .map_err(|_| "Ping timeout".to_string())?
        .map_err(|e| format!("Ping command failed: {}", e))?;
        
        if !output.status.success() {
            return Err("Ping failed".to_string());
        }
        
        Ok(())
    }
    
    /// Custom command health check
    async fn custom_health_check(command: &str, expected_exit_code: i32) -> Result<(), String> {
        let mut cmd_parts = command.split_whitespace();
        let program = cmd_parts.next().ok_or("Empty command")?;
        let args: Vec<&str> = cmd_parts.collect();
        
        let output = tokio::process::Command::new(program)
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Command execution failed: {}", e))?;
        
        let exit_code = output.status.code().unwrap_or(-1);
        if exit_code != expected_exit_code {
            return Err(format!(
                "Command exit code mismatch: expected {}, got {}",
                expected_exit_code, exit_code
            ));
        }
        
        Ok(())
    }
    
    /// Get node resource metrics
    async fn get_node_resources(
        address: SocketAddr,
        http_client: &reqwest::Client,
    ) -> Option<NodeResources> {
        let url = format!("http://{}/metrics", address);
        
        let response = http_client
            .get(&url)
            .send()
            .await
            .ok()?;
        
        if response.status() != 200 {
            return None;
        }
        
        let metrics_text = response.text().await.ok()?;
        Self::parse_node_metrics(&metrics_text)
    }
    
    /// Parse Prometheus-style metrics
    fn parse_node_metrics(metrics: &str) -> Option<NodeResources> {
        let mut cpu_usage = None;
        let mut memory_usage = None;
        let mut packet_loss = None;
        let mut connection_count = None;
        
        for line in metrics.lines() {
            if let Some(value) = Self::extract_metric_value(line, "cpu_usage_percent") {
                cpu_usage = value.parse().ok();
            } else if let Some(value) = Self::extract_metric_value(line, "memory_usage_percent") {
                memory_usage = value.parse().ok();
            } else if let Some(value) = Self::extract_metric_value(line, "packet_loss_rate") {
                packet_loss = value.parse().ok();
            } else if let Some(value) = Self::extract_metric_value(line, "active_connections") {
                connection_count = value.parse().ok();
            }
        }
        
        Some(NodeResources {
            cpu_usage,
            memory_usage,
            packet_loss,
            connection_count,
        })
    }
    
    /// Extract metric value from Prometheus-style line
    fn extract_metric_value<'a>(line: &'a str, metric_name: &str) -> Option<&'a str> {
        if line.starts_with(metric_name) && !line.starts_with(&format!("{}_", metric_name)) {
            if let Some(space_pos) = line.find(' ') {
                return Some(&line[space_pos + 1..].trim());
            }
        }
        None
    }
}

/// Node resource metrics
#[derive(Debug, Clone)]
struct NodeResources {
    cpu_usage: Option<f64>,
    memory_usage: Option<f64>,
    packet_loss: Option<f64>,
    connection_count: Option<u32>,
}
