// Audit logging for security and compliance
use std::collections::HashMap;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error};

use super::LoggingConfig;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    Authentication,
    Authorization,
    DataAccess,
    Configuration,
    Security,
    Network,
    Performance,
    Error,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: String,
    pub event_type: AuditEventType,
    pub timestamp: SystemTime,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub source_ip: Option<String>,
    pub action: String,
    pub resource: Option<String>,
    pub result: AuditResult,
    pub details: HashMap<String, String>,
    pub risk_level: RiskLevel,
}

/// Audit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
    Warning,
    Error,
}

/// Risk levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Audit logger
pub struct AuditLogger {
    config: LoggingConfig,
}

impl AuditLogger {
    pub fn new(config: LoggingConfig) -> Self {
        Self { config }
    }
    
    /// Log audit event
    pub async fn log_event(&self, event: AuditEvent) {
        match event.risk_level {
            RiskLevel::Low => {
                info!(
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    action = %event.action,
                    result = ?event.result,
                    "Audit event"
                );
            },
            RiskLevel::Medium => {
                warn!(
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    action = %event.action,
                    result = ?event.result,
                    "Medium risk audit event"
                );
            },
            RiskLevel::High | RiskLevel::Critical => {
                error!(
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    action = %event.action,
                    result = ?event.result,
                    risk_level = ?event.risk_level,
                    "High risk audit event"
                );
            }
        }
    }
    
    /// Log authentication event
    pub async fn log_authentication(&self, user_id: &str, success: bool, source_ip: Option<String>) {
        let event = AuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: AuditEventType::Authentication,
            timestamp: SystemTime::now(),
            user_id: Some(user_id.to_string()),
            session_id: None,
            source_ip,
            action: "authenticate".to_string(),
            resource: None,
            result: if success { AuditResult::Success } else { AuditResult::Failure },
            details: HashMap::new(),
            risk_level: if success { RiskLevel::Low } else { RiskLevel::Medium },
        };
        
        self.log_event(event).await;
    }
    
    /// Log configuration change
    pub async fn log_configuration_change(&self, user_id: Option<String>, key: &str, old_value: Option<String>, new_value: &str) {
        let mut details = HashMap::new();
        details.insert("key".to_string(), key.to_string());
        details.insert("new_value".to_string(), new_value.to_string());
        if let Some(old) = old_value {
            details.insert("old_value".to_string(), old);
        }
        
        let event = AuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: AuditEventType::Configuration,
            timestamp: SystemTime::now(),
            user_id,
            session_id: None,
            source_ip: None,
            action: "configuration_change".to_string(),
            resource: Some(key.to_string()),
            result: AuditResult::Success,
            details,
            risk_level: RiskLevel::Medium,
        };
        
        self.log_event(event).await;
    }
    
    /// Log security event
    pub async fn log_security_event(&self, action: &str, details: HashMap<String, String>, risk_level: RiskLevel) {
        let event = AuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: AuditEventType::Security,
            timestamp: SystemTime::now(),
            user_id: None,
            session_id: None,
            source_ip: None,
            action: action.to_string(),
            resource: None,
            result: AuditResult::Warning,
            details,
            risk_level,
        };
        
        self.log_event(event).await;
    }
}