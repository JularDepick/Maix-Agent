//! Undo/redo manager for file operations.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Type of file operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Write,
    Edit,
    Delete,
    Create,
}

/// A single undo entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    pub id: u64,
    pub operation: Operation,
    pub file_path: PathBuf,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub description: String,
}

/// Undo/redo manager.
pub struct UndoManager {
    history: Vec<UndoEntry>,
    current: usize,
    max_history: usize,
    next_id: u64,
}

impl UndoManager {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            current: 0,
            max_history,
            next_id: 1,
        }
    }

    /// Record a new operation.
    pub fn record(&mut self, operation: Operation, file_path: PathBuf, old_content: Option<String>, new_content: Option<String>, description: String) {
        // Truncate any redo history
        self.history.truncate(self.current);

        let entry = UndoEntry {
            id: self.next_id,
            operation,
            file_path,
            old_content,
            new_content,
            description,
        };
        self.next_id += 1;

        self.history.push(entry);
        self.current = self.history.len();

        // Trim old entries
        if self.history.len() > self.max_history {
            let excess = self.history.len() - self.max_history;
            self.history.drain(..excess);
            self.current -= excess;
        }
    }

    /// Undo the last operation. Returns the entry that was undone.
    pub async fn undo(&mut self) -> MaixResult<Option<&UndoEntry>> {
        if self.current == 0 {
            return Ok(None);
        }

        self.current -= 1;
        let entry = &self.history[self.current];

        match &entry.operation {
            Operation::Write | Operation::Edit => {
                if let Some(ref old) = entry.old_content {
                    tokio::fs::write(&entry.file_path, old).await
                        .map_err(maix_core::MaixError::Io)?;
                }
            }
            Operation::Delete => {
                if let Some(ref old) = entry.old_content {
                    tokio::fs::write(&entry.file_path, old).await
                        .map_err(maix_core::MaixError::Io)?;
                }
            }
            Operation::Create => {
                tokio::fs::remove_file(&entry.file_path).await
                    .map_err(maix_core::MaixError::Io)?;
            }
        }

        Ok(Some(&self.history[self.current]))
    }

    /// Redo the next operation. Returns the entry that was redone.
    pub async fn redo(&mut self) -> MaixResult<Option<&UndoEntry>> {
        if self.current >= self.history.len() {
            return Ok(None);
        }

        let entry = &self.history[self.current];
        self.current += 1;

        match &entry.operation {
            Operation::Write | Operation::Edit | Operation::Create => {
                if let Some(ref new) = entry.new_content {
                    tokio::fs::write(&entry.file_path, new).await
                        .map_err(maix_core::MaixError::Io)?;
                }
            }
            Operation::Delete => {
                tokio::fs::remove_file(&entry.file_path).await
                    .map_err(maix_core::MaixError::Io)?;
            }
        }

        Ok(Some(&self.history[self.current - 1]))
    }

    /// Get the operation history.
    pub fn history(&self) -> &[UndoEntry] {
        &self.history
    }

    /// Current position in history.
    pub fn position(&self) -> usize {
        self.current
    }

    /// Format history for display.
    pub fn format_history(&self) -> String {
        if self.history.is_empty() {
            return "No operations in history.".into();
        }

        let mut lines = vec!["Operation history:".to_string()];
        for (i, entry) in self.history.iter().enumerate() {
            let marker = if i == self.current { "->" } else { "  " };
            let path = entry.file_path.display();
            lines.push(format!("  {marker} #{}: {} ({})", entry.id, entry.description, path));
        }
        lines.push(format!("Position: {}/{}", self.current, self.history.len()));
        lines.join("\n")
    }

    /// Clear history.
    pub fn clear(&mut self) {
        self.history.clear();
        self.current = 0;
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Undo the last file operation.
pub struct UndoTool(pub Arc<Mutex<UndoManager>>);

#[async_trait]
impl Tool for UndoTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "undo".into(),
            description: "Undo the last file operation (write, edit, delete, or create). Restores the file to its previous state.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mut mgr = self.0.lock().await;
        match mgr.undo().await? {
            Some(entry) => Ok(format!("Undone: {} ({})", entry.description, entry.file_path.display())),
            None => Ok("Nothing to undo.".into()),
        }
    }
}

/// Redo the last undone operation.
pub struct RedoTool(pub Arc<Mutex<UndoManager>>);

#[async_trait]
impl Tool for RedoTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "redo".into(),
            description: "Redo the last undone file operation.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mut mgr = self.0.lock().await;
        match mgr.redo().await? {
            Some(entry) => Ok(format!("Redone: {} ({})", entry.description, entry.file_path.display())),
            None => Ok("Nothing to redo.".into()),
        }
    }
}

/// Show operation history.
pub struct UndoHistoryTool(pub Arc<Mutex<UndoManager>>);

#[async_trait]
impl Tool for UndoHistoryTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "undo_history".into(),
            description: "Show the file operation history with undo/redo position.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mgr = self.0.lock().await;
        Ok(mgr.format_history())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_history() {
        let mut mgr = UndoManager::new(100);
        assert_eq!(mgr.position(), 0);

        mgr.record(Operation::Write, "/tmp/test.txt".into(), None, Some("hello".into()), "Write test.txt".into());
        assert_eq!(mgr.position(), 1);
        assert_eq!(mgr.history().len(), 1);

        mgr.record(Operation::Edit, "/tmp/test.txt".into(), Some("hello".into()), Some("world".into()), "Edit test.txt".into());
        assert_eq!(mgr.position(), 2);
    }

    #[test]
    fn test_max_history() {
        let mut mgr = UndoManager::new(2);
        mgr.record(Operation::Write, "/a".into(), None, Some("a".into()), "a".into());
        mgr.record(Operation::Write, "/b".into(), None, Some("b".into()), "b".into());
        mgr.record(Operation::Write, "/c".into(), None, Some("c".into()), "c".into());

        assert_eq!(mgr.history().len(), 2);
        assert_eq!(mgr.position(), 2);
    }

    #[test]
    fn test_format_history_empty() {
        let mgr = UndoManager::new(100);
        assert_eq!(mgr.format_history(), "No operations in history.");
    }
}
