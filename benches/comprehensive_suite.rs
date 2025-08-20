// Comprehensive benchmarking suite for Nym Mixnode
// Performance regression testing and optimization validation

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput, 
    black_box, measurement::WallTime, BatchSize
};
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;
use nym_mixnode_rs::{
    sphinx::{SphinxProcessor, SphinxPacket, SphinxHeader},
    vrf::selection::VRFSelector,
    crypto::simd::SIMDKeyDeriver,
    rate_limit::RateLimiter,
    load_balancer::LoadBalancer,
    discovery::NodeDiscovery,
    metrics::collector::MetricsCollector,
};

/// Benchmark configuration
#[derive(Clone)]
struct BenchmarkConfig {
    target_pps: u32,
    packet_sizes: Vec<usize>,
    batch_sizes: Vec<usize>,
    concurrent_connections: Vec<usize>,
    test_duration: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            target_pps: 25000,
            packet_sizes: vec![512, 1024, 2048, 4096],
            batch_sizes: vec![1, 10, 100, 1000],
            concurrent_connections: vec![1, 10, 50, 100, 500],
            test_duration: Duration::from_secs(10),
        }
    }
}

/// Performance baselines for regression testing
struct PerformanceBaselines {
    sphinx_processing_us: f64,        // Target: ≤40µs
    vrf_selection_ns: f64,            // Target: ≤1000ns
    throughput_pps: f64,              // Target: ≥25000
    memory_usage_mb: f64,             // Target: ≤512MB
    cpu_utilization: f64,             // Target: ≤80%
    latency_p99_us: f64,              // Target: ≤100µs
}

impl Default for PerformanceBaselines {
    fn default() -> Self {
        Self {
            sphinx_processing_us: 40.0,
            vrf_selection_ns: 1000.0,
            throughput_pps: 25000.0,
            memory_usage_mb: 512.0,
            cpu_utilization: 80.0,
            latency_p99_us: 100.0,
        }
    }
}

/// Benchmark suite for core Sphinx packet processing
fn bench_sphinx_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::default();
    
    let mut group = c.benchmark_group("sphinx_processing");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(1000);
    
    // Single packet processing benchmark
    group.bench_function("single_packet", |b| {
        rt.block_on(async {
            let mut processor = SphinxProcessor::new().unwrap();
            let packet = create_test_packet(1024);
            
            b.iter(|| {
                black_box(processor.process_packet(&packet))
            });
        });
    });
    
    // Batch processing benchmarks
    for &batch_size in &config.batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_processing", batch_size),
            &batch_size,
            |b, &batch_size| {
                rt.block_on(async {
                    let mut processor = SphinxProcessor::new().unwrap();
                    let packets: Vec<_> = (0..batch_size)
                        .map(|_| create_test_packet(1024))
                        .collect();
                    
                    b.iter_batched(
                        || packets.clone(),
                        |packets| {
                            for packet in packets {
                                black_box(processor.process_packet(&packet));
                            }
                        },
                        BatchSize::SmallInput
                    );
                });
            }
        );
    }
    
    // Variable packet size benchmarks
    for &packet_size in &config.packet_sizes {
        group.throughput(Throughput::Bytes(packet_size as u64));
        group.bench_with_input(
            BenchmarkId::new("variable_size", packet_size),
            &packet_size,
            |b, &packet_size| {
                rt.block_on(async {
                    let mut processor = SphinxProcessor::new().unwrap();
                    let packet = create_test_packet(packet_size);
                    
                    b.iter(|| {
                        black_box(processor.process_packet(&packet))
                    });
                });
            }
        );
    }
    
    group.finish();
}

