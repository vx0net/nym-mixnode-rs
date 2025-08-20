// Comprehensive logging system for the Nym mixnode
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use chrono::{DateTime, Utc};

/// Logging configuration for the mixnode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub log_level: LogLevel,
    pub log_format: LogFormat,
    pub log_destinations: Vec<LogDestination>,
    pub structured_logging: bool,
    pub performance_logging: bool,
    pub security_logging: bool,
    pub audit_logging: bool,
    pub log_rotation: LogRotationConfig,
    pub sampling_config: LogSamplingConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
            log_format: LogFormat::Structured,
            log_destinations: vec![
                LogDestination::Console,
                LogDestination::File {
                    path: "./logs/mixnode.log".to_string(),
                    max_size_mb: 100,
                },
            ],
            structured_logging: true,
            performance_logging: true,
            security_logging: true,
            audit_logging: true,
            log_rotation: LogRotationConfig::default(),
            sampling_config: LogSamplingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Critical => "CRITICAL",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            LogLevel::Trace => "🔍",
            LogLevel::Debug => "🐛",
            LogLevel::Info => "ℹ️",
            LogLevel::Warn => "⚠️",
            LogLevel::Error => "❌",
            LogLevel::Critical => "🚨",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Human,      // Human-readable format
    Structured, // JSON structured logging
    Compact,    // Compact single-line format
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogDestination {
    Console,
    File {
        path: String,
        max_size_mb: u64,
    },
    Syslog {
        facility: String,
    },
    Network {
        endpoint: String,
        api_key: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    pub max_files: u32,
    pub rotation_size_mb: u64,
    pub rotation_interval_hours: u64,
    pub compression: bool,
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            max_files: 10,
            rotation_size_mb: 100,
            rotation_interval_hours: 24,
            compression: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSamplingConfig {
    pub enable_sampling: bool,
    pub sample_rate: f64, // 0.0 - 1.0
    pub high_volume_categories: HashMap<String, f64>,
}

impl Default for LogSamplingConfig {
    fn default() -> Self {
        let mut high_volume = HashMap::new();
        high_volume.insert("packet_processing".to_string(), 0.01); // 1% sampling
        high_volume.insert("cover_traffic".to_string(), 0.001); // 0.1% sampling
        
        Self {
            enable_sampling: true,
            sample_rate: 1.0, // Default: log everything
            high_volume_categories: high_volume,
        }
    }
}

/// Structured log entry
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub category: LogCategory,
    pub message: String,
    pub fields: HashMap<String, serde_json::Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub node_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogCategory {
    // Core system categories
    System,
    Config,
    Startup,
    Shutdown,
    
    // Networking categories
    Network,
    P2P,
    Transport,
    Connection,
    Discovery,
    
    // Mixnet categories
    Sphinx,
    Routing,
    PacketProcessing,
    CoverTraffic,
    VRF,
    
    // Security categories
    Security,
    Authentication,
    RateLimit,
    Intrusion,
    Audit,
    
    // Performance categories
    Performance,
    Metrics,
    Benchmark,
    Resource,
    
    // Observability
    Telemetry,
    Monitoring,
    Health,
    Alert,
}

impl LogCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogCategory::System => "system",
            LogCategory::Config => "config",
            LogCategory::Startup => "startup",
            LogCategory::Shutdown => "shutdown",
            LogCategory::Network => "network",
            LogCategory::P2P => "p2p",
            LogCategory::Transport => "transport",
            LogCategory::Connection => "connection",
            LogCategory::Discovery => "discovery",
            LogCategory::Sphinx => "sphinx",
            LogCategory::Routing => "routing",
            LogCategory::PacketProcessing => "packet_processing",
            LogCategory::CoverTraffic => "cover_traffic",
            LogCategory::VRF => "vrf",
            LogCategory::Security => "security",
            LogCategory::Authentication => "authentication",
            LogCategory::RateLimit => "rate_limit",
            LogCategory::Intrusion => "intrusion",
            LogCategory::Audit => "audit",
            LogCategory::Performance => "performance",
            LogCategory::Metrics => "metrics",
            LogCategory::Benchmark => "benchmark",
            LogCategory::Resource => "resource",
            LogCategory::Telemetry => "telemetry",
            LogCategory::Monitoring => "monitoring",
            LogCategory::Health => "health",
            LogCategory::Alert => "alert",
        }
    }
}

