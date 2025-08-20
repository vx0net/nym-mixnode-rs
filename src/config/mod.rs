// Node configuration module
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use serde::{Serialize, Deserialize};

pub mod manager;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub node: NodeConfig,
    pub p2p: P2PConfig,
    pub sphinx: SphinxConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            p2p: P2PConfig::default(),
            sphinx: SphinxConfig::default(),
            metrics: MetricsConfig::default(),
            logging: LoggingConfig::default(),
            storage: StorageConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// Node-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub version: String,
    pub region: String,
    pub stake: u64,
    pub data_dir: PathBuf,
    pub bind_address: SocketAddr,
    pub advertise_address: Option<SocketAddr>,
    pub max_threads: Option<usize>,
    pub enable_cover_traffic: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            region: "unknown".to_string(),
            stake: 1000,
            data_dir: PathBuf::from("./data"),
            bind_address: "0.0.0.0:1789".parse().unwrap(),
            advertise_address: None,
            max_threads: None, // Auto-detect
            enable_cover_traffic: true,
        }
    }
}

/// P2P networking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfig {
    pub listen_address: SocketAddr,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub discovery_interval: Duration,
    pub bootstrap_peers: Vec<SocketAddr>,
    pub enable_upnp: bool,
    pub enable_mdns: bool,
    pub protocol_version: String,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0:1789".parse().unwrap(),
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
            discovery_interval: Duration::from_secs(60),
            bootstrap_peers: vec![
                "bootstrap1.nymtech.net:1789".parse().unwrap(),
                "bootstrap2.nymtech.net:1789".parse().unwrap(),
            ],
            enable_upnp: false,
            enable_mdns: false,
            protocol_version: "nym-mixnode/1.0.0".to_string(),
        }
    }
}

/// Sphinx packet processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SphinxConfig {
    pub max_delay: Duration,
    pub delay_distribution: String, // "uniform", "exponential", "poisson"
    pub cover_traffic_rate: f64,
    pub batch_size: usize,
    pub packet_size: usize,
    pub max_hops: u8,
    pub enable_simd: bool,
    pub memory_pool_size: usize,
}

impl Default for SphinxConfig {
    fn default() -> Self {
        Self {
            max_delay: Duration::from_secs(10),
            delay_distribution: "exponential".to_string(),
            cover_traffic_rate: 0.1,
            batch_size: 100,
            packet_size: 1024,
            max_hops: 5,
            enable_simd: true,
            memory_pool_size: 10000,
        }
    }
}

/// Metrics and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub prometheus_bind: SocketAddr,
    pub collection_interval: Duration,
    pub retention_period: Duration,
    pub export_format: String, // "prometheus", "json", "csv"
    pub export_path: Option<PathBuf>,
    pub enable_alerts: bool,
    pub alert_thresholds: HashMap<String, f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        let mut thresholds = HashMap::new();
        thresholds.insert("cpu_usage".to_string(), 80.0);
        thresholds.insert("memory_usage".to_string(), 85.0);
        thresholds.insert("packet_loss_rate".to_string(), 5.0);
        
        Self {
            enabled: true,
            prometheus_bind: "0.0.0.0:9090".parse().unwrap(),
            collection_interval: Duration::from_secs(10),
            retention_period: Duration::from_secs(86400), // 24 hours
            export_format: "prometheus".to_string(),
            export_path: None,
            enable_alerts: true,
            alert_thresholds: thresholds,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String, // "json", "compact", "full"
    pub file_path: Option<PathBuf>,
    pub max_file_size: u64,
    pub max_files: u32,
    pub enable_audit: bool,
    pub audit_file: Option<PathBuf>,
    pub enable_structured: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "compact".to_string(),
            file_path: Some(PathBuf::from("./logs/nym-mixnode.log")),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_files: 10,
            enable_audit: true,
            audit_file: Some(PathBuf::from("./logs/audit.log")),
            enable_structured: true,
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub cache_size: usize,
    pub backup_enabled: bool,
    pub backup_interval: Duration,
    pub backup_retention: u32,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            cache_size: 10000,
            backup_enabled: true,
            backup_interval: Duration::from_secs(3600), // 1 hour
            backup_retention: 7, // Keep 7 backups
            compression_enabled: true,
            encryption_enabled: false,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enable_firewall: bool,
    pub allowed_ips: Vec<String>,
    pub blocked_ips: Vec<String>,
    pub rate_limit_enabled: bool,
    pub max_requests_per_second: u32,
    pub enable_ddos_protection: bool,
    pub intrusion_detection: bool,
    pub certificate_path: Option<PathBuf>,
    pub private_key_path: Option<PathBuf>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_firewall: false, // Disabled by default for development
            allowed_ips: vec!["127.0.0.1".to_string(), "::1".to_string()],
            blocked_ips: Vec::new(),
            rate_limit_enabled: true,
            max_requests_per_second: 1000,
            enable_ddos_protection: true,
            intrusion_detection: false,
            certificate_path: None,
            private_key_path: None,
        }
    }
}