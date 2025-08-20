// Structured logging with OpenTelemetry integration
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

use crate::logging::{LogCategory, LogLevel, MixnodeLogger};
use crate::metrics::telemetry::TelemetryCollector;

/// Structured logging context for correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub service: String,
    pub version: String,
    pub attributes: HashMap<String, serde_json::Value>,
}

impl LogContext {
    pub fn new(operation: &str) -> Self {
        Self {
            trace_id: Self::generate_id(16),
            span_id: Self::generate_id(8),
            parent_span_id: None,
            operation: operation.to_string(),
            service: "nym-mixnode".to_string(),
            version: "1.0.0".to_string(),
            attributes: HashMap::new(),
        }
    }

    pub fn child(&self, operation: &str) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: Self::generate_id(8),
            parent_span_id: Some(self.span_id.clone()),
            operation: operation.to_string(),
            service: self.service.clone(),
            version: self.version.clone(),
            attributes: HashMap::new(),
        }
    }

    pub fn with_attribute(mut self, key: &str, value: serde_json::Value) -> Self {
        self.attributes.insert(key.to_string(), value);
        self
    }

    fn generate_id(bytes: usize) -> String {
        use rand::RngCore;
        let mut buffer = vec![0u8; bytes];
        rand::thread_rng().fill_bytes(&mut buffer);
        hex::encode(buffer)
    }
}

/// Structured logger with context support
pub struct StructuredLogger {
    logger: Arc<MixnodeLogger>,
    telemetry: Option<Arc<TelemetryCollector>>,
    context_stack: Arc<RwLock<Vec<LogContext>>>,
}