/// Benchmark suite for VRF-based path selection
fn bench_vrf_selection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("vrf_selection");
    group.measurement_time(Duration::from_secs(15));
    
    // Single path selection
    group.bench_function("single_selection", |b| {
        rt.block_on(async {
            let selector = VRFSelector::new().unwrap();
            let candidates = create_test_nodes(100);
            
            b.iter(|| {
                black_box(selector.select_path(&candidates, 3))
            });
        });
    });
    
    // Variable candidate pool sizes
    for candidate_count in [10, 50, 100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::new("candidate_pool", candidate_count),
            &candidate_count,
            |b, &candidate_count| {
                rt.block_on(async {
                    let selector = VRFSelector::new().unwrap();
                    let candidates = create_test_nodes(candidate_count);
                    
                    b.iter(|| {
                        black_box(selector.select_path(&candidates, 3))
                    });
                });
            }
        );
    }
    
    group.finish();
}

/// Benchmark suite for SIMD optimizations
fn bench_simd_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_operations");
    group.measurement_time(Duration::from_secs(20));
    
    // Key derivation with SIMD
    group.bench_function("simd_key_derivation", |b| {
        let deriver = SIMDKeyDeriver::new();
        let shared_secret = [0u8; 32];
        
        b.iter(|| {
            black_box(deriver.derive_key(&shared_secret))
        });
    });
    
    // Batch key derivation
    for batch_size in [1, 10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_key_derivation", batch_size),
            &batch_size,
            |b, &batch_size| {
                let deriver = SIMDKeyDeriver::new();
                let secrets: Vec<[u8; 32]> = (0..batch_size)
                    .map(|i| {
                        let mut secret = [0u8; 32];
                        secret[0] = i as u8;
                        secret
                    })
                    .collect();
                
                b.iter_batched(
                    || secrets.clone(),
                    |secrets| {
                        for secret in secrets {
                            black_box(deriver.derive_key(&secret));
                        }
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }
    
    group.finish();
}

/// Benchmark suite for load balancing performance
fn bench_load_balancing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("load_balancing");
    group.measurement_time(Duration::from_secs(25));
    
    // Node selection performance
    group.bench_function("node_selection", |b| {
        rt.block_on(async {
            let metrics = Arc::new(MetricsCollector::new());
            let (load_balancer, _) = LoadBalancer::new(Default::default(), metrics);
            
            // Add test nodes
            for i in 0..100 {
                let addr = format!("192.168.1.{}:1789", i + 1).parse().unwrap();
                let _ = load_balancer.add_node(format!("node-{}", i), addr, Some(1.0)).await;
            }
            
            b.iter(|| {
                rt.block_on(async {
                    black_box(load_balancer.select_node(Some("192.168.1.100")).await)
                })
            });
        });
    });
    
    // Concurrent node selection
    for &concurrency in &[1, 10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_selection", concurrency),
            &concurrency,
            |b, &concurrency| {
                rt.block_on(async {
                    let metrics = Arc::new(MetricsCollector::new());
                    let (load_balancer, _) = LoadBalancer::new(Default::default(), metrics);
                    
                    // Add test nodes
                    for i in 0..100 {
                        let addr = format!("192.168.1.{}:1789", i + 1).parse().unwrap();
                        let _ = load_balancer.add_node(format!("node-{}", i), addr, Some(1.0)).await;
                    }
                    
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::new();
                            
                            for _ in 0..concurrency {
                                let lb = &load_balancer;
                                let handle = tokio::spawn(async move {
                                    black_box(lb.select_node(Some("192.168.1.100")).await)
                                });
                                handles.push(handle);
                            }
                            
                            for handle in handles {
                                let _ = handle.await;
                            }
                        });
                    });
                });
            }
        );
    }
    
    group.finish();
}

