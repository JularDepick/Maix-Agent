//! Session bookmarks — save and restore conversation checkpoints.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A session bookmark snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBookmark {
    pub id: String,
    pub name: String,
    pub description: String,
    pub message_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Manages session bookmarks with persistence.
pub struct BookmarkManager {
    storage_path: PathBuf,
    bookmarks: Vec<SessionBookmark>,
}

impl BookmarkManager {
    pub fn new(storage_path: PathBuf) -> Self {
        let bookmarks = Self::load_from_disk(&storage_path).unwrap_or_default();
        Self {
            storage_path,
            bookmarks,
        }
    }

    pub fn create(&mut self, name: &str, message_count: usize) -> &SessionBookmark {
        let bookmark = SessionBookmark {
            id: format!("bm_{}", uuid::Uuid::new_v4()),
            name: name.to_string(),
            description: format!("{} messages", message_count),
            message_count,
            created_at: chrono::Utc::now(),
        };
        self.bookmarks.push(bookmark);
        self.save_to_disk();
        // SAFETY: we just pushed, so bookmarks is non-empty
        self.bookmarks.last().expect("just pushed a bookmark")
    }

    pub fn get(&self, name: &str) -> Option<&SessionBookmark> {
        self.bookmarks.iter().find(|b| b.name == name)
    }

    pub fn get_by_id(&self, id: &str) -> Option<&SessionBookmark> {
        self.bookmarks.iter().find(|b| b.id == id)
    }

    pub fn list(&self) -> &[SessionBookmark] {
        &self.bookmarks
    }

    pub fn delete(&mut self, name: &str) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|b| b.name != name);
        let deleted = self.bookmarks.len() < before;
        if deleted {
            self.save_to_disk();
        }
        deleted
    }

    pub fn count(&self) -> usize {
        self.bookmarks.len()
    }

    fn save_to_disk(&self) {
        if let Some(parent) = self.storage_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create bookmark dir: {e}");
            }
        }
        if let Ok(data) = serde_json::to_vec_pretty(&self.bookmarks) {
            if let Err(e) = std::fs::write(&self.storage_path, data) {
                tracing::warn!("Failed to save bookmarks: {e}");
            }
        }
    }

    fn load_from_disk(path: &PathBuf) -> Option<Vec<SessionBookmark>> {
        let data = std::fs::read(path).ok()?;
        serde_json::from_slice(&data).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_create() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mut mgr = BookmarkManager::new(path);
        let bm = mgr.create("checkpoint-1", 10);
        assert_eq!(bm.name, "checkpoint-1");
        assert_eq!(bm.message_count, 10);
    }

    #[test]
    fn test_bookmark_list() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mut mgr = BookmarkManager::new(path);
        mgr.create("a", 5);
        mgr.create("b", 10);
        assert_eq!(mgr.list().len(), 2);
    }

    #[test]
    fn test_bookmark_get() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mut mgr = BookmarkManager::new(path);
        mgr.create("test", 42);
        let bm = mgr.get("test").unwrap();
        assert_eq!(bm.message_count, 42);
    }

    #[test]
    fn test_bookmark_delete() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mut mgr = BookmarkManager::new(path);
        mgr.create("temp", 1);
        assert!(mgr.delete("temp"));
        assert_eq!(mgr.count(), 0);
        assert!(!mgr.delete("nonexistent"));
    }

    #[test]
    fn test_bookmark_persistence() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        {
            let mut mgr = BookmarkManager::new(path.clone());
            mgr.create("saved", 20);
        }
        // Reload from disk
        let mgr = BookmarkManager::new(path);
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.get("saved").unwrap().message_count, 20);
    }

    #[test]
    fn test_bookmark_get_by_id() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mut mgr = BookmarkManager::new(path);
        let bm = mgr.create("test", 5);
        let id = bm.id.clone();
        assert!(mgr.get_by_id(&id).is_some());
    }

    #[test]
    fn test_bookmark_empty_list() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bookmarks.json");
        let mgr = BookmarkManager::new(path);
        assert!(mgr.list().is_empty());
    }
}
