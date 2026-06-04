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
    flushed_up_to: usize,
}

impl AuditLog {
    pub fn new(log_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&log_dir);
        Self {
            log_dir,
            entries: Vec::new(),
            flushed_up_to: 0,
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
        use std::io::Write;

        if self.flushed_up_to >= self.entries.len() {
            return Ok(0);
        }

        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let log_file = self.log_dir.join(format!("audit_{}.jsonl", date));

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)?;

        let mut count = 0;
        for entry in &self.entries[self.flushed_up_to..] {
            let json = serde_json::json!({
                "timestamp": entry.timestamp.to_rfc3339(),
                "action": entry.action,
                "actor": entry.actor,
                "details": entry.details,
                "success": entry.success,
            });
            writeln!(file, "{}", json)?;
            count += 1;
        }

        self.flushed_up_to = self.entries.len();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_audit_log() -> AuditLog {
        let dir = std::env::temp_dir().join(format!(
            "maix-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        AuditLog::new(dir)
    }

    #[test]
    fn test_audit_log_new() {
        let log = temp_audit_log();
        assert!(log.entries().is_empty());
        assert!(log.log_dir().exists());
    }

    #[test]
    fn test_audit_log_record() {
        let mut log = temp_audit_log();
        log.record("skill.run", "user", "executed test skill", true);
        assert_eq!(log.entries().len(), 1);

        let entry = &log.entries()[0];
        assert_eq!(entry.action, "skill.run");
        assert_eq!(entry.actor, "user");
        assert_eq!(entry.details, "executed test skill");
        assert!(entry.success);
    }

    #[test]
    fn test_audit_log_record_failure() {
        let mut log = temp_audit_log();
        log.record("skill.run", "user", "timeout", false);
        assert!(!log.entries()[0].success);
    }

    #[test]
    fn test_audit_log_multiple_records() {
        let mut log = temp_audit_log();
        log.record("a", "u1", "d1", true);
        log.record("b", "u2", "d2", false);
        log.record("c", "u3", "d3", true);
        assert_eq!(log.entries().len(), 3);
    }

    #[test]
    fn test_audit_log_flush() {
        let mut log = temp_audit_log();
        log.record("a", "u", "d", true);
        log.record("b", "u", "d", false);
        let count = log.flush().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_audit_log_flush_empty() {
        let mut log = temp_audit_log();
        let count = log.flush().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_audit_log_entries_accessor() {
        let log = temp_audit_log();
        assert!(log.entries().is_empty());
    }

    #[test]
    fn test_audit_log_entries_after_record() {
        let mut log = temp_audit_log();
        log.record("action", "actor", "details", true);
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].action, "action");
        assert_eq!(log.entries()[0].actor, "actor");
        assert!(log.entries()[0].success);
    }

    #[test]
    fn test_audit_log_dir_matches() {
        let dir = std::env::temp_dir().join(format!("maix-test-audit-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let log = AuditLog::new(dir.clone());
        assert_eq!(log.log_dir(), &dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_audit_flush_idempotent() {
        let mut log = temp_audit_log();
        log.record("a", "u", "d", true);
        let count1 = log.flush().unwrap();
        assert_eq!(count1, 1);
        // Second flush with no new entries returns 0
        let count2 = log.flush().unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn test_audit_flush_does_not_clear() {
        let mut log = temp_audit_log();
        log.record("a", "u", "d", true);
        log.flush().unwrap();
        assert_eq!(log.entries().len(), 1); // entries still there
    }

    #[test]
    fn test_audit_record_empty_strings() {
        let mut log = temp_audit_log();
        log.record("", "", "", true);
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].action, "");
    }

    #[test]
    fn test_audit_flush_writes_file() {
        let mut log = temp_audit_log();
        log.record("skill.run", "user", "test details", true);
        log.record("skill.error", "user", "fail details", false);
        log.flush().unwrap();

        // Verify a JSONL file was created in the log directory
        let files: Vec<_> = std::fs::read_dir(log.log_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("audit_"))
            .collect();
        assert_eq!(files.len(), 1);

        let content = std::fs::read_to_string(files[0].path()).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry["action"], "skill.run");
        assert_eq!(entry["success"], true);

        let entry2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(entry2["action"], "skill.error");
        assert_eq!(entry2["success"], false);
    }

    #[test]
    fn test_audit_flush_appends_across_calls() {
        let mut log = temp_audit_log();
        log.record("first", "u", "d", true);
        log.flush().unwrap();

        log.record("second", "u", "d", true);
        log.flush().unwrap();

        let files: Vec<_> = std::fs::read_dir(log.log_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("audit_"))
            .collect();
        assert_eq!(files.len(), 1); // same day, same file

        let content = std::fs::read_to_string(files[0].path()).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }
}
