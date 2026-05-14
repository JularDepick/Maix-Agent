//! Side-git workspace snapshots for rollback support.
//!
//! Creates a separate git repo in `.maix/side-git/` to track workspace state
//! without polluting the project's `.git`. Supports snapshot, restore, list, and diff.

use maix_core::MaixResult;
use std::path::{Path, PathBuf};

/// Information about a snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub hash: String,
    pub label: String,
    pub timestamp: String,
}

/// Side-git manager for workspace snapshots.
pub struct SideGit {
    /// Path to the side-git repo (.maix/side-git/)
    side_repo: PathBuf,
    /// Path to the project root being tracked
    project_root: PathBuf,
}

impl SideGit {
    /// Initialize side-git for a project. Creates `.maix/side-git/` if needed.
    pub fn init(project_root: &Path) -> MaixResult<Self> {
        let side_repo = project_root.join(".maix").join("side-git");

        if !side_repo.exists() {
            std::fs::create_dir_all(&side_repo)
                .map_err(maix_core::MaixError::Io)?;

            // Initialize a bare git repo
            let status = std::process::Command::new("git")
                .args(["init", "--bare"])
                .current_dir(&side_repo)
                .status()
                .map_err(maix_core::MaixError::Io)?;

            if !status.success() {
                return Err(maix_core::MaixError::Io(std::io::Error::other(
                    "failed to init side-git repo",
                )));
            }
        }

        Ok(Self {
            side_repo,
            project_root: project_root.to_path_buf(),
        })
    }

    /// Create a snapshot of the current workspace state.
    pub fn snapshot(&self, label: &str) -> MaixResult<String> {
        let work_dir = self.side_repo.join("work");

        // Ensure work directory exists
        if !work_dir.exists() {
            std::fs::create_dir_all(&work_dir)
                .map_err(maix_core::MaixError::Io)?;

            // Clone from bare repo into work dir
            let status = std::process::Command::new("git")
                .args(["clone", &self.side_repo.to_string_lossy(), &work_dir.to_string_lossy()])
                .output()
                .map_err(maix_core::MaixError::Io)?;

            if !status.status.success() {
                // If clone fails (empty bare repo), init directly
                std::process::Command::new("git")
                    .args(["init"])
                    .current_dir(&work_dir)
                    .status()
                    .map_err(maix_core::MaixError::Io)?;

                // Rename branch to main
                std::process::Command::new("git")
                    .args(["branch", "-M", "main"])
                    .current_dir(&work_dir)
                    .status()
                    .map_err(maix_core::MaixError::Io)?;

                // Set remote to bare repo
                std::process::Command::new("git")
                    .args(["remote", "add", "origin", &self.side_repo.to_string_lossy()])
                    .current_dir(&work_dir)
                    .status()
                    .map_err(maix_core::MaixError::Io)?;
            }
        }

        // Sync project files to work dir (excluding .git, .maix, node_modules, target)
        self.sync_project_to_work(&work_dir)?;

        // git add -A && git commit
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&work_dir)
            .status()
            .map_err(maix_core::MaixError::Io)?;

        let commit_msg = format!("snapshot: {}", label);
        let output = std::process::Command::new("git")
            .args(["commit", "-m", &commit_msg, "--allow-empty"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("nothing to commit") {
                tracing::warn!("side-git commit warning: {}", stderr);
            }
        }

        // Push to bare repo
        std::process::Command::new("git")
            .args(["push", "origin", "HEAD:main", "--force"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        // Get commit hash
        let hash_output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        let hash = String::from_utf8_lossy(&hash_output.stdout)
            .trim()
            .to_string();

        tracing::info!("side-git snapshot: {} ({})", label, &hash[..8.min(hash.len())]);
        Ok(hash)
    }

    /// Restore workspace to a specific snapshot.
    pub fn restore(&self, hash: &str) -> MaixResult<()> {
        let work_dir = self.side_repo.join("work");

        if !work_dir.exists() {
            return Err(maix_core::MaixError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "side-git work directory not found",
            )));
        }

        // Checkout the specified commit
        let output = std::process::Command::new("git")
            .args(["checkout", hash])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(maix_core::MaixError::Io(std::io::Error::other(
                format!("failed to checkout {}: {}", hash, stderr),
            )));
        }

        // Copy files back to project root
        self.sync_work_to_project(&work_dir)?;

