// Cover traffic generation module
use tokio::net::UdpSocket;
use std::sync::Arc;
use rand_core::{OsRng, RngCore};

pub struct CoverTrafficGenerator {
    ratio: f64,
    config: CoverTrafficConfig,
}

#[derive(Debug, Clone)]
pub struct CoverTrafficConfig {
    pub min_interval_ms: u64,
    pub max_interval_ms: u64,
    pub cover_packet_ratio: f64, // 0.1 = 10% cover traffic
    pub burst_protection: bool,
}

impl Default for CoverTrafficConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: 100,
            max_interval_ms: 1000,
            cover_packet_ratio: 0.1,
            burst_protection: true,
        }
    }
}

impl CoverTrafficGenerator {
    pub fn new(ratio: f64) -> Self {
        let config = CoverTrafficConfig {
            cover_packet_ratio: ratio,
            ..Default::default()
        };
        
        Self { ratio, config }
    }
    
    /// Generate cover traffic to hide real traffic patterns
    pub async fn run(&mut self, socket: Arc<UdpSocket>) {
        let mut rng = OsRng;
        
        println!("🎭 Starting cover traffic generator ({}% ratio)", self.ratio * 100.0);
        
        loop {
            let interval = self.calculate_next_interval(&mut rng);
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;
            
            // Generate realistic cover packet
            let cover_packet = self.generate_cover_packet(&mut rng);
            let target_addr = self.select_random_peer(&mut rng);
            
            if let Err(e) = socket.send_to(&cover_packet, target_addr).await {
                eprintln!("Cover traffic send error: {}", e);
            }
        }
    }
    
    fn calculate_next_interval(&self, rng: &mut impl RngCore) -> u64 {
        let min = self.config.min_interval_ms;
        let max = self.config.max_interval_ms;
        
        if max <= min {
            return min;
        }
        
        let range = max - min;
        let random_offset = (rng.next_u64() % range) as u64;
        
        min + random_offset
    }
    
    fn generate_cover_packet(&self, rng: &mut impl RngCore) -> Vec<u8> {
        // Generate a packet that looks like a real Sphinx packet
        let packet_size = crate::sphinx::SPHINX_PACKET_SIZE;
        let mut packet = vec![0u8; packet_size];
        
        // Fill with realistic but random data
        rng.fill_bytes(&mut packet);
        
        // Make some parts look more structured like real packets
        // Set some realistic header patterns
        packet[0..4].copy_from_slice(&[0x42, 0x4E, 0x01, 0x00]); // "BN" + version
        
        // Add some structure to make it look like encrypted data
        for i in (32..64).step_by(8) {
            let value = rng.next_u64();
            if i + 8 <= packet.len() {
                packet[i..i + 8].copy_from_slice(&value.to_be_bytes());
            }
        }
        
        packet
    }
    
    fn select_random_peer(&self, rng: &mut impl RngCore) -> std::net::SocketAddr {
        // For testing, use a set of dummy addresses
        // In production, this would select from the actual peer list
        let test_addresses = [
            "127.0.0.1:8081",
            "127.0.0.1:8082", 
            "127.0.0.1:8083",
            "127.0.0.1:8084",
        ];
        
        let index = (rng.next_u32() as usize) % test_addresses.len();
        test_addresses[index].parse().unwrap_or_else(|_| "127.0.0.1:8080".parse().unwrap())
    }
}