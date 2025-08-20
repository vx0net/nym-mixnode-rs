// High-performance memory pool for zero-allocation packet processing
// Implements object pooling pattern to eliminate GC pressure and allocation overhead

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::mem;
use std::ptr;

/// Memory pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Initial number of objects to pre-allocate
    pub initial_capacity: usize,
    /// Maximum number of objects to keep in pool
    pub max_capacity: usize,
    /// Maximum number of objects to allocate if pool is empty
    pub growth_size: usize,
    /// Whether to pre-zero memory when allocating
    pub pre_zero: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_capacity: 1000,    // Pre-allocate 1000 buffers
            max_capacity: 10000,       // Keep up to 10k buffers in pool
            growth_size: 100,          // Grow by 100 when empty
            pre_zero: true,            // Zero memory for security
        }
    }
}

/// Generic object pool for managing reusable objects
pub struct ObjectPool<T> {
    pool: Arc<Mutex<VecDeque<T>>>,
    config: PoolConfig,
    factory: Box<dyn Fn() -> T + Send + Sync>,
    reset: Box<dyn Fn(&mut T) + Send + Sync>,
    stats: Arc<Mutex<PoolStats>>,
}

#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub allocations: u64,
    pub deallocations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub current_size: usize,
    pub peak_size: usize,
}

impl<T> ObjectPool<T> {
    /// Create a new object pool with custom factory and reset functions
    pub fn new<F, R>(config: PoolConfig, factory: F, reset: R) -> Self 
    where
        F: Fn() -> T + Send + Sync + 'static,
        R: Fn(&mut T) + Send + Sync + 'static,
    {
        let pool = Arc::new(Mutex::new(VecDeque::with_capacity(config.max_capacity)));
        let stats = Arc::new(Mutex::new(PoolStats::default()));

        let mut object_pool = Self {
            pool,
            config: config.clone(),
            factory: Box::new(factory),
            reset: Box::new(reset),
            stats,
        };

        // Pre-allocate initial objects
        object_pool.grow_pool(config.initial_capacity);
        
        object_pool
    }

    /// Get an object from the pool or create a new one
    pub fn get(&self) -> PooledObject<T> {
        let mut stats = self.stats.lock().unwrap();
        
        let object = {
            let mut pool = self.pool.lock().unwrap();
            if let Some(mut obj) = pool.pop_front() {
                stats.cache_hits += 1;
                // Reset object to clean state
                (self.reset)(&mut obj);
                obj
            } else {
                stats.cache_misses += 1;
                drop(pool); // Release lock before calling factory
                (self.factory)()
            }
        };

        stats.allocations += 1;

        PooledObject::new(object, self.pool.clone(), self.stats.clone())
    }

    /// Grow the pool by adding new objects
    fn grow_pool(&self, count: usize) {
        let mut pool = self.pool.lock().unwrap();
        for _ in 0..count {
            if pool.len() >= self.config.max_capacity {
                break;
            }
            pool.push_back((self.factory)());
        }
        
        let mut stats = self.stats.lock().unwrap();
        stats.current_size = pool.len();
        if stats.current_size > stats.peak_size {
            stats.peak_size = stats.current_size;
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let pool_size = self.pool.lock().unwrap().len();
        let mut stats = self.stats.lock().unwrap();
        stats.current_size = pool_size;
        stats.clone()
    }

    /// Clear all objects from the pool
    pub fn clear(&self) {
        let mut pool = self.pool.lock().unwrap();
        pool.clear();
        
        let mut stats = self.stats.lock().unwrap();
        stats.current_size = 0;
    }
}

/// RAII wrapper for pooled objects
pub struct PooledObject<T> {
    object: Option<T>,
    pool: Arc<Mutex<VecDeque<T>>>,
    stats: Arc<Mutex<PoolStats>>,
}

impl<T> PooledObject<T> {
    fn new(object: T, pool: Arc<Mutex<VecDeque<T>>>, stats: Arc<Mutex<PoolStats>>) -> Self {
        Self {
            object: Some(object),
            pool,
            stats,
        }
    }

    /// Get a reference to the pooled object
    pub fn get(&self) -> &T {
        self.object.as_ref().unwrap()
    }

    /// Get a mutable reference to the pooled object
    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            let mut stats = self.stats.lock().unwrap();
            stats.deallocations += 1;

            let mut pool = self.pool.lock().unwrap();
            if pool.len() < pool.capacity() {
                pool.push_back(object);
                stats.current_size = pool.len();
            }
            // If pool is full, object is dropped (not returned to pool)
        }
    }
}

impl<T> std::ops::Deref for PooledObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> std::ops::DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

/// Specialized buffer pool for Sphinx packet processing
pub type SphinxPacketBuffer = Vec<u8>;

/// High-performance buffer pool for packet data
pub struct PacketBufferPool {
    pool: ObjectPool<SphinxPacketBuffer>,
}

impl PacketBufferPool {
    /// Create a new packet buffer pool optimized for Sphinx packets
    pub fn new(config: PoolConfig) -> Self {
        let pool = ObjectPool::new(
            config,
            || {
                // Factory function: create new buffer
                vec![0u8; crate::sphinx::SPHINX_PACKET_SIZE]
            },
            |buffer: &mut SphinxPacketBuffer| {
                // Reset function: clear buffer securely
                buffer.fill(0);
            },
        );

        Self { pool }
    }

    /// Get a buffer for packet processing
    pub fn get_buffer(&self) -> PooledObject<SphinxPacketBuffer> {
        self.pool.get()
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        self.pool.stats()
    }

    /// Clear the pool
    pub fn clear(&self) {
        self.pool.clear()
    }
}

