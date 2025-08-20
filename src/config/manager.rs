// Configuration management and loading
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

use super::{AppConfig, NodeConfig, P2PConfig, SphinxConfig, MetricsConfig, LoggingConfig, StorageConfig, SecurityConfig};

/// Configuration manager for handling app configuration loading and saving
pub struct ConfigManager {
    config_path: PathBuf,
    config: Arc<RwLock<AppConfig>>,
    auto_save: bool,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            config: Arc::new(RwLock::new(AppConfig::default())),
            auto_save: false,
        }
    }
    
    /// Create a new configuration manager with auto-save enabled
    pub fn with_auto_save(config_path: PathBuf) -> Self {
        Self {
            config_path,
            config: Arc::new(RwLock::new(AppConfig::default())),
            auto_save: true,
        }
    }
    
    /// Load configuration from file
    pub async fn load(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.config_path.exists() {
            info!("Loading configuration from: {:?}", self.config_path);
            
            let content = fs::read_to_string(&self.config_path)?;
            let loaded_config: AppConfig = if self.config_path.extension()
                .and_then(|ext| ext.to_str()) == Some("json") {
                serde_json::from_str(&content)?
            } else {
                // Default to YAML
                serde_yaml::from_str(&content)?
            };
            
            let mut config = self.config.write().await;
            *config = loaded_config;
            
            debug!("Configuration loaded successfully");
        } else {
            info!("Configuration file not found, using defaults: {:?}", self.config_path);
            // Save default configuration
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Save configuration to file
    pub async fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Saving configuration to: {:?}", self.config_path);
        
        // Ensure directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let config = self.config.read().await;
        let content = if self.config_path.extension()
            .and_then(|ext| ext.to_str()) == Some("json") {
            serde_json::to_string_pretty(&*config)?
        } else {
            // Default to YAML
            serde_yaml::to_string(&*config)?
        };
        
        fs::write(&self.config_path, content)?;
        debug!("Configuration saved successfully");
        
        Ok(())
    }
    
    /// Get a copy of the current configuration
    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }
    
    /// Update the entire configuration
    pub async fn update_config(&self, new_config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update node configuration
    pub async fn update_node_config(&self, node_config: NodeConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.node = node_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update P2P configuration
    pub async fn update_p2p_config(&self, p2p_config: P2PConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.p2p = p2p_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update Sphinx configuration
    pub async fn update_sphinx_config(&self, sphinx_config: SphinxConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.sphinx = sphinx_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update metrics configuration
    pub async fn update_metrics_config(&self, metrics_config: MetricsConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.metrics = metrics_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update logging configuration
    pub async fn update_logging_config(&self, logging_config: LoggingConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.logging = logging_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update storage configuration
    pub async fn update_storage_config(&self, storage_config: StorageConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.storage = storage_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Update security configuration
    pub async fn update_security_config(&self, security_config: SecurityConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut config = self.config.write().await;
            config.security = security_config;
        }
        
        if self.auto_save {
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Get node configuration
    pub async fn get_node_config(&self) -> NodeConfig {
        self.config.read().await.node.clone()
    }
    
    /// Get P2P configuration
    pub async fn get_p2p_config(&self) -> P2PConfig {
        self.config.read().await.p2p.clone()
    }
    
    /// Get Sphinx configuration
    pub async fn get_sphinx_config(&self) -> SphinxConfig {
        self.config.read().await.sphinx.clone()
    }
    
    /// Get metrics configuration
    pub async fn get_metrics_config(&self) -> MetricsConfig {
        self.config.read().await.metrics.clone()
    }
    
    /// Get logging configuration
    pub async fn get_logging_config(&self) -> LoggingConfig {
        self.config.read().await.logging.clone()
    }
    
    /// Get storage configuration
    pub async fn get_storage_config(&self) -> StorageConfig {
        self.config.read().await.storage.clone()
    }
    
    /// Get security configuration
    pub async fn get_security_config(&self) -> SecurityConfig {
        self.config.read().await.security.clone()
    }
    
    /// Validate configuration
    pub async fn validate(&self) -> Result<(), Vec<String>> {
        let config = self.config.read().await;
        let mut errors = Vec::new();
        
        // Validate node configuration
        if config.node.node_id.is_empty() {
            errors.push("Node ID cannot be empty".to_string());
        }
        
        if config.node.stake == 0 {
            errors.push("Node stake must be greater than 0".to_string());
        }
        
        // Validate P2P configuration
        if config.p2p.max_connections == 0 {
            errors.push("Max connections must be greater than 0".to_string());
        }
        
        if config.p2p.bootstrap_peers.is_empty() {
            errors.push("At least one bootstrap peer must be configured".to_string());
        }
        
        // Validate Sphinx configuration
        if config.sphinx.packet_size == 0 {
            errors.push("Sphinx packet size must be greater than 0".to_string());
        }
        
        if config.sphinx.max_hops == 0 {
            errors.push("Sphinx max hops must be greater than 0".to_string());
        }
        
        // Validate metrics configuration
        if config.metrics.enabled && config.metrics.collection_interval.as_secs() == 0 {
            errors.push("Metrics collection interval must be greater than 0".to_string());
        }
        
        // Validate logging configuration
        if !["error", "warn", "info", "debug", "trace"].contains(&config.logging.level.as_str()) {
            errors.push("Invalid logging level".to_string());
        }
        
        // Validate storage configuration
        if config.storage.cache_size == 0 {
            errors.push("Storage cache size must be greater than 0".to_string());
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Create configuration from environment variables
    pub async fn load_from_env(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut config = self.config.write().await;
        
        // Load node configuration from environment
        if let Ok(node_id) = std::env::var("NYM_NODE_ID") {
            config.node.node_id = node_id;
        }
        
        if let Ok(region) = std::env::var("NYM_NODE_REGION") {
            config.node.region = region;
        }
        
        if let Ok(stake_str) = std::env::var("NYM_NODE_STAKE") {
            if let Ok(stake) = stake_str.parse::<u64>() {
                config.node.stake = stake;
            }
        }
        
        if let Ok(bind_addr) = std::env::var("NYM_BIND_ADDRESS") {
            if let Ok(addr) = bind_addr.parse() {
                config.node.bind_address = addr;
            }
        }
        
        // Load P2P configuration from environment
        if let Ok(listen_addr) = std::env::var("NYM_P2P_LISTEN_ADDRESS") {
            if let Ok(addr) = listen_addr.parse() {
                config.p2p.listen_address = addr;
            }
        }
        
        if let Ok(max_conn_str) = std::env::var("NYM_P2P_MAX_CONNECTIONS") {
            if let Ok(max_conn) = max_conn_str.parse::<usize>() {
                config.p2p.max_connections = max_conn;
            }
        }
        
        // Load logging configuration from environment
        if let Ok(log_level) = std::env::var("NYM_LOG_LEVEL") {
            config.logging.level = log_level;
        }
        
        // Load metrics configuration from environment
        if let Ok(metrics_enabled) = std::env::var("NYM_METRICS_ENABLED") {
            config.metrics.enabled = metrics_enabled.parse().unwrap_or(true);
        }
        
        if let Ok(prometheus_bind) = std::env::var("NYM_PROMETHEUS_BIND") {
            if let Ok(addr) = prometheus_bind.parse() {
                config.metrics.prometheus_bind = addr;
            }
        }
        
        info!("Configuration loaded from environment variables");
        
        if self.auto_save {
            drop(config); // Release the lock before saving
            self.save().await?;
        }
        
        Ok(())
    }
    
    /// Get configuration file path
    pub fn get_config_path(&self) -> &Path {
        &self.config_path
    }
    
    /// Enable or disable auto-save
    pub fn set_auto_save(&mut self, auto_save: bool) {
        self.auto_save = auto_save;
    }
    
    /// Check if auto-save is enabled
    pub fn is_auto_save(&self) -> bool {
        self.auto_save
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config_path: self.config_path.clone(),
            config: self.config.clone(),
            auto_save: self.auto_save,
        }
    }
}

/// Configuration builder for programmatic configuration creation
pub struct ConfigBuilder {
    config: AppConfig,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: AppConfig::default(),
        }
    }
    
    pub fn node(mut self, node_config: NodeConfig) -> Self {
        self.config.node = node_config;
        self
    }
    
    pub fn p2p(mut self, p2p_config: P2PConfig) -> Self {
        self.config.p2p = p2p_config;
        self
    }
    
    pub fn sphinx(mut self, sphinx_config: SphinxConfig) -> Self {
        self.config.sphinx = sphinx_config;
        self
    }
    
    pub fn metrics(mut self, metrics_config: MetricsConfig) -> Self {
        self.config.metrics = metrics_config;
        self
    }
    
    pub fn logging(mut self, logging_config: LoggingConfig) -> Self {
        self.config.logging = logging_config;
        self
    }
    
    pub fn storage(mut self, storage_config: StorageConfig) -> Self {
        self.config.storage = storage_config;
        self
    }
    
    pub fn security(mut self, security_config: SecurityConfig) -> Self {
        self.config.security = security_config;
        self
    }
    
    pub fn build(self) -> AppConfig {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}