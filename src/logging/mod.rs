// Advanced structured logging
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use tracing::{info, Level};
use tracing_subscriber::{
    fmt::time::UtcTime,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Registry,
};

pub mod performance;
pub mod audit;
pub mod rotation;

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
    pub output: LogOutput,
    pub enable_colors: bool,
    pub enable_timestamps: bool,
    pub enable_line_numbers: bool,
    pub file_config: Option<FileLogConfig>,
    pub structured_fields: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Text,
    Compact,
    Pretty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    Stdout,
    Stderr,
    File(PathBuf),
    Multiple(Vec<LogOutput>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    pub path: PathBuf,
    pub max_size_mb: u64,
    pub max_files: u32,
    pub compress: bool,
    pub rotation: RotationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationConfig {
    Size(u64),      // Rotate when file reaches size in MB
    Time(String),   // Rotate based on time (daily, hourly, etc.)
    Both(u64, String),
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Text,
            output: LogOutput::Stdout,
            enable_colors: true,
            enable_timestamps: true,
            enable_line_numbers: false,
            file_config: None,
            structured_fields: true,
        }
    }
}

/// Logging manager
pub struct LoggingManager {
    config: LoggingConfig,
}

impl LoggingManager {
    pub fn new(config: LoggingConfig) -> Self {
        Self { config }
    }
    
    /// Initialize logging system
    pub fn initialize(&self) -> Result<(), String> {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.level));
        
        match self.config.format {
            LogFormat::Json => {
                let subscriber = Registry::default()
                    .with(filter)
                    .with(
                        tracing_subscriber::fmt::layer()
                            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
                    );
                
                subscriber.init();
            },
            LogFormat::Text | LogFormat::Compact | LogFormat::Pretty => {
                if self.config.enable_timestamps {
                    let subscriber = Registry::default()
                        .with(filter)
                        .with(tracing_subscriber::fmt::layer()
                            .with_ansi(self.config.enable_colors)
                            .with_timer(UtcTime::rfc_3339())
                            .with_line_number(self.config.enable_line_numbers));
                    subscriber.init();
                } else {
                    let subscriber = Registry::default()
                        .with(filter)
                        .with(tracing_subscriber::fmt::layer()
                            .with_ansi(self.config.enable_colors)
                            .with_timer(())
                            .with_line_number(self.config.enable_line_numbers));
                    subscriber.init();
                }
            }
        }
        
        info!("Logging system initialized with level: {}", self.config.level);
        Ok(())
    }
    
    /// Create performance logger
    pub fn create_performance_logger(&self) -> performance::PerformanceLogger {
        performance::PerformanceLogger::new(self.config.clone())
    }
    
    /// Create audit logger
    pub fn create_audit_logger(&self) -> audit::AuditLogger {
        audit::AuditLogger::new(self.config.clone())
    }
}