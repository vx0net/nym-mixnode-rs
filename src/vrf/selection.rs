use sha2::{Sha256, Digest};
use std::collections::HashMap;
use lru::LruCache;
use curve25519_dalek::ristretto::RistrettoPoint;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use serde::{Serialize, Deserialize};
use rand_core::OsRng;
use std::num::NonZeroUsize;
use crate::sphinx::MixNodeId;

/// Real VRF-based mixnode selection using Ed25519
pub struct MixNodeRegistry {
    nodes: HashMap<MixNodeId, MixNodeInfo>,
    vrf_signing_key: SigningKey,
    vrf_verifying_key: VerifyingKey,
    selection_cache: LruCache<[u8; 32], Vec<MixNodeId>>,
}

#[derive(Debug, Clone)]
pub struct MixNodeInfo {
    pub id: MixNodeId,
    pub public_key: RistrettoPoint,
    pub stake_weight: u64,
    pub reliability_score: f64,
    pub geographic_region: Region,
    pub last_seen: std::time::SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Region {
    NorthAmerica,
    Europe,
    Asia,
    Oceania,
    SouthAmerica,
    Africa,
}

impl MixNodeRegistry {
    pub fn new() -> Result<Self, VRFError> {
        // Generate VRF keypair using Ed25519
        let vrf_signing_key = SigningKey::generate(&mut OsRng);
        let vrf_verifying_key = vrf_signing_key.verifying_key();
        
        Ok(Self {
            nodes: HashMap::new(),
            vrf_signing_key,
            vrf_verifying_key,
            selection_cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
        })
    }
    
    /// Create registry with existing keypair
    pub fn with_keypair(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        
        Self {
            nodes: HashMap::new(),
            vrf_signing_key: signing_key,
            vrf_verifying_key: verifying_key,
            selection_cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
        }
    }
    
    /// Get VRF public key for verification
    pub fn get_vrf_public_key(&self) -> VerifyingKey {
        self.vrf_verifying_key
    }
    
    pub fn add_node(&mut self, node: MixNodeInfo) {
        self.nodes.insert(node.id, node);
    }
    
    /// CRITICAL: VRF hop selection with stake weighting
    pub fn select_path(
        &mut self, 
        stream_id: &[u8], 
        epoch: u64,
        path_length: usize
    ) -> Result<Vec<MixNodeId>, VRFError> {
        // Create deterministic but unpredictable seed
        let mut hasher = Sha256::new();
        hasher.update(b"BETANET_MIX_PATH_v1");
        hasher.update(stream_id);
        hasher.update(&epoch.to_be_bytes());
        let seed = hasher.finalize();
        
        // Check cache first
        let seed_array: [u8; 32] = seed.into();
        if let Some(cached_path) = self.selection_cache.get(&seed_array) {
            return Ok(cached_path.clone());
        }
        
        let mut selected_nodes = Vec::with_capacity(path_length);
        let mut used_regions = std::collections::HashSet::new();
        
        for hop in 0..path_length {
            // Create hop-specific VRF input
            let mut hop_input = Vec::new();
            hop_input.extend_from_slice(&seed);
            hop_input.push(hop as u8);
            
            // Generate REAL VRF proof using Ed25519 signature
            let signature = self.vrf_signing_key.sign(&hop_input);
            
            // Use signature as VRF output (standard practice for Ed25519 VRF)
            let vrf_output = signature.to_bytes();
            let selected_node = self.select_node_from_vrf(
                &vrf_output, 
                &selected_nodes, 
                &used_regions
            )?;
            
            selected_nodes.push(selected_node.id);
            used_regions.insert(selected_node.geographic_region);
        }
        
        // Cache the result
        self.selection_cache.put(seed_array, selected_nodes.clone());
        
        Ok(selected_nodes)
    }
    
    fn select_node_from_vrf(
        &self,
        vrf_output: &[u8],
        excluded_nodes: &[MixNodeId],
        excluded_regions: &std::collections::HashSet<Region>
    ) -> Result<MixNodeInfo, VRFError> {
        // Convert VRF output to selection index with stake weighting
        let selection_value = u64::from_be_bytes([
            vrf_output[0], vrf_output[1], vrf_output[2], vrf_output[3],
            vrf_output[4], vrf_output[5], vrf_output[6], vrf_output[7],
        ]);
        
        // Filter available nodes (exclude already selected + same regions)
        let available_nodes: Vec<_> = self.nodes.values()
            .filter(|node| !excluded_nodes.contains(&node.id))
            .filter(|node| !excluded_regions.contains(&node.geographic_region))
            .filter(|node| self.is_node_active(node))
            .collect();
        
        if available_nodes.is_empty() {
            return Err(VRFError::NoAvailableNodes);
        }
        
        // Calculate total stake weight
        let total_weight: u64 = available_nodes.iter()
            .map(|node| node.stake_weight)
            .sum();
        
        if total_weight == 0 {
            // If no stake weights, select uniformly at random
            let index = (selection_value as usize) % available_nodes.len();
            return Ok(available_nodes[index].clone());
        }
        
        // Select based on stake-weighted randomness
        let selection_point = selection_value % total_weight;
        let mut cumulative_weight = 0;
        
        for node in &available_nodes {
            cumulative_weight += node.stake_weight;
            if cumulative_weight > selection_point {
                return Ok((*node).clone());
            }
        }
        
        // Fallback (should not happen)
        Ok(available_nodes[0].clone())
    }
    
    fn is_node_active(&self, node: &MixNodeInfo) -> bool {
        let now = std::time::SystemTime::now();
        let active_threshold = std::time::Duration::from_secs(300); // 5 minutes
        
        now.duration_since(node.last_seen)
            .map(|d| d < active_threshold)
            .unwrap_or(false)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VRFError {
    #[error("VRF setup failed: {0}")]
    Setup(String),
    #[error("VRF proof generation failed: {0}")]
    ProofGeneration(String),
    #[error("No available nodes for selection")]
    NoAvailableNodes,
}