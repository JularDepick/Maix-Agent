//! Event log — record and query tool execution events for observability.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Type of event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    ToolStarted,
    ToolCompleted,
    ToolFailed,
    FileRead,
    FileWrite,
    FileEdit,
    FileDelete,
    ShellExec,
    Search,
    GitAction,
    System,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolStarted => write!(f, "TOOL_START"),
            Self::ToolCompleted => write!(f, "TOOL_DONE"),
            Self::ToolFailed => write!(f, "TOOL_FAIL"),
            Self::FileRead => write!(f, "FILE_READ"),
            Self::FileWrite => write!(f, "FILE_WRITE"),
            Self::FileEdit => write!(f, "FILE_EDIT"),
            Self::FileDelete => write!(f, "FILE_DEL"),
            Self::ShellExec => write!(f, "SHELL"),
            Self::Search => write!(f, "SEARCH"),
            Self::GitAction => write!(f, "GIT"),
            Self::System => write!(f, "SYSTEM"),
        }
    }
}

/// A single event in the log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub event_type: EventType,
    pub source: String,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration_ms: Option<u64>,
    pub success: bool,
}

/// In-memory event log with configurable capacity.
pub struct EventLog {
    events: VecDeque<Event>,
    max_size: usize,
    next_id: u64,
}

impl EventLog {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: VecDeque::new(),
            max_size,
            next_id: 1,
        }
    }

    /// Record a new event.
    pub fn record(
        &mut self,
        event_type: EventType,
        source: String,
        message: String,
        details: Option<String>,
        duration_ms: Option<u64>,
        success: bool,
    ) -> &Event {
        let event = Event {
            id: self.next_id,
            event_type,
            source,
            message,
            details,
            timestamp: chrono::Utc::now(),
            duration_ms,
            success,
        };
        self.next_id += 1;
        self.events.push_back(event);

        while self.events.len() > self.max_size {
            self.events.pop_front();
        }

        self.events.back().unwrap()
    }

    /// Get the last N events.
    pub fn recent(&self, n: usize) -> Vec<&Event> {
        self.events.iter().rev().take(n).collect()
    }

    /// Filter events by type.
    pub fn filter_by_type(&self, event_type: &EventType) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| &e.event_type == event_type)
            .collect()
    }

    /// Filter events by source.
    pub fn filter_by_source(&self, source: &str) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.source == source)
            .collect()
    }

    /// Get total event count.
    pub fn count(&self) -> usize {
        self.events.len()
    }

    /// Get event counts by type.
    pub fn counts_by_type(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for event in &self.events {
            *counts
                .entry(event.event_type.to_string())
                .or_insert(0) += 1;
        }
        counts
    }

    /// Format events for display.
    pub fn format_events(&self, events: &[&Event]) -> String {
        if events.is_empty() {
            return "No events.".into();
        }

        let mut lines = Vec::new();
        for event in events {
            let status = if event.success { "+" } else { "!" };
            let duration = event
                .duration_ms
                .map(|d| format!(" ({:.1}s)", d as f64 / 1000.0))
                .unwrap_or_default();
            let time = event.timestamp.format("%H:%M:%S");
            lines.push(format!(
                "  [{status}] {} {} {}{} — {}",
                time, event.event_type, event.source, duration, event.message
            ));
            if let Some(ref details) = event.details {
                // Show first line of details
                let first_line = details.lines().next().unwrap_or("");
                if !first_line.is_empty() {
                    lines.push(format!("        {}", first_line));
                }
            }
        }
        lines.join("\n")
    }

    /// Format summary statistics.
    pub fn format_summary(&self) -> String {
        let counts = self.counts_by_type();
        let mut lines = vec![
            format!("Event log: {} events (capacity: {})", self.count(), self.max_size),
            "".to_string(),
        ];

        let mut sorted: Vec<_> = counts.into_iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));

        for (event_type, count) in sorted {
            lines.push(format!("  {:<12} {}", event_type, count));
        }

        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Show recent events from the event log.