/// Main logging system
pub struct MixnodeLogger {
    config: LoggingConfig,
    writers: Arc<RwLock<HashMap<String, Box<dyn LogWriter + Send + Sync>>>>,
    log_sender: mpsc::UnboundedSender<LogEntry>,
    node_id: String,
    start_time: SystemTime,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

/// Trait for log writers
pub trait LogWriter {
    fn write_log(&mut self, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn should_rotate(&self) -> bool;
    fn rotate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Console log writer
pub struct ConsoleLogWriter {
    config: LoggingConfig,
}

impl ConsoleLogWriter {
    pub fn new(config: LoggingConfig) -> Self {
        Self { config }
    }
}

impl LogWriter for ConsoleLogWriter {
    fn write_log(&mut self, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let output = match self.config.log_format {
            LogFormat::Human => self.format_human(entry),
            LogFormat::Structured => self.format_structured(entry)?,
            LogFormat::Compact => self.format_compact(entry),
        };
        
        println!("{}", output);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        io::stdout().flush()?;
        Ok(())
    }

    fn should_rotate(&self) -> bool {
        false // Console doesn't rotate
    }

    fn rotate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(()) // No-op for console
    }
}

impl ConsoleLogWriter {
    fn format_human(&self, entry: &LogEntry) -> String {
        let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC");
        let level_emoji = entry.level.emoji();
        let level_str = entry.level.as_str();
        let category = entry.category.as_str();
        
        let mut output = format!(
            "{} {} [{}] [{}] {}",
            timestamp, level_emoji, level_str, category, entry.message
        );
        
        // Add important fields inline for human readability
        if !entry.fields.is_empty() {
            let key_fields: Vec<String> = entry.fields.iter()
                .filter(|(k, _)| {
                    matches!(k.as_str(), "peer_id" | "packet_id" | "latency_ms" | "throughput" | "error_code")
                })
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            
            if !key_fields.is_empty() {
                output.push_str(&format!(" [{}]", key_fields.join(" ")));
            }
        }
        
        output
    }

    fn format_structured(&self, entry: &LogEntry) -> Result<String, serde_json::Error> {
        serde_json::to_string(entry)
    }

    fn format_compact(&self, entry: &LogEntry) -> String {
        let timestamp = entry.timestamp.format("%H:%M:%S%.3f");
        format!(
            "{} {} [{}] {}",
            timestamp, entry.level.emoji(), entry.category.as_str(), entry.message
        )
    }
}

/// File log writer with rotation
pub struct FileLogWriter {
    file_path: PathBuf,
    current_file: Option<File>,
    current_size: u64,
    max_size: u64,
    config: LoggingConfig,
}

impl FileLogWriter {
    pub fn new(path: impl AsRef<Path>, max_size_mb: u64, config: LoggingConfig) -> Result<Self, io::Error> {
        let file_path = path.as_ref().to_path_buf();
        
        // Create directory if it doesn't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
            
        let current_size = file.metadata()?.len();
        
        Ok(Self {
            file_path,
            current_file: Some(file),
            current_size,
            max_size: max_size_mb * 1024 * 1024,
            config,
        })
    }
}

impl LogWriter for FileLogWriter {
    fn write_log(&mut self, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let output = match self.config.log_format {
            LogFormat::Structured => serde_json::to_string(entry)? + "\n",
            _ => {
                // For file logging, prefer structured format for better parsing
                serde_json::to_string(entry)? + "\n"
            }
        };
        
        if let Some(ref mut file) = self.current_file {
            file.write_all(output.as_bytes())?;
            self.current_size += output.len() as u64;
        }
        
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref mut file) = self.current_file {
            file.flush()?;
        }
        Ok(())
    }

    fn should_rotate(&self) -> bool {
        self.current_size >= self.max_size
    }

    fn rotate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(file) = self.current_file.take() {
            drop(file);
        }