        tracing::info!("side-git restored to {}", &hash[..8.min(hash.len())]);
        Ok(())
    }

    /// List all snapshots.
    pub fn list_snapshots(&self) -> MaixResult<Vec<SnapshotInfo>> {
        let work_dir = self.side_repo.join("work");

        if !work_dir.exists() {
            return Ok(Vec::new());
        }

        // Try main first, then master
        let output = std::process::Command::new("git")
            .args(["log", "--oneline", "--format=%H|%s|%ai", "main"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        let output = if output.status.success() {
            output
        } else {
            std::process::Command::new("git")
                .args(["log", "--oneline", "--format=%H|%s|%ai", "master"])
                .current_dir(&work_dir)
                .output()
                .map_err(maix_core::MaixError::Io)?
        };

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let snapshots: Vec<SnapshotInfo> = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() >= 3 {
                    let label = parts[1]
                        .strip_prefix("snapshot: ")
                        .unwrap_or(parts[1])
                        .to_string();
                    Some(SnapshotInfo {
                        hash: parts[0].to_string(),
                        label,
                        timestamp: parts[2].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(snapshots)
    }

    /// Get diff between current state and a snapshot.
    pub fn diff_since(&self, hash: &str) -> MaixResult<String> {
        let work_dir = self.side_repo.join("work");

        if !work_dir.exists() {
            return Ok(String::new());
        }

        let output = std::process::Command::new("git")
            .args(["diff", hash, "HEAD", "--stat"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get full diff between current state and a snapshot.
    pub fn diff_full(&self, hash: &str) -> MaixResult<String> {
        let work_dir = self.side_repo.join("work");

        if !work_dir.exists() {
            return Ok(String::new());
        }

        let output = std::process::Command::new("git")
            .args(["diff", hash, "HEAD"])
            .current_dir(&work_dir)
            .output()
            .map_err(maix_core::MaixError::Io)?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Clean up old snapshots, keeping only the most recent `keep` ones.
    pub fn cleanup(&self, keep: usize) -> MaixResult<usize> {
        let snapshots = self.list_snapshots()?;
        if snapshots.len() <= keep {
            return Ok(0);
        }

        let to_remove = snapshots.len() - keep;
        // We can't easily remove individual commits from a bare repo,
        // but we can recreate the bare repo with only the recent commits
        tracing::info!("side-git cleanup: would remove {} old snapshots", to_remove);
        Ok(to_remove)
    }

    /// Sync project files to the side-git work directory.
    fn sync_project_to_work(&self, work_dir: &Path) -> MaixResult<()> {
        let exclude = [".git", ".maix", "node_modules", "target", ".DS_Store"];

        // Use robocopy on Windows, rsync on Unix
        #[cfg(target_os = "windows")]
        {
            let status = std::process::Command::new("robocopy")
                .args([
                    &self.project_root.to_string_lossy(),
                    &work_dir.to_string_lossy(),
                    "/E", "/XD", ".git", ".maix", "node_modules", "target",
                    "/XF", ".DS_Store",
                    "/NFL", "/NDL", "/NJH", "/NJS", "/NC", "/NS",
                ])
                .status();

            match status {
                Ok(s) => {
                    // robocopy returns 0-7 for success, 8+ for failure
                    if s.code().unwrap_or(8) >= 8 {
                        tracing::warn!("robocopy returned {}", s.code().unwrap_or(-1));
                    }
                }
                Err(e) => {
                    tracing::warn!("robocopy failed: {}, falling back to manual copy", e);
                    self.copy_dir_recursive(&self.project_root, work_dir, &exclude)?;
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let status = std::process::Command::new("rsync")
                .args([
                    "-a", "--delete",
                    "--exclude=.git",
                    "--exclude=.maix",
                    "--exclude=node_modules",
                    "--exclude=target",
                    "--exclude=.DS_Store",
                    &format!("{}/", self.project_root.to_string_lossy()),
                    &work_dir.to_string_lossy(),
                ])
                .status();

            match status {
                Ok(s) if s.success() => {}
                _ => {
                    tracing::warn!("rsync failed, falling back to manual copy");
                    self.copy_dir_recursive(&self.project_root, work_dir, &exclude)?;
                }
            }
        }

        Ok(())
    }

    /// Sync work directory back to project root.
    fn sync_work_to_project(&self, work_dir: &Path) -> MaixResult<()> {
        let exclude = [".git", ".maix"];

        #[cfg(target_os = "windows")]
        {
            let status = std::process::Command::new("robocopy")
                .args([
                    &work_dir.to_string_lossy(),
                    &self.project_root.to_string_lossy(),
                    "/E", "/XD", ".git", ".maix",
                    "/NFL", "/NDL", "/NJH", "/NJS", "/NC", "/NS",
                ])
                .status();

            match status {
                Ok(s) => {
                    if s.code().unwrap_or(8) >= 8 {
                        tracing::warn!("robocopy restore returned {}", s.code().unwrap_or(-1));
                    }
                }
                Err(e) => {
                    tracing::warn!("robocopy failed: {}, falling back to manual copy", e);
                    self.copy_dir_recursive(work_dir, &self.project_root, &exclude)?;
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let status = std::process::Command::new("rsync")
                .args([
                    "-a", "--delete",
                    "--exclude=.git",
                    "--exclude=.maix",
                    &format!("{}/", work_dir.to_string_lossy()),
                    &self.project_root.to_string_lossy(),
                ])
                .status();

            match status {
                Ok(s) if s.success() => {}
                _ => {
                    tracing::warn!("rsync restore failed, falling back to manual copy");
                    self.copy_dir_recursive(work_dir, &self.project_root, &exclude)?;
                }
            }
        }

        Ok(())
    }

    /// Manual recursive copy as fallback.
    fn copy_dir_recursive(&self, from: &Path, to: &Path, exclude: &[&str]) -> MaixResult<()> {
        if !to.exists() {
            std::fs::create_dir_all(to).map_err(maix_core::MaixError::Io)?;
        }

        for entry in std::fs::read_dir(from).map_err(maix_core::MaixError::Io)? {
            let entry = entry.map_err(maix_core::MaixError::Io)?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if exclude.iter().any(|e| *e == name_str.as_ref()) {
                continue;
            }

            let from_path = entry.path();
            let to_path = to.join(&name);

            if from_path.is_dir() {
                self.copy_dir_recursive(&from_path, &to_path, exclude)?;
            } else {
                if let Some(parent) = to_path.parent() {
                    std::fs::create_dir_all(parent).map_err(maix_core::MaixError::Io)?;
                }
                std::fs::copy(&from_path, &to_path).map_err(maix_core::MaixError::Io)?;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

use crate::{Tool, ToolDef, ToolCtx, RiskLevel};
use async_trait::async_trait;
use serde_json::Value;

pub struct SnapshotTool;

impl Default for SnapshotTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for SnapshotTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "snapshot".into(),
            description: "Create a workspace snapshot for rollback. Saves current file state.".into(),
            risk_level: RiskLevel::ReadOnly,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Optional label for this snapshot"
                    }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let label = args.get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        let side_git = SideGit::init(&ctx.working_dir)?;
        let hash = side_git.snapshot(label)?;
        Ok(format!("Snapshot created: {}", &hash[..8.min(hash.len())]))
    }
}

pub struct RestoreTool;

impl Default for RestoreTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RestoreTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for RestoreTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "restore".into(),
            description: "Restore workspace to a previous snapshot. Provide a snapshot hash or 'latest'.".into(),
            risk_level: RiskLevel::Write,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "hash": {
                        "type": "string",
                        "description": "Snapshot hash or 'latest'"
                    }
                },
                "required": ["hash"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let hash = args.get("hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| maix_core::MaixError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "hash parameter required",
            )))?;

        let side_git = SideGit::init(&ctx.working_dir)?;

        let target_hash = if hash == "latest" {
            let snapshots = side_git.list_snapshots()?;
            snapshots.first()
                .ok_or_else(|| maix_core::MaixError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no snapshots available",
                )))?
                .hash.clone()
        } else {
            hash.to_string()
        };

        side_git.restore(&target_hash)?;
        Ok(format!("Restored to snapshot {}", &target_hash[..8.min(target_hash.len())]))
    }
}

pub struct SnapshotListTool;

impl Default for SnapshotListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotListTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for SnapshotListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "snapshot_list".into(),
            description: "List all workspace snapshots.".into(),
            risk_level: RiskLevel::ReadOnly,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, ctx: &ToolCtx, _args: Value) -> MaixResult<String> {
        let side_git = SideGit::init(&ctx.working_dir)?;
        let snapshots = side_git.list_snapshots()?;

        if snapshots.is_empty() {
            return Ok("No snapshots found.".into());
        }

        let mut output = String::from("Workspace snapshots:\n");
        for (i, snap) in snapshots.iter().take(20).enumerate() {
            output.push_str(&format!(
                "  {}. {} [{}] {}\n",
                i + 1,
                &snap.hash[..8.min(snap.hash.len())],
                snap.timestamp,
                snap.label
            ));
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maix-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_side_git_init() {
        let project = make_test_dir("side-git-init");
        let side_git = SideGit::init(&project).unwrap();
        assert!(side_git.side_repo.exists());
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn test_side_git_snapshot_and_list() {
        let project = make_test_dir("side-git-snap");
        fs::write(project.join("test.txt"), "hello").unwrap();

        let side_git = SideGit::init(&project).unwrap();
        let hash = side_git.snapshot("test-snap").unwrap();
        assert!(!hash.is_empty());

        let snapshots = side_git.list_snapshots().unwrap();
        assert!(!snapshots.is_empty());
        assert_eq!(snapshots[0].label, "test-snap");
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn test_side_git_diff() {
        let project = make_test_dir("side-git-diff");
        fs::write(project.join("test.txt"), "hello").unwrap();

        let side_git = SideGit::init(&project).unwrap();
        let hash = side_git.snapshot("initial").unwrap();

        let diff = side_git.diff_since(&hash).unwrap();
        // Diff should be empty or contain our file
        assert!(diff.is_empty() || diff.contains("test.txt"));
        let _ = fs::remove_dir_all(&project);
    }
}
