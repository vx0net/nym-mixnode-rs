// OpenTelemetry integration for distributed tracing and metrics
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Interval;

/// OpenTelemetry-compatible metrics collector
pub struct TelemetryCollector {
    metrics: Arc<Mutex<TelemetryMetrics>>,
    config: TelemetryConfig,
    exporters: Vec<Box<dyn MetricExporter + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub service_version: String,
    pub export_interval: Duration,
    pub enable_traces: bool,
    pub enable_metrics: bool,
    pub enable_logs: bool,
    pub jaeger_endpoint: Option<String>,
    pub prometheus_endpoint: Option<String>,
    pub otel_collector_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "nym-mixnode".to_string(),
            service_version: "1.0.0".to_string(),
            export_interval: Duration::from_secs(10),
            enable_traces: true,
            enable_metrics: true,
            enable_logs: true,
            jaeger_endpoint: None,
            prometheus_endpoint: Some("http://localhost:9090".to_string()),
            otel_collector_endpoint: None,
        }
    }
}

/// Core telemetry metrics
#[derive(Debug, Clone, Default)]
pub struct TelemetryMetrics {
    // Performance metrics
    pub packets_processed_total: u64,
    pub packets_per_second: f64,
    pub processing_time_histogram: Vec<f64>,
    pub throughput_gauge: f64,
    
    // System metrics  
    pub cpu_usage: f64,
    pub memory_usage_bytes: u64,
    pub network_bytes_in: u64,
    pub network_bytes_out: u64,
    
    // Application metrics
    pub active_connections: u64,
    pub error_count: u64,
    pub uptime_seconds: u64,
    pub rate_limited_packets: u64,
    
    // Custom attributes
    pub attributes: HashMap<String, String>,
}

/// Trait for metric exporters (Prometheus, OTLP, etc.)
pub trait MetricExporter {
    fn export_metrics(&self, metrics: &TelemetryMetrics) -> Result<(), Box<dyn std::error::Error>>;
    fn name(&self) -> &str;
}

/// Prometheus exporter
pub struct PrometheusExporter {
    endpoint: String,
    client: reqwest::Client,
}

impl PrometheusExporter {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::Client::new(),
        }
    }

    fn format_prometheus_metrics(&self, metrics: &TelemetryMetrics) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        format!(
            r#"# HELP nym_packets_processed_total Total packets processed
# TYPE nym_packets_processed_total counter
nym_packets_processed_total {} {}

# HELP nym_packets_per_second Current packet processing rate
# TYPE nym_packets_per_second gauge
nym_packets_per_second {} {}

# HELP nym_processing_time_seconds Packet processing time histogram
# TYPE nym_processing_time_seconds histogram
nym_processing_time_seconds_bucket{{le="0.001"}} {} {}
nym_processing_time_seconds_bucket{{le="0.005"}} {} {}
nym_processing_time_seconds_bucket{{le="0.01"}} {} {}
nym_processing_time_seconds_bucket{{le="0.05"}} {} {}
nym_processing_time_seconds_bucket{{le="0.1"}} {} {}
nym_processing_time_seconds_bucket{{le="+Inf"}} {} {}

# HELP nym_cpu_usage_ratio Current CPU usage ratio
# TYPE nym_cpu_usage_ratio gauge
nym_cpu_usage_ratio {} {}

# HELP nym_memory_usage_bytes Current memory usage in bytes
# TYPE nym_memory_usage_bytes gauge
nym_memory_usage_bytes {} {}

# HELP nym_active_connections Current active connections
# TYPE nym_active_connections gauge
nym_active_connections {} {}

# HELP nym_error_count_total Total errors encountered
# TYPE nym_error_count_total counter
nym_error_count_total {} {}

# HELP nym_uptime_seconds Service uptime in seconds
# TYPE nym_uptime_seconds counter
nym_uptime_seconds {} {}

# HELP nym_rate_limited_packets_total Total rate-limited packets
# TYPE nym_rate_limited_packets_total counter
nym_rate_limited_packets_total {} {}
"#,
            metrics.packets_processed_total, timestamp,
            metrics.packets_per_second, timestamp,
            self.count_histogram_bucket(&metrics.processing_time_histogram, 0.001), timestamp,
            self.count_histogram_bucket(&metrics.processing_time_histogram, 0.005), timestamp,
            self.count_histogram_bucket(&metrics.processing_time_histogram, 0.01), timestamp,
            self.count_histogram_bucket(&metrics.processing_time_histogram, 0.05), timestamp,
            self.count_histogram_bucket(&metrics.processing_time_histogram, 0.1), timestamp,
            metrics.processing_time_histogram.len(), timestamp,
            metrics.cpu_usage, timestamp,
            metrics.memory_usage_bytes, timestamp,
            metrics.active_connections, timestamp,
            metrics.error_count, timestamp,
            metrics.uptime_seconds, timestamp,
            metrics.rate_limited_packets, timestamp
        )
    }

    fn count_histogram_bucket(&self, histogram: &[f64], le: f64) -> usize {
        histogram.iter().filter(|&&x| x <= le).count()
    }
}

