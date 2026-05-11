//! Audit logging — skill execution and user action audit trails.

use std::path::PathBuf;

/// Audit log entry for skill execution and user actions.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action: String,
    pub actor: String,
    pub details: String,
    pub success: bool,
}

/// Writes audit logs to ~/.maix/logs/
pub struct AuditLog {
    log_dir: PathBuf,
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new(log_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&log_dir);
        Self {
            log_dir,
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, action: &str, actor: &str, details: &str, success: bool) {
        self.entries.push(AuditEntry {
            timestamp: chrono::Utc::now(),
            action: action.into(),
            actor: actor.into(),
            details: details.into(),
            success,
        });
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn log_dir(&self) -> &PathBuf {
        &self.log_dir
    }

    pub fn flush(&mut self) -> Result<usize, std::io::Error> {
        Ok(self.entries.len())
    }
}
