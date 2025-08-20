// Performance logging and profiling
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use serde::{Serialize, Deserialize};
use tracing::{info, debug, instrument};

use super::LoggingConfig;

/// Performance profiling checkpoint
#[derive(Debug, Clone, Serialize)]
struct ProfilingCheckpoint {
    name: String,
    #[serde(skip)]
    timestamp: Instant,
    duration_since_start: Duration,
    metadata: std::collections::HashMap<String, String>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    pub packets_processed: u64,
    pub avg_processing_time_ms: f64,
    pub throughput_pps: f64,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub error_count: u64,
    pub uptime_seconds: u64,
}

/// Performance logger
pub struct PerformanceLogger {
    config: LoggingConfig,
    metrics_history: Arc<RwLock<VecDeque<PerformanceMetrics>>>,
    checkpoints: Arc<RwLock<Vec<ProfilingCheckpoint>>>,
    start_time: Instant,
}

impl PerformanceLogger {
    pub fn new(config: LoggingConfig) -> Self {
        Self {
            config,
            metrics_history: Arc::new(RwLock::new(VecDeque::new())),
            checkpoints: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
        }
    }
    
    /// Start performance profiling session
    #[instrument(skip(self))]
    pub fn start_profiling(&self, session_name: &str) {
        let mut checkpoints = self.checkpoints.write().unwrap();
        checkpoints.clear();
        
        let checkpoint = ProfilingCheckpoint {
            name: format!("START: {}", session_name),
            timestamp: Instant::now(),
            duration_since_start: Duration::ZERO,
            metadata: std::collections::HashMap::new(),
        };
        
        checkpoints.push(checkpoint);
        debug!("Started profiling session: {}", session_name);
    }
    
    /// Add profiling checkpoint
    #[instrument(skip(self))]
    pub fn checkpoint(&self, name: &str, metadata: Option<std::collections::HashMap<String, String>>) {
        let now = Instant::now();
        let mut checkpoints = self.checkpoints.write().unwrap();
        
        let duration_since_start = if let Some(first) = checkpoints.first() {
            now.duration_since(first.timestamp)
        } else {
            Duration::ZERO
        };
        
        let checkpoint = ProfilingCheckpoint {
            name: name.to_string(),
            timestamp: now,
            duration_since_start,
            metadata: metadata.unwrap_or_default(),
        };
        
        checkpoints.push(checkpoint);
        debug!("Profiling checkpoint: {} (+{:?})", name, duration_since_start);
    }
    
    /// End profiling session and generate report
    #[instrument(skip(self))]
    pub fn end_profiling(&self) -> ProfilingReport {
        let checkpoints = self.checkpoints.read().unwrap().clone();
        
        let total_duration = if let (Some(first), Some(last)) = (checkpoints.first(), checkpoints.last()) {
            last.timestamp.duration_since(first.timestamp)
        } else {
            Duration::ZERO
        };
        
        let report = ProfilingReport {
            total_duration,
            checkpoints: checkpoints.clone(),
            checkpoint_count: checkpoints.len(),
        };
        
        info!("Profiling session completed: {} checkpoints, total duration: {:?}", 
              report.checkpoint_count, report.total_duration);
        
        report
    }
    
    /// Log performance metrics
    #[instrument(skip(self))]
    pub fn log_metrics(&self, metrics: PerformanceMetrics) {
        // Add to history
        {
            let mut history = self.metrics_history.write().unwrap();
            history.push_back(metrics.clone());
            
            // Keep only last 1000 entries
            if history.len() > 1000 {
                history.pop_front();
            }
        }
        
        // Log current metrics
        info!(
            packets_processed = metrics.packets_processed,
            avg_processing_time_ms = metrics.avg_processing_time_ms,
            throughput_pps = metrics.throughput_pps,
            cpu_usage_percent = metrics.cpu_usage_percent,
            memory_usage_mb = metrics.memory_usage_mb,
            error_count = metrics.error_count,
            uptime_seconds = metrics.uptime_seconds,
            "Performance metrics updated"
        );
    }
    
    /// Get metrics history
    pub fn get_metrics_history(&self) -> Vec<PerformanceMetrics> {
        self.metrics_history.read().unwrap().iter().cloned().collect()
    }
    
    /// Generate performance report
    #[instrument(skip(self))]
    pub fn generate_report(&self) -> PerformanceReport {
        let history = self.get_metrics_history();
        let uptime = self.start_time.elapsed();
        
        let avg_throughput = if !history.is_empty() {
            history.iter().map(|m| m.throughput_pps).sum::<f64>() / history.len() as f64
        } else {
            0.0
        };
        
        let avg_processing_time = if !history.is_empty() {
            history.iter().map(|m| m.avg_processing_time_ms).sum::<f64>() / history.len() as f64
        } else {
            0.0
        };
        
        let total_packets = history.last().map(|m| m.packets_processed).unwrap_or(0);
        let total_errors = history.last().map(|m| m.error_count).unwrap_or(0);
        
        PerformanceReport {
            uptime,
            total_packets_processed: total_packets,
            avg_throughput_pps: avg_throughput,
            avg_processing_time_ms: avg_processing_time,
            total_errors,
            metrics_count: history.len(),
        }
    }
}

/// Profiling report
#[derive(Debug, Clone, Serialize)]
pub struct ProfilingReport {
    pub total_duration: Duration,
    pub checkpoints: Vec<ProfilingCheckpoint>,
    pub checkpoint_count: usize,
}

/// Performance report
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceReport {
    pub uptime: Duration,
    pub total_packets_processed: u64,
    pub avg_throughput_pps: f64,
    pub avg_processing_time_ms: f64,
    pub total_errors: u64,
    pub metrics_count: usize,
}