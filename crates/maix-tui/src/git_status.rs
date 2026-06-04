//! Git status detection for TUI status bar.

use std::process::Command;

/// Git repository status information.
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// Current branch name
    pub branch: String,
    /// Number of modified files
    pub modified: usize,
    /// Number of staged files
    pub staged: usize,
    /// Number of untracked files
    pub untracked: usize,
    /// Whether there are uncommitted changes
    pub dirty: bool,
}

impl GitStatus {
    /// Detect git status from the current directory.
    pub fn detect() -> Option<Self> {
        let branch = git_branch()?;
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .ok()?;
        let status_text = String::from_utf8_lossy(&status_output.stdout);

        let mut modified = 0;
        let mut staged = 0;
        let mut untracked = 0;

        for line in status_text.lines() {
            if line.len() < 2 {
                continue;
            }
            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');

            if index_status == '?' && worktree_status == '?' {
                untracked += 1;
            } else {
                if index_status != ' ' && index_status != '?' {
                    staged += 1;
                }
                if worktree_status != ' ' && worktree_status != '?' {
                    modified += 1;
                }
            }
        }

        let dirty = modified > 0 || staged > 0 || untracked > 0;

        Some(Self {
            branch,
            modified,
            staged,
            untracked,
            dirty,
        })
    }

    /// Format for display in status bar.
    pub fn display(&self) -> String {
        if self.branch.is_empty() {
            return String::new();
        }
        let dirty_marker = if self.dirty { "*" } else { "" };
        let mut parts = vec![format!("{}{}", self.branch, dirty_marker)];
        if self.staged > 0 {
            parts.push(format!("+{}", self.staged));
        }
        if self.modified > 0 {
            parts.push(format!("~{}", self.modified));
        }
        if self.untracked > 0 {
            parts.push(format!("?{}", self.untracked));
        }
        parts.join(" ")
    }
}

/// Get the current git branch name.
fn git_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() {
            None
        } else {
            Some(branch)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_default() {
        let status = GitStatus::default();
        assert!(status.branch.is_empty());
        assert_eq!(status.modified, 0);
        assert!(!status.dirty);
    }

    #[test]
    fn test_display_empty() {
        let status = GitStatus::default();
        assert_eq!(status.display(), "");
    }

    #[test]
    fn test_display_clean() {
        let status = GitStatus {
            branch: "main".to_string(),
            ..Default::default()
        };
        assert_eq!(status.display(), "main");
    }

    #[test]
    fn test_display_dirty() {
        let status = GitStatus {
            branch: "feature".to_string(),
            modified: 2,
            staged: 1,
            untracked: 3,
            dirty: true,
        };
        assert_eq!(status.display(), "feature* +1 ~2 ?3");
    }

    #[test]
    fn test_display_staged_only() {
        let status = GitStatus {
            branch: "main".to_string(),
            staged: 5,
            dirty: true,
            ..Default::default()
        };
        assert_eq!(status.display(), "main* +5");
    }
}
