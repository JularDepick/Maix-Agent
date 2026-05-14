//! Error recovery and graceful degradation for the agent loop.

use std::collections::VecDeque;
use std::time::Instant;

/// Classification of errors for recovery decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    /// Transient network/API error — retry with backoff.
    Transient,
    /// Rate limit — wait and retry.
    RateLimit,
    /// Context overflow — compact and retry.
    ContextOverflow,
    /// Permission denied — report to user.
    Permission,
    /// Tool execution error — skip or fallback.
    ToolError,
    /// Permanent error — stop trying.
    Permanent,
}

/// Classify an error string into an error class.
pub fn classify_error(error: &str) -> ErrorClass {
    let lower = error.to_lowercase();

    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests") {
        return ErrorClass::RateLimit;
    }

    if lower.contains("context") && (lower.contains("overflow") || lower.contains("too long") || lower.contains("exceeds")) {
        return ErrorClass::ContextOverflow;
    }

    if lower.contains("permission") || lower.contains("denied") || lower.contains("forbidden") || lower.contains("403") {
        return ErrorClass::Permission;
    }

    if lower.contains("timeout") || lower.contains("connection") || lower.contains("network")
        || lower.contains("502") || lower.contains("503") || lower.contains("504")
        || lower.contains("eof") || lower.contains("broken pipe") {
        return ErrorClass::Transient;
    }

    if lower.contains("tool error") || lower.contains("tool execution") {
        return ErrorClass::ToolError;
    }

    if lower.contains("400") || lower.contains("401") || lower.contains("invalid") || lower.contains("malformed") {
        return ErrorClass::Permanent;
    }

    // Default to transient for unknown errors (safer to retry)
    ErrorClass::Transient
}

/// An error record for history tracking.
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    pub timestamp: Instant,
    pub error_class: ErrorClass,
    pub message: String,
    pub recovery_action: Option<String>,
    pub recovered: bool,
}

/// Recovery manager tracks error history and provides recovery suggestions.
pub struct RecoveryManager {
    history: VecDeque<ErrorRecord>,
    max_history: usize,
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryManager {
    pub fn new() -> Self {
        Self {
            history: VecDeque::new(),
            max_history: 100,
        }
    }

    /// Record an error and return a recovery suggestion.
    pub fn record_error(&mut self, error: &str) -> RecoveryAction {
        let error_class = classify_error(error);

        self.history.push_back(ErrorRecord {
            timestamp: Instant::now(),
            error_class: error_class.clone(),
            message: error.to_string(),
            recovery_action: None,
            recovered: false,
        });

        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        let action = self.suggest_recovery(&error_class, error);
        if let Some(record) = self.history.back_mut() {
            record.recovery_action = Some(format!("{:?}", action));
        }

        action
    }

    /// Mark the last error as recovered.
    pub fn mark_recovered(&mut self) {
        if let Some(record) = self.history.back_mut() {
            record.recovered = true;
        }
    }

    /// Suggest a recovery action based on error class.
    fn suggest_recovery(&self, class: &ErrorClass, _error: &str) -> RecoveryAction {
        // Check for repeated errors (same class in last 3 attempts)
        let recent_same_class = self.history.iter()
            .rev()
            .take(3)
            .filter(|r| r.error_class == *class)
            .count();

        if recent_same_class >= 3 {
            return RecoveryAction::Abort {
                reason: format!("Repeated {:?} errors (3+ in succession)", class),
            };
        }

        match class {
            ErrorClass::Transient => RecoveryAction::Retry {
                delay_ms: 2000,
                max_attempts: 3,
            },
            ErrorClass::RateLimit => RecoveryAction::WaitAndRetry {
                delay_ms: 10_000,
            },
            ErrorClass::ContextOverflow => RecoveryAction::CompactContext,
            ErrorClass::Permission => RecoveryAction::ReportToUser {
                message: "Permission denied. Check file permissions or API access.".into(),
            },
            ErrorClass::ToolError => RecoveryAction::SkipTool {
                message: "Tool execution failed. Skipping and continuing.".into(),
            },
            ErrorClass::Permanent => RecoveryAction::Abort {
                reason: "Permanent error — cannot recover.".into(),
            },
        }
    }

    /// Get error statistics.
    pub fn stats(&self) -> RecoveryStats {
        let total = self.history.len();
        let recovered = self.history.iter().filter(|r| r.recovered).count();
        let by_class = |class: &ErrorClass| {
            self.history.iter().filter(|r| r.error_class == *class).count()
        };

        RecoveryStats {
            total_errors: total,
            recovered,
            transient_errors: by_class(&ErrorClass::Transient),
            rate_limit_errors: by_class(&ErrorClass::RateLimit),
            context_overflow_errors: by_class(&ErrorClass::ContextOverflow),
            permission_errors: by_class(&ErrorClass::Permission),
            tool_errors: by_class(&ErrorClass::ToolError),
            permanent_errors: by_class(&ErrorClass::Permanent),
        }
    }
}

/// Suggested recovery action.
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Retry the operation after a delay.
    Retry { delay_ms: u64, max_attempts: u32 },
    /// Wait for rate limit to reset, then retry.
    WaitAndRetry { delay_ms: u64 },
    /// Compact context and retry.
    CompactContext,
    /// Report the error to the user and wait for input.
    ReportToUser { message: String },
    /// Skip the failed operation and continue.
    SkipTool { message: String },
    /// Abort — too many errors or permanent failure.
    Abort { reason: String },
}

