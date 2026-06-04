//! Agent Desk — virtual workspace with sticky notes, pinned files, and task board.
//!
//! Inspired by OpenHanako's "Desk" metaphor: a persistent workspace per session
//! where the agent and user can pin context, leave notes, and track tasks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Sticky note color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum NoteColor {
    #[default]
    Yellow,
    Blue,
    Green,
    Red,
    Purple,
}


impl NoteColor {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "blue" | "b" => Self::Blue,
            "green" | "g" => Self::Green,
            "red" | "r" => Self::Red,
            "purple" | "p" => Self::Purple,
            _ => Self::Yellow,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Red => "red",
            Self::Purple => "purple",
        }
    }
}

/// A sticky note on the desk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickyNote {
    pub id: String,
    pub content: String,
    pub color: NoteColor,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub pinned: bool,
}

#[allow(dead_code)]
impl StickyNote {
    pub fn new(id: &str, content: &str, color: NoteColor) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_string(),
            content: content.to_string(),
            color,
            created_at: now,
            updated_at: now,
            pinned: false,
        }
    }
}

/// A file pinned to the desk with a preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedFile {
    pub path: PathBuf,
    pub preview: String,
    pub line_count: usize,
    pub dirty: bool,
    pub pinned_at: DateTime<Utc>,
}

#[allow(dead_code)]
impl PinnedFile {
    /// Create a pinned file, reading the first `preview_lines` lines as preview.
    pub fn new(path: &Path, preview_lines: usize) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let line_count = content.lines().count();
        let preview: String = content
            .lines()
            .take(preview_lines)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Self {
            path: path.to_path_buf(),
            preview,
            line_count,
            dirty: false,
            pinned_at: Utc::now(),
        })
    }

    /// Refresh the preview from disk.
    pub fn refresh(&mut self, preview_lines: usize) -> std::io::Result<()> {
        let content = std::fs::read_to_string(&self.path)?;
        self.line_count = content.lines().count();
        self.preview = content
            .lines()
            .take(preview_lines)
            .collect::<Vec<_>>()
            .join("\n");
        Ok(())
    }
}

/// Task status on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
}

impl TaskStatus {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Done => "done",
        }
    }

    pub fn checkbox(&self) -> &'static str {
        match self {
            Self::Todo => "[ ]",
            Self::InProgress => "[~]",
            Self::Done => "[x]",
        }
    }
}

/// A task on the board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardTask {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}

#[allow(dead_code)]
impl BoardTask {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            status: TaskStatus::Todo,
            created_at: Utc::now(),
        }
    }
}

/// Task board — a simple kanban-style list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBoard {
    pub tasks: Vec<BoardTask>,
}

#[allow(dead_code)]
impl TaskBoard {
    pub fn add_task(&mut self, id: &str, title: &str) {
        self.tasks.push(BoardTask::new(id, title));
    }

    pub fn update_status(&mut self, id: &str, status: TaskStatus) -> Result<(), String> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("task '{}' not found", id))?;
        task.status = status;
        Ok(())
    }

    pub fn remove_task(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| format!("task '{}' not found", id))?;
        self.tasks.remove(idx);
        Ok(())
    }

    pub fn todo_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Todo).count()
    }

    pub fn done_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Done).count()
    }
}

/// The agent desk — a virtual workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDesk {
    pub sticky_notes: Vec<StickyNote>,
    pub pinned_files: Vec<PinnedFile>,
    pub task_board: TaskBoard,
    pub workspace_path: PathBuf,
    #[serde(skip)]
    #[allow(dead_code)]
    preview_lines: usize,
}

#[allow(dead_code)]
impl AgentDesk {
    pub fn new(workspace_path: PathBuf) -> Self {
        Self {
            sticky_notes: Vec::new(),
            pinned_files: Vec::new(),
            task_board: TaskBoard::default(),
            workspace_path,
            preview_lines: 20,
        }
    }

    // -- Sticky Notes --

    pub fn add_note(&mut self, content: &str, color: NoteColor) -> String {
        let id = format!("note-{}", self.sticky_notes.len() + 1);
        self.sticky_notes
            .push(StickyNote::new(&id, content, color));
        id
    }

