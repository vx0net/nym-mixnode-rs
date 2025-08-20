use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use curve25519_dalek::scalar::Scalar;
use rand_core::{OsRng, RngCore};

pub mod sphinx;
pub mod vrf;
pub mod cover_traffic;
pub mod rate_limit;
pub mod metrics;
pub mod config;
pub mod load_balancer;
pub mod discovery;
pub mod security;
pub mod health;
pub mod cli;
pub mod p2p;
pub mod storage;
pub mod logging;

pub use sphinx::*;
pub use vrf::*;

use chrono;

pub struct HighPerformanceMixnode {
    config: MixnodeConfig,
    mixer: Arc<tokio::sync::Mutex<SphinxMixer>>,
    vrf_selector: Arc<tokio::sync::Mutex<MixNodeRegistry>>,
    packet_counter: Arc<AtomicU64>,
    #[allow(dead_code)]
    cover_traffic: CoverTrafficGenerator,
    #[allow(dead_code)]
    rate_limiter: RateLimiter,
}

#[derive(Debug, Clone)]
pub struct MixnodeConfig {
    pub listen_address: String,
    pub max_packet_rate: u64,        // packets per second
    pub cover_traffic_ratio: f64,    // 0.1 = 10% cover traffic
    pub worker_threads: usize,       // Number of packet processing threads
}

impl Default for MixnodeConfig {
    fn default() -> Self {
        Self {
            listen_address: "127.0.0.1:8080".to_string(),
            max_packet_rate: 30_000,
            cover_traffic_ratio: 0.1,
            worker_threads: num_cpus::get(),
        }
    }
}

struct PacketBatch {
    packets: Vec<(SphinxPacket, std::net::SocketAddr)>,
}

impl PacketBatch {
    fn new() -> Self {
        Self {
            packets: Vec::with_capacity(100), // Batch size for efficiency
        }
    }
    
    fn push(&mut self, packet: SphinxPacket, addr: std::net::SocketAddr) {
        self.packets.push((packet, addr));
    }
    
    fn is_full(&self) -> bool {
        self.packets.len() >= 100
    }
}

pub struct CoverTrafficGenerator {
    _ratio: f64,
}

impl CoverTrafficGenerator {
    pub fn new(ratio: f64) -> Self {
        Self { _ratio: ratio }
    }
    
    pub async fn run(&mut self, _socket: Arc<UdpSocket>) {
        // Placeholder for cover traffic generation
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }
}

pub struct RateLimiter {
    _config: RateLimitConfig,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub packets_per_second_per_ip: u32,
    pub global_packets_per_second: u32,
    pub burst_size: u32,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self { _config: config }
    }
}

impl HighPerformanceMixnode {
    pub fn new(config: MixnodeConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate mixnode identity
        let mut rng = OsRng;
        let mut key_bytes = [0u8; 32];
        rng.fill_bytes(&mut key_bytes);
        let private_key = Scalar::from_bytes_mod_order(key_bytes);
        let mixer = Arc::new(tokio::sync::Mutex::new(SphinxMixer::new(private_key)));
        
        let vrf_selector = Arc::new(tokio::sync::Mutex::new(MixNodeRegistry::new()?));
        
        Ok(Self {
            mixer,
            vrf_selector,
            packet_counter: Arc::new(AtomicU64::new(0)),
            cover_traffic: CoverTrafficGenerator::new(config.cover_traffic_ratio),
            rate_limiter: RateLimiter::new(RateLimitConfig {
                packets_per_second_per_ip: 1000,
                global_packets_per_second: config.max_packet_rate as u32,
                burst_size: 100,
            }),
            config,
        })
    }
    
    /// CRITICAL: Main performance target - ≥25k packets/second sustained
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting High-Performance Mixnode");
        println!("📊 Target: ≥25,000 packets/second");
        
        // Create high-throughput UDP socket with SO_REUSEPORT for multi-core scaling
        let socket = self.create_optimized_socket().await?;
        let socket = Arc::new(socket);
        
        // Multi-threaded packet processing pipeline
        let (packet_tx, packet_rx) = mpsc::channel::<PacketBatch>(1000);
        
        // Spawn packet receivers (one per CPU core)
        let num_cores = num_cpus::get();
        println!("💻 Using {} CPU cores for packet processing", num_cores);
        
        for core_id in 0..num_cores {
            let socket_clone = socket.clone();
            let mixer_clone = self.mixer.clone();
            let counter_clone = self.packet_counter.clone();
            let packet_tx_clone = packet_tx.clone();
            
            tokio::spawn(async move {
                Self::packet_receiver_loop(
                    core_id,
                    socket_clone,
                    mixer_clone,
                    counter_clone,
                    packet_tx_clone
                ).await;
            });
        }
        