/// Recovery statistics.
#[derive(Debug)]
pub struct RecoveryStats {
    pub total_errors: usize,
    pub recovered: usize,
    pub transient_errors: usize,
    pub rate_limit_errors: usize,
    pub context_overflow_errors: usize,
    pub permission_errors: usize,
    pub tool_errors: usize,
    pub permanent_errors: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rate_limit() {
        assert_eq!(classify_error("HTTP 429 Too Many Requests"), ErrorClass::RateLimit);
        assert_eq!(classify_error("rate limit exceeded"), ErrorClass::RateLimit);
    }

    #[test]
    fn test_classify_transient() {
        assert_eq!(classify_error("connection timeout"), ErrorClass::Transient);
        assert_eq!(classify_error("HTTP 503 Service Unavailable"), ErrorClass::Transient);
        assert_eq!(classify_error("network error"), ErrorClass::Transient);
    }

    #[test]
    fn test_classify_context_overflow() {
        assert_eq!(classify_error("context length exceeds maximum"), ErrorClass::ContextOverflow);
        assert_eq!(classify_error("context overflow"), ErrorClass::ContextOverflow);
    }

    #[test]
    fn test_classify_permission() {
        assert_eq!(classify_error("permission denied"), ErrorClass::Permission);
        assert_eq!(classify_error("HTTP 403 Forbidden"), ErrorClass::Permission);
    }

    #[test]
    fn test_classify_permanent() {
        assert_eq!(classify_error("HTTP 400 Bad Request"), ErrorClass::Permanent);
        assert_eq!(classify_error("invalid API key"), ErrorClass::Permanent);
    }

    #[test]
    fn test_recovery_manager() {
        let mut mgr = RecoveryManager::new();

        let action = mgr.record_error("connection timeout");
        assert!(matches!(action, RecoveryAction::Retry { .. }));

        mgr.mark_recovered();
        let stats = mgr.stats();
        assert_eq!(stats.total_errors, 1);
        assert_eq!(stats.recovered, 1);
    }

    #[test]
    fn test_repeated_errors_abort() {
        let mut mgr = RecoveryManager::new();

        // Record 3 transient errors
        mgr.record_error("timeout 1");
        mgr.record_error("timeout 2");
        let action = mgr.record_error("timeout 3");

        // Should abort after 3 repeated errors
        assert!(matches!(action, RecoveryAction::Abort { .. }));
    }
}
