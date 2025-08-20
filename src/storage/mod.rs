// Storage layer for persistence
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use tracing::{info, error};

pub mod database;
pub mod cache;
pub mod backup;

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub enable_persistence: bool,
    pub max_cache_size: usize,
    pub backup_interval: std::time::Duration,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            enable_persistence: true,
            max_cache_size: 10000,
            backup_interval: std::time::Duration::from_secs(3600),
        }
    }
}

/// Main storage manager
pub struct StorageManager {
    config: StorageConfig,
    database: database::Database,
    cache: cache::Cache,
}

impl StorageManager {
    pub fn new(config: StorageConfig) -> Result<Self, String> {
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
        
        let database = database::Database::new(&config.data_dir)?;
        let cache = cache::Cache::new(config.max_cache_size);
        
        Ok(Self {
            config,
            database,
            cache,
        })
    }
    
    pub async fn store_node_state(&self, key: &str, value: &[u8]) -> Result<(), String> {
        self.cache.put(key.to_string(), value.to_vec());
        
        if self.config.enable_persistence {
            self.database.store(key, value).await?;
        }
        
        Ok(())
    }
    
    pub async fn load_node_state(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        // Try cache first
        if let Some(value) = self.cache.get(key) {
            return Ok(Some(value));
        }
        
        // Try database
        if self.config.enable_persistence {
            let value = self.database.load(key).await?;
            if let Some(ref v) = value {
                self.cache.put(key.to_string(), v.clone());
            }
            Ok(value)
        } else {
            Ok(None)
        }
    }
}
