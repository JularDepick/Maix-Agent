//! Debug logging configuration — verbose mode and file logging.

use std::path::PathBuf;

/// Debug logger configuration.
pub struct DebugLogger {
    level: LogLevel,
    log_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn from_env() -> Self {
        match std::env::var("MAIX_DEBUG").as_deref() {
            Ok("1") | Ok("true") => Self::Debug,
            Ok("trace") => Self::Trace,
            _ => Self::Info,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl DebugLogger {
    pub fn new(debug: bool, log_file: Option<PathBuf>) -> Self {
        let level = if debug { LogLevel::Debug } else { LogLevel::Info };
        Self { level, log_file }
    }

    pub fn level(&self) -> LogLevel {
        self.level
    }

    pub fn log_file(&self) -> Option<&PathBuf> {
        self.log_file.as_ref()
    }

    pub fn is_debug(&self) -> bool {
        self.level >= LogLevel::Debug
    }

    pub fn init(&self) {
        // In real implementation, would configure tracing subscriber
        // This is a placeholder for the configuration logic
    }
}

/// Format a debug log line with file and line info.
pub fn format_log_line(level: LogLevel, file: &str, line: u32, message: &str) -> String {
    format!("[{}] {}:{} {}", level.as_str(), file, line, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Debug.as_str(), "debug");
    }

    #[test]
    fn test_debug_logger_new() {
        let logger = DebugLogger::new(true, None);
        assert_eq!(logger.level(), LogLevel::Debug);
        assert!(logger.is_debug());
    }

    #[test]
    fn test_debug_logger_info() {
        let logger = DebugLogger::new(false, None);
        assert_eq!(logger.level(), LogLevel::Info);
        assert!(!logger.is_debug());
    }

    #[test]
    fn test_debug_logger_with_file() {
        let path = PathBuf::from("/tmp/maix.log");
        let logger = DebugLogger::new(true, Some(path.clone()));
        assert_eq!(logger.log_file(), Some(&path));
    }

    #[test]
    fn test_format_log_line() {
        let line = format_log_line(LogLevel::Debug, "main.rs", 42, "test message");
        assert!(line.contains("[debug]"));
        assert!(line.contains("main.rs:42"));
        assert!(line.contains("test message"));
    }
}
