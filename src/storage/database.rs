// Database abstraction layer
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use serde::{Serialize, Deserialize};

pub struct Database {
    data_dir: PathBuf,
    // In-memory store for simplicity (replace with real DB in production)
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Database {
    pub fn new(data_dir: &PathBuf) -> Result<Self, String> {
        Ok(Self {
            data_dir: data_dir.clone(),
            store: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub async fn store(&self, key: &str, value: &[u8]) -> Result<(), String> {
        self.store.write().unwrap().insert(key.to_string(), value.to_vec());
        
        // In real implementation, persist to disk
        let file_path = self.data_dir.join(format!("{}.dat", key));
        std::fs::write(file_path, value)
            .map_err(|e| format!("Failed to write to disk: {}", e))?;
        
        Ok(())
    }
    
    pub async fn load(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        // Try memory first
        if let Some(value) = self.store.read().unwrap().get(key) {
            return Ok(Some(value.clone()));
        }
        
        // Try disk
        let file_path = self.data_dir.join(format!("{}.dat", key));
        if file_path.exists() {
            let value = std::fs::read(file_path)
                .map_err(|e| format!("Failed to read from disk: {}", e))?;
            
            // Cache in memory
            self.store.write().unwrap().insert(key.to_string(), value.clone());
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
    
    pub async fn delete(&self, key: &str) -> Result<(), String> {
        self.store.write().unwrap().remove(key);
        
        let file_path = self.data_dir.join(format!("{}.dat", key));
        if file_path.exists() {
            std::fs::remove_file(file_path)
                .map_err(|e| format!("Failed to delete from disk: {}", e))?;
        }
        
        Ok(())
    }
}