impl MetricExporter for PrometheusExporter {
    fn export_metrics(&self, metrics: &TelemetryMetrics) -> Result<(), Box<dyn std::error::Error>> {
        let prometheus_format = self.format_prometheus_metrics(metrics);
        
        // In a real implementation, this would push to Prometheus Push Gateway
        // For now, we'll just log the metrics
        println!("Exporting to Prometheus: {} metrics", prometheus_format.lines().count());
        
        Ok(())
    }

    fn name(&self) -> &str {
        "prometheus"
    }
}

/// OTLP (OpenTelemetry Protocol) exporter
pub struct OtlpExporter {
    endpoint: String,
    client: reqwest::Client,
}

impl OtlpExporter {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::Client::new(),
        }
    }
}

impl MetricExporter for OtlpExporter {
    fn export_metrics(&self, metrics: &TelemetryMetrics) -> Result<(), Box<dyn std::error::Error>> {
        // Convert to OTLP format (JSON representation)
        let otlp_metrics = serde_json::json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "nym-mixnode"}},
                        {"key": "service.version", "value": {"stringValue": "1.0.0"}}
                    ]
                },
                "scopeMetrics": [{
                    "scope": {"name": "nym.mixnode.metrics"},
                    "metrics": [
                        {
                            "name": "nym.packets.processed",
                            "description": "Total packets processed",
                            "unit": "1",
                            "sum": {
                                "dataPoints": [{
                                    "timeUnixNano": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
                                    "asInt": metrics.packets_processed_total
                                }]
                            }
                        },
                        {
                            "name": "nym.packets.rate",
                            "description": "Packets per second",
                            "unit": "pkt/s",
                            "gauge": {
                                "dataPoints": [{
                                    "timeUnixNano": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
                                    "asDouble": metrics.packets_per_second
                                }]
                            }
                        }
                    ]
                }]
            }]
        });

        println!("Exporting to OTLP endpoint: {} bytes", otlp_metrics.to_string().len());
        Ok(())
    }

    fn name(&self) -> &str {
        "otlp"
    }
}

impl TelemetryCollector {
    pub fn new(config: TelemetryConfig) -> Self {
        let mut exporters: Vec<Box<dyn MetricExporter + Send + Sync>> = Vec::new();

        // Add Prometheus exporter if configured
        if let Some(prometheus_endpoint) = &config.prometheus_endpoint {
            exporters.push(Box::new(PrometheusExporter::new(prometheus_endpoint.clone())));
        }

        // Add OTLP exporter if configured
        if let Some(otlp_endpoint) = &config.otel_collector_endpoint {
            exporters.push(Box::new(OtlpExporter::new(otlp_endpoint.clone())));
        }

        Self {
            metrics: Arc::new(Mutex::new(TelemetryMetrics::default())),
            config,
            exporters,
        }
    }

    /// Record packet processing metrics
    pub fn record_packet_processed(&self, processing_time: Duration) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.packets_processed_total += 1;
        metrics.processing_time_histogram.push(processing_time.as_secs_f64());
        