    pub fn edit_note(&mut self, id: &str, content: &str) -> Result<(), String> {
        let note = self
            .sticky_notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("note '{}' not found", id))?;
        note.content = content.to_string();
        note.updated_at = Utc::now();
        Ok(())
    }

    pub fn remove_note(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .sticky_notes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| format!("note '{}' not found", id))?;
        self.sticky_notes.remove(idx);
        Ok(())
    }

    pub fn toggle_note_pin(&mut self, id: &str) -> Result<bool, String> {
        let note = self
            .sticky_notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("note '{}' not found", id))?;
        note.pinned = !note.pinned;
        Ok(note.pinned)
    }

    // -- Pinned Files --

    pub fn pin_file(&mut self, path: &Path) -> Result<(), String> {
        if self.pinned_files.iter().any(|f| f.path == path) {
            return Err(format!("file '{}' is already pinned", path.display()));
        }
        let pf = PinnedFile::new(path, self.preview_lines)
            .map_err(|e| format!("failed to read file: {}", e))?;
        self.pinned_files.push(pf);
        Ok(())
    }

    pub fn unpin_file(&mut self, path: &Path) -> Result<(), String> {
        let idx = self
            .pinned_files
            .iter()
            .position(|f| f.path == path)
            .ok_or_else(|| format!("file '{}' is not pinned", path.display()))?;
        self.pinned_files.remove(idx);
        Ok(())
    }

    pub fn refresh_pinned_files(&mut self) {
        for pf in &mut self.pinned_files {
            let _ = pf.refresh(self.preview_lines);
        }
    }

    // -- Task Board --

    pub fn add_task(&mut self, title: &str) -> String {
        let id = format!("task-{}", self.task_board.tasks.len() + 1);
        self.task_board.add_task(&id, title);
        id
    }

    pub fn complete_task(&mut self, id: &str) -> Result<(), String> {
        self.task_board.update_status(id, TaskStatus::Done)
    }

    pub fn start_task(&mut self, id: &str) -> Result<(), String> {
        self.task_board.update_status(id, TaskStatus::InProgress)
    }

    // -- Persistence --

    /// Save desk state to a JSON file.
    pub fn save(&self) -> Result<(), String> {
        let dir = self.workspace_path.join("desks");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create desks dir: {}", e))?;
        let path = dir.join("desk.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize desk: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write desk: {}", e))?;
        Ok(())
    }

    /// Load desk state from a JSON file.
    pub fn load(workspace_path: &Path) -> Self {
        let path = workspace_path.join("desks").join("desk.json");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(mut desk) = serde_json::from_str::<AgentDesk>(&content) {
                desk.workspace_path = workspace_path.to_path_buf();
                desk.preview_lines = 20;
                return desk;
            }
        }
        Self::new(workspace_path.to_path_buf())
    }

    // -- Display --

    pub fn format_desk(&self) -> String {
        let mut lines = Vec::new();

        // Sticky notes
        if !self.sticky_notes.is_empty() {
            lines.push("Sticky Notes:".to_string());
            for note in &self.sticky_notes {
                let pin = if note.pinned { " [pinned]" } else { "" };
                lines.push(format!(
                    "  {} {} ({}){}",
                    note.id, note.content, note.color.as_str(), pin
                ));
            }
            lines.push(String::new());
        }

        // Pinned files
        if !self.pinned_files.is_empty() {
            lines.push("Pinned Files:".to_string());
            for pf in &self.pinned_files {
                let dirty = if pf.dirty { " (modified)" } else { "" };
                lines.push(format!(
                    "  {} ({} lines){}",
                    pf.path.display(),
                    pf.line_count,
                    dirty
                ));
            }
            lines.push(String::new());
        }

        // Task board
        if !self.task_board.tasks.is_empty() {
            lines.push(format!(
                "Task Board ({}/{} done):",
                self.task_board.done_count(),
                self.task_board.tasks.len()
            ));
            for task in &self.task_board.tasks {
                lines.push(format!(
                    "  {} {} {}",
                    task.status.checkbox(),
                    task.id,
                    task.title
                ));
            }
        }

        if lines.is_empty() {
            "Desk is empty. Use /note add, /pin, or task_create to populate.".to_string()
        } else {
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_desk() -> AgentDesk {
        AgentDesk::new(PathBuf::from("/tmp/test-workspace"))
    }

    #[test]
    fn test_add_note() {
        let mut desk = test_desk();
        let id = desk.add_note("Fix the bug", NoteColor::Yellow);
        assert_eq!(desk.sticky_notes.len(), 1);
        assert_eq!(desk.sticky_notes[0].content, "Fix the bug");
        assert!(id.starts_with("note-"));
    }

    #[test]
    fn test_edit_note() {
        let mut desk = test_desk();
        let id = desk.add_note("Old text", NoteColor::Blue);
        desk.edit_note(&id, "New text").unwrap();
        assert_eq!(desk.sticky_notes[0].content, "New text");
    }

    #[test]
    fn test_remove_note() {
        let mut desk = test_desk();
        let id = desk.add_note("Temp", NoteColor::Red);
        desk.remove_note(&id).unwrap();
        assert!(desk.sticky_notes.is_empty());
    }

    #[test]
    fn test_toggle_note_pin() {
        let mut desk = test_desk();
        let id = desk.add_note("Important", NoteColor::Green);
        assert!(desk.toggle_note_pin(&id).unwrap());
        assert!(desk.sticky_notes[0].pinned);
        assert!(!desk.toggle_note_pin(&id).unwrap());
    }

    #[test]
    fn test_note_not_found() {
        let mut desk = test_desk();
        assert!(desk.edit_note("nonexistent", "text").is_err());
        assert!(desk.remove_note("nonexistent").is_err());
    }

    #[test]
    fn test_note_color_from_str() {
        assert_eq!(NoteColor::from_str("blue"), NoteColor::Blue);
        assert_eq!(NoteColor::from_str("RED"), NoteColor::Red);
        assert_eq!(NoteColor::from_str("unknown"), NoteColor::Yellow);
    }

    #[test]
    fn test_task_board() {
        let mut desk = test_desk();
        let id = desk.add_task("Write tests");
        assert_eq!(desk.task_board.tasks.len(), 1);
        assert_eq!(desk.task_board.todo_count(), 1);

        desk.start_task(&id).unwrap();
        assert_eq!(desk.task_board.tasks[0].status, TaskStatus::InProgress);

        desk.complete_task(&id).unwrap();
        assert_eq!(desk.task_board.done_count(), 1);
    }

    #[test]
    fn test_task_board_remove() {
        let mut board = TaskBoard::default();
        board.add_task("t1", "Task 1");
        board.add_task("t2", "Task 2");
        board.remove_task("t1").unwrap();
        assert_eq!(board.tasks.len(), 1);
        assert_eq!(board.tasks[0].id, "t2");
    }

    #[test]
    fn test_task_not_found() {
        let mut board = TaskBoard::default();
        assert!(board.update_status("nope", TaskStatus::Done).is_err());
        assert!(board.remove_task("nope").is_err());
    }

    #[test]
    fn test_format_empty_desk() {
        let desk = test_desk();
        let s = desk.format_desk();
        assert!(s.contains("empty"));
    }

    #[test]
    fn test_format_desk_with_content() {
        let mut desk = test_desk();
        desk.add_note("Fix auth", NoteColor::Yellow);
        desk.add_task("Write tests");
        let s = desk.format_desk();
        assert!(s.contains("Fix auth"));
        assert!(s.contains("Write tests"));
    }

    #[test]
    fn test_pinned_file_from_string() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "line 1\nline 2\nline 3\n").unwrap();
        let pf = PinnedFile::new(tmp.path(), 2).unwrap();
        assert_eq!(pf.line_count, 3);
        assert_eq!(pf.preview, "line 1\nline 2");
    }

    #[test]
    fn test_pin_file_duplicate() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "content").unwrap();
        let mut desk = test_desk();
        desk.pin_file(tmp.path()).unwrap();
        assert!(desk.pin_file(tmp.path()).is_err());
    }

    #[test]
    fn test_unpin_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "content").unwrap();
        let mut desk = test_desk();
        desk.pin_file(tmp.path()).unwrap();
        desk.unpin_file(tmp.path()).unwrap();
        assert!(desk.pinned_files.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let mut desk = AgentDesk::new(tmp.path().to_path_buf());
        desk.add_note("Test note", NoteColor::Blue);
        desk.add_task("Test task");
        desk.save().unwrap();

        let loaded = AgentDesk::load(tmp.path());
        assert_eq!(loaded.sticky_notes.len(), 1);
        assert_eq!(loaded.sticky_notes[0].content, "Test note");
        assert_eq!(loaded.task_board.tasks.len(), 1);
    }
}
