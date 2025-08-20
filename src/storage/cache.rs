// In-memory cache
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct Cache {
    store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    max_size: usize,
}

impl Cache {
    pub fn new(max_size: usize) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }
    
    pub fn put(&self, key: String, value: Vec<u8>) {
        let mut store = self.store.write().unwrap();
        
        // Simple eviction: remove oldest if at capacity
        if store.len() >= self.max_size {
            if let Some(oldest_key) = store.keys().next().cloned() {
                store.remove(&oldest_key);
            }
        }
        
        store.insert(key, value);
    }
    
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.store.read().unwrap().get(key).cloned()
    }
    
    pub fn remove(&self, key: &str) {
        self.store.write().unwrap().remove(key);
    }
    
    pub fn clear(&self) {
        self.store.write().unwrap().clear();
    }
    
    pub fn size(&self) -> usize {
        self.store.read().unwrap().len()
    }
}
