//! File locking — prevent concurrent edit conflicts.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Type of file lock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LockType {
    Exclusive,
    Shared,
}

/// A file lock entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLock {
    pub path: PathBuf,
    pub owner: String,
    pub lock_type: LockType,
    pub locked_at: chrono::DateTime<chrono::Utc>,
}

/// Manages file locks to prevent concurrent edits.
pub struct FileLockManager {
    locks: HashMap<PathBuf, Vec<FileLock>>,
}

impl Default for FileLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileLockManager {
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
        }
    }

    /// Acquire a lock on a file. Returns an error if already exclusively locked by another owner.
    pub fn acquire(
        &mut self,
        path: PathBuf,
        owner: String,
        lock_type: LockType,
    ) -> MaixResult<()> {
        let entry = self.locks.entry(path.clone()).or_default();

        // Check for conflicting locks
        for existing in entry.iter() {
            if existing.owner == owner {
                return Err(maix_core::MaixError::Tool(format!(
                    "File already locked by this session: {}",
                    path.display()
                )));
            }
            if existing.lock_type == LockType::Exclusive || lock_type == LockType::Exclusive {
                return Err(maix_core::MaixError::Tool(format!(
                    "File locked by '{}': {}",
                    existing.owner,
                    path.display()
                )));
            }
        }

        entry.push(FileLock {
            path,
            owner,
            lock_type,
            locked_at: chrono::Utc::now(),
        });
        Ok(())
    }

    /// Release a lock on a file.
    pub fn release(&mut self, path: &Path, owner: &str) -> MaixResult<()> {
        if let Some(locks) = self.locks.get_mut(path) {
            let before = locks.len();
            locks.retain(|l| l.owner != owner);
            if locks.len() == before {
                return Err(maix_core::MaixError::Tool(format!(
                    "No lock found for owner '{}' on {}",
                    owner,
                    path.display()
                )));
            }
            if locks.is_empty() {
                self.locks.remove(path);
            }
            Ok(())
        } else {
            Err(maix_core::MaixError::Tool(format!(
                "No locks on {}",
                path.display()
            )))
        }
    }

    /// Release all locks owned by a session.
    pub fn release_all(&mut self, owner: &str) {
        for locks in self.locks.values_mut() {
            locks.retain(|l| l.owner != owner);
        }
        self.locks.retain(|_, locks| !locks.is_empty());
    }

    /// Check if a file is locked.
    pub fn is_locked(&self, path: &Path) -> bool {
        self.locks
            .get(path)
            .map(|l| !l.is_empty())
            .unwrap_or(false)
    }

    /// Get lock info for a file.
    pub fn get_locks(&self, path: &Path) -> Vec<&FileLock> {
        self.locks
            .get(path)
            .map(|l| l.iter().collect())
            .unwrap_or_default()
    }

    /// List all active locks.
    pub fn list_all(&self) -> Vec<&FileLock> {
        self.locks.values().flatten().collect()
    }

    /// Format all locks for display.
    pub fn format_locks(&self) -> String {
        let all = self.list_all();
        if all.is_empty() {
            return "No active file locks.".into();
        }
        let mut lines = vec![format!("Active file locks ({}):", all.len())];
        for lock in &all {
            let ltype = match lock.lock_type {
                LockType::Exclusive => "EXCLUSIVE",
                LockType::Shared => "SHARED",
            };
            lines.push(format!(
                "  [{}] {} by '{}' ({})",
                ltype,
                lock.path.display(),
                lock.owner,
                lock.locked_at.format("%H:%M:%S")
            ));
        }
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Lock a file to prevent concurrent edits.
pub struct FileLockTool(pub Arc<Mutex<FileLockManager>>);

#[async_trait]
impl Tool for FileLockTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "file_lock".into(),
            description: "Lock a file to prevent concurrent edits by other sessions.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to lock" },
                    "lock_type": { "type": "string", "description": "Lock type: 'exclusive' or 'shared' (default: exclusive)", "enum": ["exclusive", "shared"] }
                },
                "required": ["path"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let lock_type = match args["lock_type"].as_str().unwrap_or("exclusive") {
            "shared" => LockType::Shared,
            _ => LockType::Exclusive,
        };

        let path = ctx.working_dir.join(path_str);
        let mut mgr = self.0.lock().await;
        let lt = lock_type.clone();
        mgr.acquire(path.clone(), ctx.session_id.clone(), lock_type)?;
        Ok(format!("Locked: {} ({:?})", path_str, lt))
    }
}

/// Unlock a previously locked file.
pub struct FileUnlockTool(pub Arc<Mutex<FileLockManager>>);

#[async_trait]
impl Tool for FileUnlockTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "file_unlock".into(),
            description: "Release a file lock acquired by this session.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to unlock" }
                },
                "required": ["path"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let path = ctx.working_dir.join(path_str);
        let mut mgr = self.0.lock().await;
        mgr.release(&path, &ctx.session_id)?;
        Ok(format!("Unlocked: {}", path_str))
    }
}

/// List all active file locks.
pub struct FileLocksTool(pub Arc<Mutex<FileLockManager>>);

#[async_trait]
impl Tool for FileLocksTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "file_locks".into(),
            description: "List all active file locks.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mgr = self.0.lock().await;
        Ok(mgr.format_locks())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_and_release() {
        let mut mgr = FileLockManager::new();
        let path = PathBuf::from("/tmp/test.rs");

        mgr.acquire(path.clone(), "session-1".into(), LockType::Exclusive)
            .unwrap();
        assert!(mgr.is_locked(&path));

        mgr.release(&path, "session-1").unwrap();
        assert!(!mgr.is_locked(&path));
    }

    #[test]
    fn test_conflict_detection() {
        let mut mgr = FileLockManager::new();
        let path = PathBuf::from("/tmp/test.rs");

        mgr.acquire(path.clone(), "session-1".into(), LockType::Exclusive)
            .unwrap();

        let result = mgr.acquire(path.clone(), "session-2".into(), LockType::Exclusive);
        assert!(result.is_err());
    }

    #[test]
    fn test_shared_locks() {
        let mut mgr = FileLockManager::new();
        let path = PathBuf::from("/tmp/test.rs");

        mgr.acquire(path.clone(), "session-1".into(), LockType::Shared)
            .unwrap();
        mgr.acquire(path.clone(), "session-2".into(), LockType::Shared)
            .unwrap();

        assert!(mgr.is_locked(&path));
        assert_eq!(mgr.get_locks(&path).len(), 2);
    }

    #[test]
    fn test_release_all() {
        let mut mgr = FileLockManager::new();
        mgr.acquire("/a".into(), "s1".into(), LockType::Exclusive)
            .unwrap();
        mgr.acquire("/b".into(), "s1".into(), LockType::Exclusive)
            .unwrap();
        mgr.acquire("/c".into(), "s2".into(), LockType::Exclusive)
            .unwrap();

        mgr.release_all("s1");
        assert!(!mgr.is_locked(Path::new("/a")));
        assert!(!mgr.is_locked(Path::new("/b")));
        assert!(mgr.is_locked(Path::new("/c")));
    }
}
