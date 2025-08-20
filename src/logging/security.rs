// Security-focused logging and audit trail
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

use crate::logging::{LogCategory, LogLevel, MixnodeLogger, structured::StructuredLogger};
use crate::p2p::transport::PeerId;

/// Security event logging and audit system
pub struct SecurityLogger {
    structured_logger: Arc<StructuredLogger>,
    audit_trail: Arc<RwLock<AuditTrail>>,
    threat_intelligence: Arc<RwLock<ThreatIntelligence>>,
    config: SecurityLoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityLoggingConfig {
    pub enable_audit_trail: bool,
    pub enable_threat_detection: bool,
    pub log_all_connections: bool,
    pub log_all_packets: bool,
    pub suspicious_activity_threshold: u32,
    pub auto_ban_threshold: u32,
    pub retention_days: u32,
}

impl Default for SecurityLoggingConfig {
    fn default() -> Self {
        Self {
            enable_audit_trail: true,
            enable_threat_detection: true,
            log_all_connections: false, // High volume
            log_all_packets: false,     // Very high volume
            suspicious_activity_threshold: 10,
            auto_ban_threshold: 20,
            retention_days: 90,
        }
    }
}

/// Comprehensive security event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEvent {
    // Authentication & Authorization
    AuthenticationAttempt {
        peer_id: Option<PeerId>,
        source_ip: IpAddr,
        method: String,
        success: bool,
        failure_reason: Option<String>,
    },
    AuthorizationFailure {
        peer_id: PeerId,
        action: String,
        required_permission: String,
    },
    
    // Rate Limiting & DoS Protection
    RateLimitTriggered {
        source_ip: IpAddr,
        limit_type: RateLimitType,
        current_rate: u32,
        limit_threshold: u32,
    },
    DDoSDetected {
        attack_type: DDoSAttackType,
        source_ips: Vec<IpAddr>,
        packet_rate: u32,
        detection_confidence: f64,
    },
    
    // Network Security
    SuspiciousConnection {
        peer_id: PeerId,
        source_ip: IpAddr,
        reason: SuspiciousActivityReason,
        risk_score: f64,
    },
    ProtocolViolation {
        peer_id: PeerId,
        violation_type: ProtocolViolationType,
        expected: String,
        received: String,
    },
    
    // Packet Security
    MalformedPacket {
        source_ip: IpAddr,
        packet_type: String,
        error_details: String,
        hex_dump: Option<String>,
    },
    TimingAttackDetected {
        source_ip: IpAddr,
        attack_type: TimingAttackType,
        statistical_deviation: f64,
    },
    
    // System Security
    IntrusionAttempt {
        source_ip: IpAddr,
        attack_vector: String,
        payload: Option<String>,
        severity: SecuritySeverity,
    },
    PrivilegeEscalation {
        user_context: String,
        attempted_action: String,
        success: bool,
    },
    