/// Benchmark suite for rate limiting
fn bench_rate_limiting(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("rate_limiting");
    group.measurement_time(Duration::from_secs(15));
    
    // Rate limit checking
    group.bench_function("rate_limit_check", |b| {
        rt.block_on(async {
            let rate_limiter = RateLimiter::new(1000, 100).unwrap();
            let ip = "192.168.1.100".parse().unwrap();
            
            b.iter(|| {
                black_box(rate_limiter.check_rate_limit(&ip))
            });
        });
    });
    
    // Concurrent rate limiting
    for &connections in &[10, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_checks", connections),
            &connections,
            |b, &connections| {
                rt.block_on(async {
                    let rate_limiter = RateLimiter::new(10000, 1000).unwrap();
                    
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::new();
                            
                            for i in 0..connections {
                                let ip = format!("192.168.1.{}", (i % 254) + 1)
                                    .parse().unwrap();
                                let rl = &rate_limiter;
                                
                                let handle = tokio::spawn(async move {
                                    black_box(rl.check_rate_limit(&ip))
                                });
                                handles.push(handle);
                            }
                            
                            for handle in handles {
                                let _ = handle.await;
                            }
                        });
                    });
                });
            }
        );
    }
    
    group.finish();
}

/// Comprehensive throughput benchmark
fn bench_full_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::default();
    
    let mut group = c.benchmark_group("full_throughput");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(50);
    
    // Target throughput test
    group.throughput(Throughput::Elements(config.target_pps as u64));
    group.bench_function("target_25k_pps", |b| {
        rt.block_on(async {
            let mut processor = SphinxProcessor::new().unwrap();
            let selector = VRFSelector::new().unwrap();
            let rate_limiter = RateLimiter::new(30000, 1000).unwrap();
            
            // Pre-generate test data
            let packets: Vec<_> = (0..1000)
                .map(|_| create_test_packet(1024))
                .collect();
            let nodes = create_test_nodes(100);
            
            b.iter(|| {
                rt.block_on(async {
                    // Process 1000 packets to simulate sustained load
                    for packet in &packets {
                        black_box(processor.process_packet(packet));
                        black_box(selector.select_path(&nodes, 3));
                        black_box(rate_limiter.check_rate_limit(&"192.168.1.1".parse().unwrap()));
                    }
                });
            });
        });
    });
    
    // Stress test with higher loads
    for multiplier in [1.5, 2.0, 3.0] {
        let target_pps = (config.target_pps as f64 * multiplier) as u32;
        
        group.throughput(Throughput::Elements(target_pps as u64));
        group.bench_with_input(
            BenchmarkId::new("stress_test", target_pps),
            &target_pps,
            |b, &target_pps| {
                rt.block_on(async {
                    let mut processor = SphinxProcessor::new().unwrap();
                    let packets: Vec<_> = (0..(target_pps / 10))
                        .map(|_| create_test_packet(1024))
                        .collect();
                    
                    b.iter(|| {
                        rt.block_on(async {
                            for packet in &packets {
                                black_box(processor.process_packet(packet));
                            }
                        });
                    });
                });
            }
        );
    }
    
    group.finish();
}

/// Memory allocation and usage benchmarks
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_usage");
    group.measurement_time(Duration::from_secs(20));
    
    // Memory pool allocation
    group.bench_function("memory_pool_allocation", |b| {
        rt.block_on(async {
            let mut processor = SphinxProcessor::new().unwrap();
            
            b.iter(|| {
                // Allocate and process packets to test memory pools
                let packets: Vec<_> = (0..100)
                    .map(|_| create_test_packet(1024))
                    .collect();
                
                for packet in packets {
                    black_box(processor.process_packet(&packet));
                }
            });
        });
    });
    
    // Large batch processing for memory stress testing
    for batch_size in [1000, 5000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("large_batch", batch_size),
            &batch_size,
            |b, &batch_size| {
                rt.block_on(async {
                    let mut processor = SphinxProcessor::new().unwrap();
                    
                    b.iter_batched(
                        || {
                            (0..batch_size)
                                .map(|_| create_test_packet(1024))
                                .collect::<Vec<_>>()
                        },
                        |packets| {
                            for packet in packets {
                                black_box(processor.process_packet(&packet));
                            }
                        },
                        BatchSize::LargeInput
                    );
                });
            }
        );
    }
    
    group.finish();
}