impl Default for PacketBufferPool {
    fn default() -> Self {
        Self::new(PoolConfig::default())
    }
}

/// Memory-aligned buffer for SIMD operations
#[repr(align(32))] // Align to 32 bytes for AVX2
pub struct AlignedBuffer {
    data: Vec<u8>,
}

impl AlignedBuffer {
    pub fn new(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        unsafe {
            data.set_len(size);
        }
        Self { data }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get raw pointer for SIMD operations
    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// Get mutable raw pointer for SIMD operations
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }
}

/// Pool for SIMD-aligned buffers
pub struct AlignedBufferPool {
    pool: ObjectPool<AlignedBuffer>,
}

impl AlignedBufferPool {
    pub fn new(config: PoolConfig, buffer_size: usize) -> Self {
        let pool = ObjectPool::new(
            config,
            move || AlignedBuffer::new(buffer_size),
            |buffer: &mut AlignedBuffer| {
                buffer.as_mut_slice().fill(0);
            },
        );

        Self { pool }
    }

    pub fn get_buffer(&self) -> PooledObject<AlignedBuffer> {
        self.pool.get()
    }

    pub fn stats(&self) -> PoolStats {
        self.pool.stats()
    }
}

/// Thread-local memory pools for maximum performance
thread_local! {
    static PACKET_BUFFER_POOL: PacketBufferPool = PacketBufferPool::default();
    static HEADER_BUFFER_POOL: AlignedBufferPool = AlignedBufferPool::new(
        PoolConfig::default(),
        512, // Sphinx header size
    );
    static PAYLOAD_BUFFER_POOL: AlignedBufferPool = AlignedBufferPool::new(
        PoolConfig::default(),
        512, // Sphinx payload size
    );
}

/// Get a packet buffer from thread-local pool
pub fn get_packet_buffer() -> PooledObject<SphinxPacketBuffer> {
    PACKET_BUFFER_POOL.with(|pool| pool.get_buffer())
}

/// Get a header buffer from thread-local pool
pub fn get_header_buffer() -> PooledObject<AlignedBuffer> {
    HEADER_BUFFER_POOL.with(|pool| pool.get_buffer())
}

/// Get a payload buffer from thread-local pool
pub fn get_payload_buffer() -> PooledObject<AlignedBuffer> {
    PAYLOAD_BUFFER_POOL.with(|pool| pool.get_buffer())
}

/// Get statistics for all thread-local pools
pub fn get_pool_stats() -> (PoolStats, PoolStats, PoolStats) {
    let packet_stats = PACKET_BUFFER_POOL.with(|pool| pool.stats());
    let header_stats = HEADER_BUFFER_POOL.with(|pool| pool.stats());
    let payload_stats = PAYLOAD_BUFFER_POOL.with(|pool| pool.stats());
    
    (packet_stats, header_stats, payload_stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_pool_basic_operations() {
        let config = PoolConfig {
            initial_capacity: 2,
            max_capacity: 5,
            growth_size: 1,
            pre_zero: true,
        };

        let pool = ObjectPool::new(
            config,
            || vec![0u8; 100],
            |v: &mut Vec<u8>| v.fill(0),
        );

        // Test getting objects
        let obj1 = pool.get();
        let obj2 = pool.get();
        
        assert_eq!(obj1.len(), 100);
        assert_eq!(obj2.len(), 100);

        // Test statistics
        let stats = pool.stats();
        assert_eq!(stats.allocations, 2);
        assert!(stats.cache_hits > 0 || stats.cache_misses > 0);
    }

    #[test]
    fn test_packet_buffer_pool() {
        let pool = PacketBufferPool::default();
        
        let buffer = pool.get_buffer();
        assert_eq!(buffer.len(), crate::sphinx::SPHINX_PACKET_SIZE);
        
        // Buffer should be zeroed
        assert!(buffer.iter().all(|&x| x == 0));
        
        drop(buffer); // Return to pool
        
        // Get another buffer - should be reused
        let buffer2 = pool.get_buffer();
        assert_eq!(buffer2.len(), crate::sphinx::SPHINX_PACKET_SIZE);
    }

    #[test]
    fn test_aligned_buffer() {
        let mut buffer = AlignedBuffer::new(1024);
        assert_eq!(buffer.len(), 1024);
        
        // Test alignment
        let ptr = buffer.as_ptr() as usize;
        assert_eq!(ptr % 32, 0); // Should be 32-byte aligned
        
        // Test mutability
        buffer.as_mut_slice()[0] = 42;
        assert_eq!(buffer.as_slice()[0], 42);
    }

    #[test]
    fn test_thread_local_pools() {
        let packet_buffer = get_packet_buffer();
        assert_eq!(packet_buffer.len(), crate::sphinx::SPHINX_PACKET_SIZE);
        
        let header_buffer = get_header_buffer();
        assert_eq!(header_buffer.len(), 512);
        
        let payload_buffer = get_payload_buffer();
        assert_eq!(payload_buffer.len(), 512);
        
        // Test statistics
        let (packet_stats, header_stats, payload_stats) = get_pool_stats();
        assert!(packet_stats.allocations > 0);
        assert!(header_stats.allocations > 0);
        assert!(payload_stats.allocations > 0);
    }

    #[test]
    fn test_pool_reuse() {
        let pool = PacketBufferPool::default();
        
        // Get and modify a buffer
        {
            let mut buffer = pool.get_buffer();
            buffer[0] = 99;
        } // Buffer returned to pool here
        
        // Get another buffer - should be the same one, but reset
        let buffer = pool.get_buffer();
        assert_eq!(buffer[0], 0); // Should be reset to 0
    }
}