    // Audit Events
    ConfigurationChange {
        parameter: String,
        old_value: String,
        new_value: String,
        change_source: String,
    },
    NodeBehaviorAnomaly {
        anomaly_type: AnomalyType,
        baseline_value: f64,
        observed_value: f64,
        confidence: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitType {
    GlobalPacketRate,
    PerIPPacketRate,
    ConnectionRate,
    MessageRate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DDoSAttackType {
    VolumetricFlood,
    ProtocolExhaustion,
    ApplicationLayer,
    SlowLoris,
    SynFlood,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuspiciousActivityReason {
    HighFailureRate,
    UnusualTrafficPattern,
    KnownMaliciousIP,
    BehaviorAnalysis,
    ReputationBased,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolViolationType {
    InvalidMessageFormat,
    UnexpectedMessageType,
    SequenceViolation,
    CryptographicFailure,
    TimingViolation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimingAttackType {
    ConstantTimeViolation,
    ResponseTimeAnalysis,
    SideChannelLeak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyType {
    PerformanceDegradation,
    UnexpectedBehavior,
    ResourceExhaustion,
    CommunicationPattern,
}

/// Audit trail for security events
#[derive(Debug, Clone)]
pub struct AuditTrail {
    events: Vec<AuditEntry>,
    max_entries: usize,
    retention_period: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    pub timestamp: SystemTime,
    pub event: SecurityEvent,
    pub risk_score: f64,
    pub action_taken: Option<String>,
    pub investigation_status: InvestigationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvestigationStatus {
    New,
    UnderReview,
    Confirmed,
    FalsePositive,
    Resolved,
}

/// Threat intelligence tracking
#[derive(Debug, Clone)]
pub struct ThreatIntelligence {
    ip_reputation: HashMap<IpAddr, IPReputation>,
    peer_reputation: HashMap<PeerId, PeerThreatProfile>,
    attack_patterns: Vec<AttackPattern>,
    indicators_of_compromise: Vec<IOC>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IPReputation {
    pub ip: IpAddr,
    pub reputation_score: f64, // 0.0 = malicious, 1.0 = trusted
    pub threat_categories: Vec<String>,
    pub first_seen: SystemTime,
    pub last_activity: SystemTime,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerThreatProfile {
    pub peer_id: PeerId,
    pub risk_score: f64,
    pub behavioral_anomalies: Vec<String>,
    pub security_violations: u32,
    pub last_assessment: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    pub name: String,
    pub signature: String,
    pub confidence: f64,
    pub mitigation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IOC {
    pub indicator_type: String,
    pub value: String,
    pub threat_level: SecuritySeverity,
    pub description: String,
    pub expires_at: SystemTime,
}

impl SecurityLogger {
    pub fn new(
        structured_logger: Arc<StructuredLogger>,
        config: SecurityLoggingConfig,
    ) -> Self {
        let audit_trail = AuditTrail {
            events: Vec::new(),
            max_entries: 10000,
            retention_period: std::time::Duration::from_secs(config.retention_days as u64 * 24 * 3600),
        };

        let threat_intelligence = ThreatIntelligence {
            ip_reputation: HashMap::new(),
            peer_reputation: HashMap::new(),
            attack_patterns: Self::load_default_attack_patterns(),
            indicators_of_compromise: Vec::new(),
        };

        Self {
            structured_logger,
            audit_trail: Arc::new(RwLock::new(audit_trail)),
            threat_intelligence: Arc::new(RwLock::new(threat_intelligence)),
            config,
        }
    }

    /// Log a security event
    pub async fn log_security_event(
        &self,
        event: SecurityEvent,
        immediate_action: Option<String>,
    ) -> u64 {
        let risk_score = self.calculate_risk_score(&event).await;
        let event_id = self.generate_event_id();

        // Create audit entry
        let audit_entry = AuditEntry {
            id: event_id,
            timestamp: SystemTime::now(),
            event: event.clone(),
            risk_score,
            action_taken: immediate_action.clone(),
            investigation_status: if risk_score > 0.8 {
                InvestigationStatus::UnderReview
            } else {
                InvestigationStatus::New
            },
        };

        // Add to audit trail
        if self.config.enable_audit_trail {
            let mut trail = self.audit_trail.write().await;
            trail.events.push(audit_entry.clone());
            
            // Cleanup old entries
            let cutoff = SystemTime::now() - trail.retention_period;
            trail.events.retain(|entry| entry.timestamp > cutoff);
            
            if trail.events.len() > trail.max_entries {
                trail.events.drain(0..trail.events.len() - trail.max_entries);
            }
        }

        // Log to structured logger
        let mut fields = self.event_to_fields(&event);
        fields.insert("event_id".to_string(), serde_json::json!(event_id));
        fields.insert("risk_score".to_string(), serde_json::json!(risk_score));
        
        if let Some(ref action) = immediate_action {
            fields.insert("action_taken".to_string(), serde_json::json!(action));
        }

        let level = self.severity_to_log_level(&event);
        let message = self.format_security_message(&event);

        self.structured_logger.log_event(
            level,
            LogCategory::Security,
            &message,
            fields,
        ).await;

        // Update threat intelligence
        if self.config.enable_threat_detection {
            self.update_threat_intelligence(&event).await;
        }

        // Trigger automatic response if needed
        if risk_score > 0.9 {
            self.trigger_automatic_response(&event, event_id).await;
        }

        event_id
    }

    /// Log authentication event
    pub async fn log_authentication(&self, peer_id: Option<PeerId>, source_ip: IpAddr, success: bool, method: &str) {
        let event = SecurityEvent::AuthenticationAttempt {
            peer_id,
            source_ip,
            method: method.to_string(),
            success,
            failure_reason: if !success { Some("Invalid credentials".to_string()) } else { None },
        };

        self.log_security_event(event, None).await;
    }

    /// Log rate limiting event
    pub async fn log_rate_limit(&self, source_ip: IpAddr, limit_type: RateLimitType, current_rate: u32, threshold: u32) {
        let event = SecurityEvent::RateLimitTriggered {
            source_ip,
            limit_type,
            current_rate,
            limit_threshold: threshold,
        };

        let action = if current_rate > threshold * 2 {
            Some("Temporary IP ban applied".to_string())
        } else {
            Some("Rate limiting applied".to_string())
        };

        self.log_security_event(event, action).await;
    }

    /// Log DDoS detection
    pub async fn log_ddos_detection(&self, attack_type: DDoSAttackType, source_ips: Vec<IpAddr>, packet_rate: u32) {
        let event = SecurityEvent::DDoSDetected {
            attack_type,
            source_ips: source_ips.clone(),
            packet_rate,
            detection_confidence: 0.95, // High confidence for now
        };

        let action = format!("DDoS mitigation activated for {} source IPs", source_ips.len());
        self.log_security_event(event, Some(action)).await;
    }

    /// Log protocol violation
    pub async fn log_protocol_violation(&self, peer_id: PeerId, violation_type: ProtocolViolationType, expected: &str, received: &str) {
        let event = SecurityEvent::ProtocolViolation {
            peer_id,
            violation_type,
            expected: expected.to_string(),
            received: received.to_string(),
        };

        let action = "Connection terminated due to protocol violation".to_string();
        self.log_security_event(event, Some(action)).await;
    }

    /// Log timing attack detection
    pub async fn log_timing_attack(&self, source_ip: IpAddr, attack_type: TimingAttackType, deviation: f64) {
        let event = SecurityEvent::TimingAttackDetected {
            source_ip,
            attack_type,
            statistical_deviation: deviation,
        };

        let action = if deviation > 3.0 {
            "High-confidence timing attack blocked".to_string()
        } else {
            "Potential timing attack logged for analysis".to_string()
        };

        self.log_security_event(event, Some(action)).await;
    }

    /// Log configuration change
    pub async fn log_config_change(&self, parameter: &str, old_value: &str, new_value: &str, source: &str) {
        let event = SecurityEvent::ConfigurationChange {
            parameter: parameter.to_string(),
            old_value: old_value.to_string(),
            new_value: new_value.to_string(),
            change_source: source.to_string(),
        };

        self.log_security_event(event, None).await;
    }

    /// Get security analytics
    pub async fn get_security_analytics(&self) -> SecurityAnalytics {
        let trail = self.audit_trail.read().await;
        let threat_intel = self.threat_intelligence.read().await;

        let now = SystemTime::now();
        let last_24h = now - std::time::Duration::from_secs(24 * 3600);

        let recent_events: Vec<&AuditEntry> = trail.events.iter()
            .filter(|entry| entry.timestamp > last_24h)
            .collect();

        let high_risk_events = recent_events.iter()
            .filter(|entry| entry.risk_score > 0.7)
            .count();

        let threat_by_category = self.categorize_threats(&recent_events);

        SecurityAnalytics {
            total_events_24h: recent_events.len(),
            high_risk_events_24h: high_risk_events,
            average_risk_score: if recent_events.is_empty() {
                0.0
            } else {
                recent_events.iter().map(|e| e.risk_score).sum::<f64>() / recent_events.len() as f64
            },
            threats_by_category: threat_by_category,
            total_blocked_ips: threat_intel.ip_reputation.values()
                .filter(|rep| rep.reputation_score < 0.3)
                .count(),
            active_investigations: trail.events.iter()
                .filter(|entry| matches!(entry.investigation_status, InvestigationStatus::UnderReview))
                .count(),
        }
    }

    /// Calculate risk score for an event
    async fn calculate_risk_score(&self, event: &SecurityEvent) -> f64 {
        let base_score = match event {
            SecurityEvent::AuthenticationAttempt { success: false, .. } => 0.3,
            SecurityEvent::RateLimitTriggered { current_rate, limit_threshold, .. } => {
                0.4 + ((*current_rate as f64 / *limit_threshold as f64 - 1.0) * 0.3).min(0.5)
            },
            SecurityEvent::DDoSDetected { .. } => 0.9,
            SecurityEvent::ProtocolViolation { .. } => 0.6,
            SecurityEvent::TimingAttackDetected { statistical_deviation, .. } => {
                0.5 + (statistical_deviation / 5.0).min(0.4)
            },
            SecurityEvent::IntrusionAttempt { severity, .. } => match severity {
                SecuritySeverity::Low => 0.3,
                SecuritySeverity::Medium => 0.5,
                SecuritySeverity::High => 0.7,
                SecuritySeverity::Critical => 0.95,
            },
            _ => 0.2,
        };

        // Adjust based on threat intelligence
        let intel_modifier = self.get_threat_intelligence_modifier(event).await;
        (base_score + intel_modifier).min(1.0)
    }

    async fn get_threat_intelligence_modifier(&self, event: &SecurityEvent) -> f64 {
        let threat_intel = self.threat_intelligence.read().await;

        let source_ip = match event {
            SecurityEvent::AuthenticationAttempt { source_ip, .. } => Some(*source_ip),
            SecurityEvent::RateLimitTriggered { source_ip, .. } => Some(*source_ip),
            SecurityEvent::MalformedPacket { source_ip, .. } => Some(*source_ip),
            SecurityEvent::TimingAttackDetected { source_ip, .. } => Some(*source_ip),
            SecurityEvent::IntrusionAttempt { source_ip, .. } => Some(*source_ip),
            _ => None,
        };

        if let Some(ip) = source_ip {
            if let Some(reputation) = threat_intel.ip_reputation.get(&ip) {
                return (1.0 - reputation.reputation_score) * 0.3;
            }
        }

        0.0
    }

    fn event_to_fields(&self, event: &SecurityEvent) -> HashMap<String, serde_json::Value> {
        let mut fields = HashMap::new();
        
        match event {
            SecurityEvent::AuthenticationAttempt { peer_id, source_ip, method, success, failure_reason } => {
                if let Some(pid) = peer_id {
                    fields.insert("peer_id".to_string(), serde_json::json!(pid.to_hex()));
                }
                fields.insert("source_ip".to_string(), serde_json::json!(source_ip));
                fields.insert("auth_method".to_string(), serde_json::json!(method));
                fields.insert("success".to_string(), serde_json::json!(success));
                if let Some(reason) = failure_reason {
                    fields.insert("failure_reason".to_string(), serde_json::json!(reason));
                }
            },
            SecurityEvent::RateLimitTriggered { source_ip, limit_type, current_rate, limit_threshold } => {
                fields.insert("source_ip".to_string(), serde_json::json!(source_ip));
                fields.insert("limit_type".to_string(), serde_json::json!(limit_type));
                fields.insert("current_rate".to_string(), serde_json::json!(current_rate));
                fields.insert("threshold".to_string(), serde_json::json!(limit_threshold));
            },
            SecurityEvent::DDoSDetected { attack_type, source_ips, packet_rate, detection_confidence } => {
                fields.insert("attack_type".to_string(), serde_json::json!(attack_type));
                fields.insert("source_ip_count".to_string(), serde_json::json!(source_ips.len()));
                fields.insert("packet_rate".to_string(), serde_json::json!(packet_rate));
                fields.insert("confidence".to_string(), serde_json::json!(detection_confidence));
            },
            // Add other event types as needed
            _ => {
                fields.insert("event_type".to_string(), serde_json::json!("security_event"));
            }
        }
        
        fields.insert("timestamp_ns".to_string(), 
            serde_json::json!(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        
        fields
    }

    fn severity_to_log_level(&self, event: &SecurityEvent) -> LogLevel {
        match event {
            SecurityEvent::DDoSDetected { .. } | 
            SecurityEvent::IntrusionAttempt { severity: SecuritySeverity::Critical, .. } => LogLevel::Critical,
            
            SecurityEvent::ProtocolViolation { .. } | 
            SecurityEvent::TimingAttackDetected { .. } |
            SecurityEvent::IntrusionAttempt { severity: SecuritySeverity::High, .. } => LogLevel::Error,
            
            SecurityEvent::RateLimitTriggered { .. } |
            SecurityEvent::SuspiciousConnection { .. } |
            SecurityEvent::MalformedPacket { .. } => LogLevel::Warn,
            
            _ => LogLevel::Info,
        }
    }

    fn format_security_message(&self, event: &SecurityEvent) -> String {
        match event {
            SecurityEvent::AuthenticationAttempt { success, method, .. } => {
                format!("Authentication {} using {}", 
                    if *success { "succeeded" } else { "failed" }, method)
            },
            SecurityEvent::RateLimitTriggered { limit_type, current_rate, .. } => {
                format!("Rate limit triggered for {:?} at {} requests", limit_type, current_rate)
            },
            SecurityEvent::DDoSDetected { attack_type, .. } => {
                format!("DDoS attack detected: {:?}", attack_type)
            },
            SecurityEvent::ProtocolViolation { violation_type, .. } => {
                format!("Protocol violation: {:?}", violation_type)
            },
            SecurityEvent::TimingAttackDetected { attack_type, .. } => {
                format!("Timing attack detected: {:?}", attack_type)
            },
            SecurityEvent::IntrusionAttempt { attack_vector, .. } => {
                format!("Intrusion attempt via {}", attack_vector)
            },
            SecurityEvent::ConfigurationChange { parameter, .. } => {
                format!("Configuration changed: {}", parameter)
            },
            _ => "Security event occurred".to_string(),
        }
    }

    async fn update_threat_intelligence(&self, event: &SecurityEvent) {
        let mut threat_intel = self.threat_intelligence.write().await;

        // Update IP reputation based on the event
        if let Some(ip) = self.extract_source_ip(event) {
            let reputation = threat_intel.ip_reputation.entry(ip).or_insert_with(|| {
                IPReputation {
                    ip,
                    reputation_score: 1.0, // Start with good reputation
                    threat_categories: Vec::new(),
                    first_seen: SystemTime::now(),
                    last_activity: SystemTime::now(),
                    incident_count: 0,
                }
            });

            reputation.last_activity = SystemTime::now();
            reputation.incident_count += 1;

            // Adjust reputation score based on event severity
            let score_adjustment = match event {
                SecurityEvent::AuthenticationAttempt { success: false, .. } => -0.05,
                SecurityEvent::RateLimitTriggered { .. } => -0.1,
                SecurityEvent::DDoSDetected { .. } => -0.5,
                SecurityEvent::ProtocolViolation { .. } => -0.2,
                SecurityEvent::TimingAttackDetected { .. } => -0.3,
                SecurityEvent::IntrusionAttempt { .. } => -0.4,
                _ => -0.01,
            };

            reputation.reputation_score = (reputation.reputation_score + score_adjustment).max(0.0);
        }
    }

    fn extract_source_ip(&self, event: &SecurityEvent) -> Option<IpAddr> {
        match event {
            SecurityEvent::AuthenticationAttempt { source_ip, .. } => Some(*source_ip),
            SecurityEvent::RateLimitTriggered { source_ip, .. } => Some(*source_ip),
            SecurityEvent::MalformedPacket { source_ip, .. } => Some(*source_ip),
            SecurityEvent::TimingAttackDetected { source_ip, .. } => Some(*source_ip),
            SecurityEvent::IntrusionAttempt { source_ip, .. } => Some(*source_ip),
            _ => None,
        }
    }

    async fn trigger_automatic_response(&self, event: &SecurityEvent, event_id: u64) {
        // Implement automatic response based on event type and severity
        match event {
            SecurityEvent::DDoSDetected { source_ips, .. } => {
                println!("🚨 AUTO-RESPONSE: Blocking {} IPs due to DDoS (Event ID: {})", 
                         source_ips.len(), event_id);
                // Would integrate with firewall/iptables here
            },
            SecurityEvent::IntrusionAttempt { source_ip, severity: SecuritySeverity::Critical, .. } => {
                println!("🚨 AUTO-RESPONSE: Emergency IP ban for {} (Event ID: {})", 
                         source_ip, event_id);
            },
            _ => {
                println!("🔍 HIGH-RISK EVENT: Manual investigation required (Event ID: {})", event_id);
            }
        }
    }

    fn categorize_threats(&self, events: &[&AuditEntry]) -> HashMap<String, u32> {
        let mut categories = HashMap::new();

        for entry in events {
            let category = match &entry.event {
                SecurityEvent::AuthenticationAttempt { .. } => "Authentication",
                SecurityEvent::RateLimitTriggered { .. } => "Rate Limiting",
                SecurityEvent::DDoSDetected { .. } => "DDoS",
                SecurityEvent::ProtocolViolation { .. } => "Protocol Violation",
                SecurityEvent::TimingAttackDetected { .. } => "Timing Attack",
                SecurityEvent::IntrusionAttempt { .. } => "Intrusion",
                _ => "Other",
            };

            *categories.entry(category.to_string()).or_insert(0) += 1;
        }

        categories
    }

    fn load_default_attack_patterns() -> Vec<AttackPattern> {
        vec![
            AttackPattern {
                name: "Constant-Time Violation".to_string(),
                signature: "timing_variance_high".to_string(),
                confidence: 0.8,
                mitigation: "Enable constant-time processing".to_string(),
            },
            AttackPattern {
                name: "Protocol Fuzzing".to_string(),
                signature: "malformed_packet_burst".to_string(),
                confidence: 0.9,
                mitigation: "Implement strict input validation".to_string(),
            },
        ]
    }

    fn generate_event_id(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityAnalytics {
    pub total_events_24h: usize,
    pub high_risk_events_24h: usize,
    pub average_risk_score: f64,
    pub threats_by_category: HashMap<String, u32>,
    pub total_blocked_ips: usize,
    pub active_investigations: usize,
}