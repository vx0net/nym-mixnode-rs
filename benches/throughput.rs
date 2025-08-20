use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use nym_mixnode_rs::{SphinxMixer, SphinxPacket, SphinxHeader, MixNodeRegistry, MixNodeInfo, Region};
use curve25519_dalek::{scalar::Scalar, ristretto::RistrettoPoint};
use std::time::Duration;
use rand_core::{OsRng, RngCore};

const SPHINX_HEADER_SIZE: usize = 512;
const SPHINX_PAYLOAD_SIZE: usize = 512;

fn bench_sphinx_processing(c: &mut Criterion) {
    let mut rng = OsRng;
    let mut key_bytes = [0u8; 32];
    rng.fill_bytes(&mut key_bytes);
    let private_key = Scalar::from_bytes_mod_order(key_bytes);
    let mut mixer = SphinxMixer::new(private_key);
    
    // Create test packet
    let test_packet = create_test_packet();
    
    let mut group = c.benchmark_group("sphinx_processing");
    
    // CRITICAL: Benchmark single packet processing time
    group.bench_function("single_packet", |b| {
        b.iter(|| {
            black_box(mixer.process_packet(&test_packet).unwrap());
        });
    });
    
    // CRITICAL: Benchmark batch processing for 25k pkt/s validation
    group.throughput(Throughput::Elements(1000));
    group.bench_function("batch_1000", |b| {
        let packets: Vec<_> = (0..1000).map(|_| create_test_packet()).collect();
        
        b.iter(|| {
            for packet in &packets {
                black_box(mixer.process_packet(packet).unwrap());
            }
        });
    });
    
    group.finish();
}

fn bench_vrf_selection(c: &mut Criterion) {
    let mut registry = MixNodeRegistry::new().unwrap();
    
    // Add test nodes
    for i in 0..1000 {
        registry.add_node(create_test_node(i));
    }
    
    let mut group = c.benchmark_group("vrf_selection");
    
    group.bench_function("path_selection", |b| {
        let stream_id = b"test_stream_12345";
        let epoch = 1000;
        
        b.iter(|| {
            black_box(registry.select_path(stream_id, epoch, 3).unwrap());
        });
    });
    
    group.finish();
}

// CRITICAL: Validate 25k pkt/s requirement
fn bench_full_throughput(c: &mut Criterion) {
    let mut rng = OsRng;
    let mut key_bytes = [0u8; 32];
    rng.fill_bytes(&mut key_bytes);
    let private_key = Scalar::from_bytes_mod_order(key_bytes);
    let mut mixer = SphinxMixer::new(private_key);
    
    let mut group = c.benchmark_group("full_throughput");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);
    
    // Test processing rate to validate 25k pkt/s target
    group.throughput(Throughput::Elements(25_000));
    group.bench_function("25k_packets", |b| {
        let packets: Vec<_> = (0..25_000).map(|_| create_test_packet()).collect();
        
        b.iter(|| {
            for packet in &packets {
                black_box(mixer.process_packet(packet).unwrap());
            }
        });
    });
    
    group.finish();
}

fn bench_constant_time_validation(c: &mut Criterion) {
    let mut rng = OsRng;
    let mut key_bytes = [0u8; 32];
    rng.fill_bytes(&mut key_bytes);
    let private_key = Scalar::from_bytes_mod_order(key_bytes);
    let mut mixer = SphinxMixer::new(private_key);
    
    let mut group = c.benchmark_group("constant_time");
    
    // Test that processing time is constant regardless of packet content
    group.bench_function("valid_packet", |b| {
        let packet = create_test_packet();
        b.iter(|| {
            black_box(mixer.process_packet(&packet).unwrap());
        });
    });
    
    group.bench_function("invalid_packet", |b| {
        let packet = create_invalid_packet();
        b.iter(|| {
            let _ = black_box(mixer.process_packet(&packet));
        });
    });
    
    group.finish();
}

fn create_test_packet() -> SphinxPacket {
    // Create realistic test packet
    SphinxPacket {
        header: SphinxHeader {
            ephemeral_key: curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT,
            routing_info: [0u8; SPHINX_HEADER_SIZE - 32],
        },
        payload: [0u8; SPHINX_PAYLOAD_SIZE],
    }
}

fn create_invalid_packet() -> SphinxPacket {
    // Create packet with invalid data for constant-time testing
    SphinxPacket {
        header: SphinxHeader {
            ephemeral_key: curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT,
            routing_info: [0xFFu8; SPHINX_HEADER_SIZE - 32],
        },
        payload: [0xFFu8; SPHINX_PAYLOAD_SIZE],
    }
}

fn create_test_node(id: u32) -> MixNodeInfo {
    let mut node_id = [0u8; 32];
    node_id[0..4].copy_from_slice(&id.to_be_bytes());
    
    MixNodeInfo {
        id: node_id,
        public_key: &Scalar::from_bytes_mod_order([1u8; 32]) * curve25519_dalek::constants::RISTRETTO_BASEPOINT_TABLE,
        stake_weight: 1000 + (id % 5000) as u64,
        reliability_score: 0.95,
        geographic_region: match id % 6 {
            0 => Region::NorthAmerica,
            1 => Region::Europe,
            2 => Region::Asia,
            3 => Region::Oceania,
            4 => Region::SouthAmerica,
            _ => Region::Africa,
        },
        last_seen: std::time::SystemTime::now(),
    }
}

criterion_group!(benches, bench_sphinx_processing, bench_vrf_selection, bench_full_throughput, bench_constant_time_validation);
criterion_main!(benches);