impl StructuredLogger {
    pub fn new(logger: Arc<MixnodeLogger>, telemetry: Option<Arc<TelemetryCollector>>) -> Self {
        Self {
            logger,
            telemetry,
            context_stack: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start a new logging span
    pub async fn start_span(&self, operation: &str) -> LogSpan {
        let context = {
            let stack = self.context_stack.read().await;
            if let Some(parent) = stack.last() {
                parent.child(operation)
            } else {
                LogContext::new(operation)
            }
        };

        {
            let mut stack = self.context_stack.write().await;
            stack.push(context.clone());
        }

        // Log span start
        let fields = self.context_to_fields(&context);
        self.logger.log_with_fields(
            LogLevel::Debug,
            LogCategory::Telemetry,
            format!("Started span: {}", operation),
            fields,
        );

        LogSpan::new(context, self.context_stack.clone(), self.logger.clone())
    }

    /// Log structured event
    pub async fn log_event(
        &self,
        level: LogLevel,
        category: LogCategory,
        event: &str,
        fields: HashMap<String, serde_json::Value>,
    ) {
        let mut combined_fields = fields;
        
        // Add context information
        {
            let stack = self.context_stack.read().await;
            if let Some(context) = stack.last() {
                let context_fields = self.context_to_fields(context);
                combined_fields.extend(context_fields);
            }
        }

        // Add timestamp and event type
        combined_fields.insert("event".to_string(), serde_json::json!(event));
        combined_fields.insert("timestamp_ns".to_string(), 
            serde_json::json!(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));

        self.logger.log_with_fields(level, category, event.to_string(), combined_fields);
    }

    /// Log performance metrics
    pub async fn log_performance(
        &self,
        operation: &str,
        duration_ms: f64,
        success: bool,
        additional_fields: HashMap<String, serde_json::Value>,
    ) {
        let mut fields = additional_fields;
        fields.insert("operation".to_string(), serde_json::json!(operation));
        fields.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
        fields.insert("success".to_string(), serde_json::json!(success));

        let level = if success { LogLevel::Info } else { LogLevel::Warn };
        let message = if success {
            format!("Operation '{}' completed in {:.2}ms", operation, duration_ms)
        } else {
            format!("Operation '{}' failed after {:.2}ms", operation, duration_ms)
        };

        self.log_event(level, LogCategory::Performance, &message, fields).await;

        // Send to telemetry if available
        if let Some(ref telemetry) = self.telemetry {
            telemetry.record_operation_duration(operation, duration_ms, success);
        }
    }

    /// Log security event
    pub async fn log_security_event(
        &self,
        event_type: SecurityEventType,
        severity: SecuritySeverity,
        description: &str,
        source_ip: Option<&str>,
        additional_context: HashMap<String, serde_json::Value>,
    ) {
        let mut fields = additional_context;
        fields.insert("event_type".to_string(), serde_json::json!(event_type));
        fields.insert("severity".to_string(), serde_json::json!(severity));
        
        if let Some(ip) = source_ip {
            fields.insert("source_ip".to_string(), serde_json::json!(ip));
        }

        let level = match severity {
            SecuritySeverity::Low => LogLevel::Info,
            SecuritySeverity::Medium => LogLevel::Warn,
            SecuritySeverity::High => LogLevel::Error,
            SecuritySeverity::Critical => LogLevel::Critical,
        };

        self.log_event(level, LogCategory::Security, description, fields).await;
    }

    /// Log network event
    pub async fn log_network_event(
        &self,
        event_type: NetworkEventType,
        peer_id: Option<&str>,
        details: HashMap<String, serde_json::Value>,
    ) {
        let mut fields = details;
        fields.insert("event_type".to_string(), serde_json::json!(event_type));
        
        if let Some(peer) = peer_id {
            fields.insert("peer_id".to_string(), serde_json::json!(peer));
        }

        let level = match event_type {
            NetworkEventType::PeerConnected | NetworkEventType::PeerDisconnected => LogLevel::Info,
            NetworkEventType::ConnectionFailed | NetworkEventType::MessageDropped => LogLevel::Warn,
            NetworkEventType::ProtocolError => LogLevel::Error,
        };

        let message = format!("Network event: {:?}", event_type);
        self.log_event(level, LogCategory::Network, &message, fields).await;
    }

    /// Log packet processing event
    pub async fn log_packet_event(
        &self,
        packet_type: PacketEventType,
        packet_id: Option<&str>,
        processing_time_ms: Option<f64>,
        success: bool,
        details: HashMap<String, serde_json::Value>,
    ) {
        let mut fields = details;
        fields.insert("packet_type".to_string(), serde_json::json!(packet_type));
        fields.insert("success".to_string(), serde_json::json!(success));
        
        if let Some(id) = packet_id {
            fields.insert("packet_id".to_string(), serde_json::json!(id));
        }
        
        if let Some(duration) = processing_time_ms {
            fields.insert("processing_time_ms".to_string(), serde_json::json!(duration));
        }

        let level = if success { LogLevel::Debug } else { LogLevel::Warn };
        let message = format!("Packet event: {:?} - {}", packet_type, 
                            if success { "success" } else { "failed" });

        self.log_event(level, LogCategory::PacketProcessing, &message, fields).await;
    }

    fn context_to_fields(&self, context: &LogContext) -> HashMap<String, serde_json::Value> {
        let mut fields = HashMap::new();
        fields.insert("trace_id".to_string(), serde_json::json!(context.trace_id));
        fields.insert("span_id".to_string(), serde_json::json!(context.span_id));
        fields.insert("operation".to_string(), serde_json::json!(context.operation));
        fields.insert("service".to_string(), serde_json::json!(context.service));
        
        if let Some(ref parent) = context.parent_span_id {
            fields.insert("parent_span_id".to_string(), serde_json::json!(parent));
        }
        
        // Add context attributes
        for (key, value) in &context.attributes {
            fields.insert(format!("attr.{}", key), value.clone());
        }
        
        fields
    }
}

/// Logging span for structured operations
pub struct LogSpan {
    context: LogContext,
    context_stack: Arc<RwLock<Vec<LogContext>>>,
    logger: Arc<MixnodeLogger>,
    start_time: SystemTime,
}

impl LogSpan {
    fn new(
        context: LogContext,
        context_stack: Arc<RwLock<Vec<LogContext>>>,
        logger: Arc<MixnodeLogger>,
    ) -> Self {
        Self {
            context,
            context_stack,
            logger,
            start_time: SystemTime::now(),
        }
    }

    /// Add attribute to the current span
    pub fn set_attribute(&mut self, key: &str, value: serde_json::Value) {
        self.context.attributes.insert(key.to_string(), value);
    }

    /// Log an event within this span
    pub fn log_event(&self, level: LogLevel, category: LogCategory, message: &str, fields: HashMap<String, serde_json::Value>) {
        let mut span_fields = self.context.attributes.clone();
        span_fields.extend(fields);
        span_fields.insert("trace_id".to_string(), serde_json::json!(self.context.trace_id));
        span_fields.insert("span_id".to_string(), serde_json::json!(self.context.span_id));
        
        self.logger.log_with_fields(level, category, message, span_fields);
    }

    /// Record an error in this span
    pub fn record_error(&mut self, error: &dyn std::error::Error) {
        self.set_attribute("error", serde_json::json!(true));
        self.set_attribute("error.message", serde_json::json!(error.to_string()));
        
        self.log_event(
            LogLevel::Error,
            LogCategory::System,
            "Span error occurred",
            HashMap::new(),
        );
    }

    /// Finish the span
    pub async fn finish(self) {
        let duration = self.start_time.elapsed().unwrap_or_default();
        let duration_ms = duration.as_millis() as f64;

        // Log span completion
        let mut fields = HashMap::new();
        fields.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
        fields.insert("trace_id".to_string(), serde_json::json!(self.context.trace_id));
        fields.insert("span_id".to_string(), serde_json::json!(self.context.span_id));

        self.logger.log_with_fields(
            LogLevel::Debug,
            LogCategory::Telemetry,
            format!("Finished span: {} ({:.2}ms)", self.context.operation, duration_ms),
            fields,
        );

        // Remove from context stack
        {
            let mut stack = self.context_stack.write().await;
            stack.pop();
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SecurityEventType {
    AuthenticationFailure,
    RateLimitExceeded,
    SuspiciousActivity,
    IntrusionAttempt,
    AccessDenied,
    ProtocolViolation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NetworkEventType {
    PeerConnected,
    PeerDisconnected,
    ConnectionFailed,
    MessageDropped,
    ProtocolError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PacketEventType {
    SphinxReceived,
    SphinxProcessed,
    SphinxForwarded,
    CoverTrafficGenerated,
    PacketDropped,
}

/// Helper for timing operations
pub struct TimedOperation {
    name: String,
    start_time: SystemTime,
    logger: Arc<StructuredLogger>,
}

impl TimedOperation {
    pub fn start(name: &str, logger: Arc<StructuredLogger>) -> Self {
        Self {
            name: name.to_string(),
            start_time: SystemTime::now(),
            logger,
        }
    }

    pub async fn finish(self, success: bool, additional_fields: HashMap<String, serde_json::Value>) {
        let duration = self.start_time.elapsed().unwrap_or_default();
        let duration_ms = duration.as_millis() as f64;

        self.logger.log_performance(&self.name, duration_ms, success, additional_fields).await;
    }
}

#[macro_export]
macro_rules! time_operation {
    ($logger:expr, $name:expr, $code:block) => {{
        let timer = $crate::logging::structured::TimedOperation::start($name, $logger.clone());
        let result = $code;
        let success = result.is_ok();
        timer.finish(success, std::collections::HashMap::new()).await;
        result
    }};
}

#[macro_export]
macro_rules! log_structured {
    ($logger:expr, $level:expr, $category:expr, $message:expr, $($key:expr => $value:expr),*) => {
        {
            let mut fields = std::collections::HashMap::new();
            $(
                fields.insert($key.to_string(), serde_json::json!($value));
            )*
            $logger.log_event($level, $category, $message, fields).await;
        }
    };
}