pub struct EventLogTool(pub Arc<Mutex<EventLog>>);

#[async_trait]
impl Tool for EventLogTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "event_log".into(),
            description: "Show recent tool execution events. Useful for debugging and understanding what happened.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer", "description": "Number of recent events to show (default: 20)" },
                    "event_type": { "type": "string", "description": "Filter by event type (optional)" },
                    "source": { "type": "string", "description": "Filter by source tool name (optional)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let count = args["count"].as_u64().unwrap_or(20) as usize;
        let event_type_filter = args["event_type"].as_str();
        let source_filter = args["source"].as_str();

        let log = self.0.lock().await;

        let events = if let Some(et) = event_type_filter {
            let et = match et.to_lowercase().as_str() {
                "tool_start" | "started" => EventType::ToolStarted,
                "tool_done" | "completed" => EventType::ToolCompleted,
                "tool_fail" | "failed" => EventType::ToolFailed,
                "file_read" | "read" => EventType::FileRead,
                "file_write" | "write" => EventType::FileWrite,
                "file_edit" | "edit" => EventType::FileEdit,
                "file_del" | "delete" => EventType::FileDelete,
                "shell" | "exec" => EventType::ShellExec,
                "search" => EventType::Search,
                "git" => EventType::GitAction,
                "system" => EventType::System,
                _ => return Ok(format!("Unknown event type: {}", et)),
            };
            log.filter_by_type(&et)
        } else if let Some(src) = source_filter {
            log.filter_by_source(src)
        } else {
            log.recent(count)
        };

        let limited: Vec<&Event> = events.into_iter().take(count).collect();
        Ok(log.format_events(&limited))
    }
}

/// Show event log statistics.
pub struct EventStatsTool(pub Arc<Mutex<EventLog>>);

#[async_trait]
impl Tool for EventStatsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "event_stats".into(),
            description: "Show event log statistics and counts by type.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let log = self.0.lock().await;
        Ok(log.format_summary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_recent() {
        let mut log = EventLog::new(100);
        log.record(
            EventType::ToolCompleted,
            "fs_read".into(),
            "Read src/main.rs".into(),
            None,
            Some(50),
            true,
        );
        log.record(
            EventType::ToolFailed,
            "shell_exec".into(),
            "Command failed".into(),
            Some("exit code 1".into()),
            Some(1200),
            false,
        );

        assert_eq!(log.count(), 2);
        let recent = log.recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].source, "shell_exec"); // most recent first
    }

    #[test]
    fn test_filter_by_type() {
        let mut log = EventLog::new(100);
        log.record(EventType::FileRead, "fs_read".into(), "a".into(), None, None, true);
        log.record(EventType::FileWrite, "fs_write".into(), "b".into(), None, None, true);
        log.record(EventType::FileRead, "fs_read".into(), "c".into(), None, None, true);

        let reads = log.filter_by_type(&EventType::FileRead);
        assert_eq!(reads.len(), 2);
    }

    #[test]
    fn test_max_size() {
        let mut log = EventLog::new(3);
        for i in 0..5 {
            log.record(
                EventType::System,
                "test".into(),
                format!("event {i}"),
                None,
                None,
                true,
            );
        }
        assert_eq!(log.count(), 3);
        let recent = log.recent(10);
        assert_eq!(recent[0].message, "event 4"); // most recent
    }

    #[test]
    fn test_counts_by_type() {
        let mut log = EventLog::new(100);
        log.record(EventType::FileRead, "a".into(), "".into(), None, None, true);
        log.record(EventType::FileRead, "b".into(), "".into(), None, None, true);
        log.record(EventType::FileWrite, "c".into(), "".into(), None, None, true);

        let counts = log.counts_by_type();
        assert_eq!(counts.get("FILE_READ"), Some(&2));
        assert_eq!(counts.get("FILE_WRITE"), Some(&1));
    }
}