        // Keep histogram size bounded
        if metrics.processing_time_histogram.len() > 10000 {
            metrics.processing_time_histogram.drain(0..5000);
        }
    }

    /// Update throughput metrics
    pub fn update_throughput(&self, packets_per_second: f64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.packets_per_second = packets_per_second;
        metrics.throughput_gauge = packets_per_second;
    }

    /// Record system metrics
    pub fn record_system_metrics(&self, cpu_usage: f64, memory_bytes: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.cpu_usage = cpu_usage;
        metrics.memory_usage_bytes = memory_bytes;
    }

    /// Record network metrics
    pub fn record_network_metrics(&self, bytes_in: u64, bytes_out: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.network_bytes_in += bytes_in;
        metrics.network_bytes_out += bytes_out;
    }

    /// Record application metrics
    pub fn record_app_metrics(&self, active_connections: u64, errors: u64, rate_limited: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.active_connections = active_connections;
        metrics.error_count += errors;
        metrics.rate_limited_packets += rate_limited;
    }

    /// Update uptime
    pub fn update_uptime(&self, uptime_seconds: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.uptime_seconds = uptime_seconds;
    }

    /// Record operation duration
    pub fn record_operation_duration(&self, operation: &str, duration_ms: f64, success: bool) {
        // For now, just log the operation
        // In a full implementation, this would record to structured metrics
        if success {
            println!("TELEMETRY: {} completed in {:.2}ms", operation, duration_ms);
        } else {
            println!("TELEMETRY: {} failed after {:.2}ms", operation, duration_ms);
        }
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> TelemetryMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Start automatic metric export
    pub async fn start_exporter(&self) {
        let mut interval = tokio::time::interval(self.config.export_interval);
        let metrics = self.metrics.clone();
        let exporters = &self.exporters;

        loop {
            interval.tick().await;
            
            let current_metrics = metrics.lock().unwrap().clone();
            
            for exporter in exporters {
                if let Err(e) = exporter.export_metrics(&current_metrics) {
                    eprintln!("Failed to export metrics via {}: {}", exporter.name(), e);
                }
            }
        }
    }

    /// Create span for distributed tracing
    pub fn create_span(&self, name: &str) -> TelemetrySpan {
        TelemetrySpan::new(name.to_string())
    }
}

/// Distributed tracing span
pub struct TelemetrySpan {
    name: String,
    start_time: SystemTime,
    attributes: HashMap<String, String>,
}

impl TelemetrySpan {
    pub fn new(name: String) -> Self {
        Self {
            name,
            start_time: SystemTime::now(),
            attributes: HashMap::new(),
        }
    }

    /// Add attribute to span
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attributes.insert(key.to_string(), value.to_string());
    }

    /// End span and record duration
    pub fn end(self) -> Duration {
        let duration = self.start_time.elapsed().unwrap_or_default();
        println!("Span '{}' completed in {:?} with attributes: {:?}", 
                 self.name, duration, self.attributes);
        duration
    }
}

/// System metrics collector
pub struct SystemMetricsCollector;

impl SystemMetricsCollector {
    /// Collect CPU usage (simplified implementation)
    pub fn get_cpu_usage() -> f64 {
        // In a real implementation, this would read from /proc/stat or use a system library
        // For now, return a simulated value
        use std::process::Command;
        
        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("top -l 1 -n 0 | grep 'CPU usage' | awk '{print $3}' | sed 's/%//'")
            .output()
        {
            if let Ok(cpu_str) = String::from_utf8(output.stdout) {
                if let Ok(cpu_val) = cpu_str.trim().parse::<f64>() {
                    return cpu_val / 100.0;
                }
            }
        }
        
        0.0 // Fallback
    }

    /// Collect memory usage
    pub fn get_memory_usage() -> u64 {
        // In a real implementation, this would read from /proc/meminfo
        use std::process::Command;
        
        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg("ps -o rss= -p $$ | awk '{print $1 * 1024}'")
            .output()
        {
            if let Ok(mem_str) = String::from_utf8(output.stdout) {
                if let Ok(mem_val) = mem_str.trim().parse::<u64>() {
                    return mem_val;
                }
            }
        }
        
        1024 * 1024 * 100 // 100MB fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_telemetry_collector() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config);

        // Test recording metrics
        collector.record_packet_processed(Duration::from_micros(42));
        collector.update_throughput(25000.0);
        collector.record_system_metrics(0.75, 1024 * 1024 * 512);

        let metrics = collector.get_metrics();
        assert_eq!(metrics.packets_processed_total, 1);
        assert_eq!(metrics.packets_per_second, 25000.0);
        assert_eq!(metrics.cpu_usage, 0.75);
    }

    #[test]
    fn test_prometheus_exporter() {
        let exporter = PrometheusExporter::new("http://localhost:9090".to_string());
        let metrics = TelemetryMetrics {
            packets_processed_total: 1000,
            packets_per_second: 25000.0,
            processing_time_histogram: vec![0.001, 0.002, 0.0015, 0.003],
            ..Default::default()
        };

        assert!(exporter.export_metrics(&metrics).is_ok());
        assert_eq!(exporter.name(), "prometheus");
    }

    #[test]
    fn test_telemetry_span() {
        let mut span = TelemetrySpan::new("test_span".to_string());
        span.set_attribute("component", "sphinx_processor");
        span.set_attribute("operation", "decrypt_header");

        let duration = span.end();
        assert!(duration.as_nanos() > 0);
    }
}