        // Move current file to backup
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        let backup_path = self.file_path.with_extension(format!("log.{}", timestamp));
        std::fs::rename(&self.file_path, &backup_path)?;

        // Optionally compress the backup
        if self.config.log_rotation.compression {
            self.compress_log_file(&backup_path)?;
        }

        // Create new log file
        let new_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.file_path)?;
            
        self.current_file = Some(new_file);
        self.current_size = 0;

        // Clean up old log files
        self.cleanup_old_logs()?;

        Ok(())
    }
}

impl FileLogWriter {
    fn compress_log_file(&self, _path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Implement compression (e.g., using gzip)
        // For now, just keep uncompressed
        Ok(())
    }

    fn cleanup_old_logs(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let dir = self.file_path.parent().unwrap();
        let base_name = self.file_path.file_stem().unwrap().to_string_lossy();
        
        let mut log_files: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.file_stem()?.to_string_lossy().starts_with(&*base_name) {
                    Some((path, entry.metadata().ok()?.created().ok()?))
                } else {
                    None
                }
            })
            .collect();

        // Sort by creation time, newest first
        log_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove excess files
        let max_files = self.config.log_rotation.max_files as usize;
        for (path, _) in log_files.into_iter().skip(max_files) {
            let _ = std::fs::remove_file(path);
        }

        Ok(())
    }
}

impl MixnodeLogger {
    pub fn new(config: LoggingConfig, node_id: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (log_sender, log_receiver) = mpsc::unbounded_channel();
        let mut writers: HashMap<String, Box<dyn LogWriter + Send + Sync>> = HashMap::new();

        // Initialize log writers based on configuration
        for destination in &config.log_destinations {
            match destination {
                LogDestination::Console => {
                    writers.insert("console".to_string(), Box::new(ConsoleLogWriter::new(config.clone())));
                }
                LogDestination::File { path, max_size_mb } => {
                    let writer = FileLogWriter::new(path, *max_size_mb, config.clone())?;
                    writers.insert(format!("file:{}", path), Box::new(writer));
                }
                LogDestination::Syslog { .. } => {
                    // TODO: Implement syslog writer
                    eprintln!("Warning: Syslog logging not yet implemented");
                }
                LogDestination::Network { .. } => {
                    // TODO: Implement network writer
                    eprintln!("Warning: Network logging not yet implemented");
                }
            }
        }

        let logger = Self {
            config: config.clone(),
            writers: Arc::new(RwLock::new(writers)),
            log_sender,
            node_id: node_id.clone(),
            start_time: SystemTime::now(),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
        };

        // Start log processing task
        let writers_clone = logger.writers.clone();
        let shutdown_clone = logger.shutdown_signal.clone();
        let sampling_config = config.sampling_config.clone();
        
        tokio::spawn(async move {
            Self::log_processor_task(log_receiver, writers_clone, shutdown_clone, sampling_config).await;
        });

        Ok(logger)
    }

    /// Main logging function
    pub fn log(
        &self,
        level: LogLevel,
        category: LogCategory,
        message: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
        trace_id: Option<String>,
    ) {
        // Check if we should log this level
        if level < self.config.log_level {
            return;
        }

        // Apply sampling if configured
        if self.should_sample(category, level) {
            return;
        }

        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            category,
            message: message.into(),
            fields,
            trace_id,
            span_id: None, // TODO: Implement span tracking
            node_id: self.node_id.clone(),
        };