/// Network discovery benchmarks
fn bench_node_discovery(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("node_discovery");
    group.measurement_time(Duration::from_secs(30));
    
    // Node discovery performance
    group.bench_function("discover_nodes", |b| {
        rt.block_on(async {
            let metrics = Arc::new(MetricsCollector::new());
            let (discovery, _) = NodeDiscovery::new(Default::default(), metrics);
            
            b.iter(|| {
                rt.block_on(async {
                    black_box(discovery.discover_nodes().await)
                });
            });
        });
    });
    
    group.finish();
}

/// Regression test to ensure performance doesn't degrade
fn bench_regression_tests(c: &mut Criterion) {
    let baselines = PerformanceBaselines::default();
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("regression_tests");
    group.measurement_time(Duration::from_secs(45));
    
    // Test that single packet processing stays under baseline
    group.bench_function("regression_sphinx_processing", |b| {
        rt.block_on(async {
            let mut processor = SphinxProcessor::new().unwrap();
            let packet = create_test_packet(1024);
            
            let mut times = Vec::new();
            
            b.iter(|| {
                let start = std::time::Instant::now();
                black_box(processor.process_packet(&packet));
                times.push(start.elapsed().as_micros() as f64);
            });
            
            // Check if we're within baseline
            let avg_time = times.iter().sum::<f64>() / times.len() as f64;
            println!("Average processing time: {:.2}µs (baseline: {:.2}µs)", 
                    avg_time, baselines.sphinx_processing_us);
            
            assert!(avg_time <= baselines.sphinx_processing_us * 1.1, 
                   "Performance regression detected: {:.2}µs > {:.2}µs", 
                   avg_time, baselines.sphinx_processing_us);
        });
    });
    
    // Test VRF selection performance
    group.bench_function("regression_vrf_selection", |b| {
        rt.block_on(async {
            let selector = VRFSelector::new().unwrap();
            let candidates = create_test_nodes(100);
            
            let mut times = Vec::new();
            
            b.iter(|| {
                let start = std::time::Instant::now();
                black_box(selector.select_path(&candidates, 3));
                times.push(start.elapsed().as_nanos() as f64);
            });
            
            let avg_time = times.iter().sum::<f64>() / times.len() as f64;
            println!("Average VRF selection time: {:.2}ns (baseline: {:.2}ns)", 
                    avg_time, baselines.vrf_selection_ns);
            
            assert!(avg_time <= baselines.vrf_selection_ns * 1.2, 
                   "VRF performance regression detected: {:.2}ns > {:.2}ns", 
                   avg_time, baselines.vrf_selection_ns);
        });
    });
    
    group.finish();
}

// Helper functions for test data generation

fn create_test_packet(size: usize) -> SphinxPacket {
    let header = SphinxHeader {
        ephemeral_key: curve25519_dalek::ristretto::RistrettoPoint::default(),
        routing_info: vec![0u8; 160],
        header_mac: [0u8; 16],
    };
    
    SphinxPacket {
        header,
        payload: vec![0u8; size.saturating_sub(192)], // Account for header size
    }
}

fn create_test_nodes(count: usize) -> Vec<nym_mixnode_rs::discovery::NodeInfo> {
    (0..count)
        .map(|i| nym_mixnode_rs::discovery::NodeInfo {
            node_id: format!("test-node-{}", i),
            address: format!("192.168.1.{}:1789", (i % 254) + 1).parse().unwrap(),
            advertise_address: None,
            region: "test".to_string(),
            stake: 1000 + i as u64,
            capabilities: vec!["mixnode".to_string()],
            version: "1.0.0".to_string(),
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            performance_metrics: Default::default(),
            reputation: Default::default(),
            public_key: vec![0u8; 32],
            signature: vec![0u8; 64],
        })
        .collect()
}

// Benchmark groups
criterion_group!(
    benches,
    bench_sphinx_processing,
    bench_vrf_selection,
    bench_simd_operations,
    bench_load_balancing,
    bench_rate_limiting,
    bench_full_throughput,
    bench_memory_usage,
    bench_node_discovery,
    bench_regression_tests
);

criterion_main!(benches);
