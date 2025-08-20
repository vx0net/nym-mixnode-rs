// Rate limiting and anti-abuse module
use governor::{Quota, RateLimiter as GovernorRateLimiter, DefaultDirectRateLimiter};
use std::net::IpAddr;
use std::collections::HashMap;
use std::time::SystemTime;

pub struct RateLimiter {
    per_ip_limiters: HashMap<IpAddr, DefaultDirectRateLimiter>,
    global_limiter: DefaultDirectRateLimiter,
    config: RateLimitConfig,
    // Anti-abuse tracking
    suspicious_ips: HashMap<IpAddr, SuspiciousActivity>,
    last_cleanup: SystemTime,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub packets_per_second_per_ip: u32,
    pub global_packets_per_second: u32,
    pub burst_size: u32,
    pub suspicious_threshold: u32,
    pub ban_duration_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            packets_per_second_per_ip: 100,
            global_packets_per_second: 30_000,
            burst_size: 50,
            suspicious_threshold: 1000,
            ban_duration_seconds: 300, // 5 minutes
        }
    }
}

#[derive(Debug)]
struct SuspiciousActivity {
    violation_count: u32,
    first_violation: SystemTime,
    last_violation: SystemTime,
    banned_until: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub enum RateLimitResult {
    Allowed,
    RateLimited(RateLimitReason),
    Banned(SystemTime), // Banned until this time
}

#[derive(Debug, Clone)]
pub enum RateLimitReason {
    PerIPLimit,
    GlobalLimit,
    Suspicious,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let global_quota = Quota::per_second(
            std::num::NonZeroU32::new(config.global_packets_per_second).unwrap()
        );
        
        Self {
            per_ip_limiters: HashMap::new(),
            global_limiter: DefaultDirectRateLimiter::direct(global_quota),
            config,
            suspicious_ips: HashMap::new(),
            last_cleanup: SystemTime::now(),
        }
    }
    
    /// Check if a packet from this IP should be allowed
    pub fn check_rate_limit(&mut self, source_ip: IpAddr) -> RateLimitResult {
        let now = SystemTime::now();
        
        // Periodic cleanup of old entries
        self.cleanup_old_entries(now);
        
        // Check if IP is currently banned
        if let Some(activity) = self.suspicious_ips.get(&source_ip) {
            if let Some(banned_until) = activity.banned_until {
                if now < banned_until {
                    return RateLimitResult::Banned(banned_until);
                }
            }
        }
        
        // Check global rate limit first (most important)
        if self.global_limiter.check().is_err() {
            self.record_violation(source_ip, RateLimitReason::GlobalLimit, now);
            return RateLimitResult::RateLimited(RateLimitReason::GlobalLimit);
        }
        
        // Check per-IP rate limit
        let limiter = self.per_ip_limiters.entry(source_ip)
            .or_insert_with(|| {
                let quota = Quota::per_second(
                    std::num::NonZeroU32::new(self.config.packets_per_second_per_ip).unwrap()
                );
                DefaultDirectRateLimiter::direct(quota)
            });
        
        if limiter.check().is_err() {
            self.record_violation(source_ip, RateLimitReason::PerIPLimit, now);
            return RateLimitResult::RateLimited(RateLimitReason::PerIPLimit);
        }
        
        RateLimitResult::Allowed
    }
    
    fn record_violation(&mut self, ip: IpAddr, reason: RateLimitReason, now: SystemTime) {
        let activity = self.suspicious_ips.entry(ip)
            .or_insert_with(|| SuspiciousActivity {
                violation_count: 0,
                first_violation: now,
                last_violation: now,
                banned_until: None,
            });
        
        activity.violation_count += 1;
        activity.last_violation = now;
        
        // Check if we should ban this IP
        if activity.violation_count >= self.config.suspicious_threshold {
            let ban_duration = std::time::Duration::from_secs(self.config.ban_duration_seconds);
            activity.banned_until = Some(now + ban_duration);
            
            println!("🚫 Banned IP {} for {} seconds due to {} violations", 
                    ip, self.config.ban_duration_seconds, activity.violation_count);
        }
    }
    
    fn cleanup_old_entries(&mut self, now: SystemTime) {
        // Only cleanup every 60 seconds to avoid performance impact
        if now.duration_since(self.last_cleanup).unwrap_or_default().as_secs() < 60 {
            return;
        }
        
        self.last_cleanup = now;
        
        // Clean up old per-IP limiters (keep only active ones from last hour)
        let one_hour_ago = now - std::time::Duration::from_secs(3600);
        
        // Remove old suspicious entries that are no longer banned
        self.suspicious_ips.retain(|_ip, activity| {
            if let Some(banned_until) = activity.banned_until {
                // Keep banned entries
                now < banned_until
            } else {
                // Keep recent violations (last hour)
                activity.last_violation > one_hour_ago
            }
        });
        
        println!("🧹 Cleaned up rate limiter: {} suspicious IPs tracked", 
                self.suspicious_ips.len());
    }
    
    /// Get current rate limiting statistics
    pub fn get_stats(&self) -> RateLimitStats {
        let now = SystemTime::now();
        let banned_count = self.suspicious_ips.values()
            .filter(|activity| {
                if let Some(banned_until) = activity.banned_until {
                    now < banned_until
                } else {
                    false
                }
            })
            .count();
        
        RateLimitStats {
            active_ip_limiters: self.per_ip_limiters.len(),
            suspicious_ips: self.suspicious_ips.len(),
            currently_banned: banned_count,
        }
    }
}

#[derive(Debug)]
pub struct RateLimitStats {
    pub active_ip_limiters: usize,
    pub suspicious_ips: usize,
    pub currently_banned: usize,
}