        // Send to async processor (non-blocking)
        let _ = self.log_sender.send(entry);
    }

    /// Convenience logging methods
    pub fn trace(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Trace, category, message, HashMap::new(), None);
    }

    pub fn debug(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Debug, category, message, HashMap::new(), None);
    }

    pub fn info(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Info, category, message, HashMap::new(), None);
    }

    pub fn warn(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Warn, category, message, HashMap::new(), None);
    }

    pub fn error(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Error, category, message, HashMap::new(), None);
    }

    pub fn critical(&self, category: LogCategory, message: impl Into<String>) {
        self.log(LogLevel::Critical, category, message, HashMap::new(), None);
    }

    /// Log with structured fields
    pub fn log_with_fields(
        &self,
        level: LogLevel,
        category: LogCategory,
        message: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
    ) {
        self.log(level, category, message, fields, None);
    }

    /// Check if we should sample this log entry
    fn should_sample(&self, category: LogCategory, level: LogLevel) -> bool {
        if !self.config.sampling_config.enable_sampling {
            return false;
        }

        // Never sample critical or error logs
        if level >= LogLevel::Error {
            return false;
        }

        let category_str = category.as_str();
        let sample_rate = self.config.sampling_config
            .high_volume_categories
            .get(category_str)
            .copied()
            .unwrap_or(self.config.sampling_config.sample_rate);

        rand::random::<f64>() > sample_rate
    }

    /// Log processor task
    async fn log_processor_task(
        mut receiver: mpsc::UnboundedReceiver<LogEntry>,
        writers: Arc<RwLock<HashMap<String, Box<dyn LogWriter + Send + Sync>>>>,
        shutdown_signal: Arc<tokio::sync::Notify>,
        _sampling_config: LogSamplingConfig,
    ) {
        let mut rotation_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        
        loop {
            tokio::select! {
                Some(entry) = receiver.recv() => {
                    // Write to all configured writers
                    let mut writers_guard = writers.write().unwrap();
                    for (name, writer) in writers_guard.iter_mut() {
                        if let Err(e) = writer.write_log(&entry) {
                            eprintln!("Failed to write log to {}: {}", name, e);
                        }
                    }
                }
                _ = rotation_interval.tick() => {
                    // Check for log rotation
                    let mut writers_guard = writers.write().unwrap();
                    for (name, writer) in writers_guard.iter_mut() {
                        if writer.should_rotate() {
                            if let Err(e) = writer.rotate() {
                                eprintln!("Failed to rotate log for {}: {}", name, e);
                            }
                        }
                    }
                }
                _ = shutdown_signal.notified() => {
                    // Flush all writers before shutdown
                    let mut writers_guard = writers.write().unwrap();
                    for (_, writer) in writers_guard.iter_mut() {
                        let _ = writer.flush();
                    }
                    break;
                }
            }
        }
    }

    /// Get logging statistics
    pub fn get_stats(&self) -> LoggingStats {
        let uptime = self.start_time.elapsed().unwrap_or_default();
        
        LoggingStats {
            uptime_seconds: uptime.as_secs(),
            active_writers: self.writers.read().unwrap().len(),
            log_level: self.config.log_level,
            structured_logging: self.config.structured_logging,
        }
    }

    /// Flush all log writers
    pub async fn flush(&self) {
        let mut writers = self.writers.write().unwrap();
        for (_, writer) in writers.iter_mut() {
            let _ = writer.flush();
        }
    }

    /// Shutdown the logger
    pub async fn shutdown(&self) {
        self.shutdown_signal.notify_waiters();
        
        // Wait a bit for pending logs to be processed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        self.flush().await;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoggingStats {
    pub uptime_seconds: u64,
    pub active_writers: usize,
    pub log_level: LogLevel,
    pub structured_logging: bool,
}

/// Global logger instance
static LOGGER: Mutex<Option<Arc<MixnodeLogger>>> = Mutex::new(None);

/// Initialize the global logger
pub fn init_logger(config: LoggingConfig, node_id: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let logger = MixnodeLogger::new(config, node_id)?;
    let mut global_logger = LOGGER.lock().unwrap();
    *global_logger = Some(Arc::new(logger));
    Ok(())
}

/// Get the global logger instance
pub fn get_logger() -> Option<Arc<MixnodeLogger>> {
    LOGGER.lock().unwrap().clone()
}

/// Convenience macros for logging
#[macro_export]
macro_rules! log_trace {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.trace($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.debug($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.info($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.warn($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.error($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_critical {
    ($category:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::get_logger() {
            logger.critical($category, format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_with_fields {
    ($level:expr, $category:expr, $message:expr, $($key:expr => $value:expr),+) => {
        if let Some(logger) = $crate::logging::get_logger() {
            let mut fields = std::collections::HashMap::new();
            $(
                fields.insert($key.to_string(), serde_json::json!($value));
            )+
            logger.log_with_fields($level, $category, $message, fields);
        }
    };
}