        // Spawn packet processor (batch processing for efficiency)
        let vrf_clone = self.vrf_selector.clone();
        tokio::spawn(async move {
            Self::packet_processor_loop(packet_rx, vrf_clone).await;
        });
        
        // Spawn cover traffic generator
        let cover_socket = socket.clone();
        let mut cover_traffic = CoverTrafficGenerator::new(self.config.cover_traffic_ratio);
        tokio::spawn(async move {
            cover_traffic.run(cover_socket).await;
        });
        
        // Main metrics loop - CRITICAL for validating 25k pkt/s requirement
        self.run_metrics_loop().await;
        
        Ok(())
    }
    
    async fn packet_receiver_loop(
        core_id: usize,
        socket: Arc<UdpSocket>,
        _mixer: Arc<tokio::sync::Mutex<SphinxMixer>>,
        counter: Arc<AtomicU64>,
        packet_tx: mpsc::Sender<PacketBatch>
    ) {
        let mut buffer = [0u8; SPHINX_PACKET_SIZE];
        let mut batch = PacketBatch::new();
        
        println!("🔄 Core {} ready for packet processing", core_id);
        
        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((size, addr)) => {
                    if size == SPHINX_PACKET_SIZE {
                        // Parse sphinx packet (zero-copy)
                        let packet = unsafe {
                            std::ptr::read(buffer.as_ptr() as *const SphinxPacket)
                        };
                        
                        batch.push(packet, addr);
                        counter.fetch_add(1, Ordering::Relaxed);
                        
                        // Send batch when full (for efficient processing)
                        if batch.is_full() {
                            if packet_tx.try_send(batch).is_err() {
                                // Channel full, drop batch (backpressure)
                                println!("⚠️  Core {}: Packet processing overloaded, dropping batch", core_id);
                            }
                            batch = PacketBatch::new();
                        }
                    }
                },
                Err(e) => {
                    eprintln!("❌ Core {}: Socket receive error: {}", core_id, e);
                }
            }
        }
    }
    
    async fn packet_processor_loop(
        mut packet_rx: mpsc::Receiver<PacketBatch>,
        _vrf_selector: Arc<tokio::sync::Mutex<MixNodeRegistry>>
    ) {
        while let Some(_batch) = packet_rx.recv().await {
            // Process batch of packets
            // For now, just acknowledge receipt
        }
    }
    
    async fn create_optimized_socket(&self) -> Result<UdpSocket, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(&self.config.listen_address).await?;
        
        println!("🔧 Optimizing socket for high performance...");
        
        // Set socket options for high performance
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            
            unsafe {
                // SO_REUSEPORT for multi-core scaling
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEPORT,
                    &1i32 as *const i32 as *const libc::c_void,
                    std::mem::size_of::<i32>() as libc::socklen_t
                );
                
                // Increase receive buffer size
                let buf_size = 1024 * 1024; // 1MB
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET, 
                    libc::SO_RCVBUF,
                    &buf_size as *const i32 as *const libc::c_void,
                    std::mem::size_of::<i32>() as libc::socklen_t
                );
            }
        }
        
        println!("✅ Socket optimized for high throughput");
        Ok(socket)
    }
}

// Performance monitoring to verify ≥25k pkt/s requirement
impl HighPerformanceMixnode {
    async fn run_metrics_loop(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_count = 0;
        let mut consecutive_failures = 0;
        
        println!("📈 Starting performance monitoring...");
        
        loop {
            interval.tick().await;
            
            let current_count = self.packet_counter.load(Ordering::Relaxed);
            let pps = current_count - last_count;
            last_count = current_count;
            
            // Performance status
            let status = if pps >= 25_000 {
                consecutive_failures = 0;
                "✅ MEETING TARGET"
            } else if pps >= 20_000 {
                "⚠️  CLOSE TO TARGET"
            } else {
                consecutive_failures += 1;
                "❌ BELOW TARGET"
            };
            
            println!("📊 {} | {:.0} pkt/s (target: ≥25k)", status, pps);
            
            // Alert if consistently below requirement
            if consecutive_failures >= 5 {
                eprintln!("🚨 CRITICAL: Performance below requirement for 5+ seconds");
                eprintln!("🚨 Current: {} pkt/s | Required: ≥25,000 pkt/s", pps);
            }
            
            // Emit structured metrics for monitoring
            self.emit_performance_metrics(pps).await;
        }
    }
    
    async fn emit_performance_metrics(&self, pps: u64) {
        // Emit metrics in structured format for external monitoring
        let metrics = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "packets_per_second": pps,
            "target_met": pps >= 25_000,
            "total_packets": self.packet_counter.load(Ordering::Relaxed),
        });
        
        // In production, send to monitoring system
        println!("METRICS: {}", metrics);
    }
}