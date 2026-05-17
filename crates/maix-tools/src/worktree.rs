//! Worktree isolation — create, enter, and exit git worktrees for isolated work.
//!
//! Equivalent to Claude Code's EnterWorktree/ExitWorktree functionality.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::PathBuf;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Information about a worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
}

/// List all worktrees in the current repository.
pub async fn list_worktrees(working_dir: &std::path::Path) -> MaixResult<Vec<WorktreeInfo>> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(working_dir)
        .output()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("git worktree list: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(maix_core::MaixError::Tool(format!("git worktree list failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch = None;
    let mut current_head = None;

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            // Save previous entry if exists
            if let Some(path) = current_path.take() {
                worktrees.push(WorktreeInfo {
                    name: path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    path,
                    branch: current_branch.take().unwrap_or_else(|| "detached".into()),
                    head: current_head.take().unwrap_or_default(),
                });
            }
            current_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            current_head = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            current_branch = Some(rest.to_string());
        }
    }

    // Save last entry
    if let Some(path) = current_path {
        worktrees.push(WorktreeInfo {
            name: path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into()),
            path,
            branch: current_branch.unwrap_or_else(|| "detached".into()),
            head: current_head.unwrap_or_default(),
        });
    }

    Ok(worktrees)
}

/// Create a new worktree.
pub async fn create_worktree(
    working_dir: &std::path::Path,
    name: &str,
    base_ref: &str,
) -> MaixResult<WorktreeInfo> {
    let worktree_dir = working_dir.join(".maix").join("worktrees").join(name);

    // Create parent directory
    if let Some(parent) = worktree_dir.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| maix_core::MaixError::Tool(format!("mkdir: {e}")))?;
    }

    let branch_name = format!("maix-worktree-{}", name);

    // Determine base ref
    let ref_arg = if base_ref == "head" || base_ref == "HEAD" {
        "HEAD".to_string()
    } else {
        // Use default branch
        let output = tokio::process::Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
            .current_dir(working_dir)
            .output()
            .await;
        match output {
            Ok(o) if o.status.success() => {
                let default_branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
                default_branch.strip_prefix("origin/").unwrap_or(&default_branch).to_string()
            }
            _ => "HEAD".to_string(),
        }
    };

    // Create worktree with new branch
    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", "-b", &branch_name, worktree_dir.to_str().unwrap_or(""), &ref_arg])
        .current_dir(working_dir)
        .output()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("git worktree add: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(maix_core::MaixError::Tool(format!("git worktree add failed: {}", stderr)));
    }

    Ok(WorktreeInfo {
        name: name.to_string(),
        path: worktree_dir,
        branch: branch_name,
        head: String::new(),
    })
}

/// Remove a worktree.
pub async fn remove_worktree(
    working_dir: &std::path::Path,
    name: &str,
    force: bool,
) -> MaixResult<String> {
    let worktree_dir = working_dir.join(".maix").join("worktrees").join(name);

    if !worktree_dir.exists() {
        return Err(maix_core::MaixError::Tool(format!("worktree '{}' not found", name)));
    }

    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(worktree_dir.to_str().unwrap_or(""));
    cmd.current_dir(working_dir);

    let output = cmd.output().await
        .map_err(|e| maix_core::MaixError::Tool(format!("git worktree remove: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("dirty") || stderr.contains("modified") {
            return Err(maix_core::MaixError::Tool(format!(
                "worktree '{}' has uncommitted changes. Use force=true to remove anyway.",
                name
            )));
        }
        return Err(maix_core::MaixError::Tool(format!("git worktree remove failed: {}", stderr)));
    }

    // Clean up branch
    let branch_name = format!("maix-worktree-{}", name);
    let _ = tokio::process::Command::new("git")
        .args(["branch", "-D", &branch_name])
        .current_dir(working_dir)
        .output()
        .await;

    Ok(format!("Removed worktree '{}'", name))
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Create a new git worktree for isolated work.
pub struct WorktreeCreateTool;

#[async_trait]
impl Tool for WorktreeCreateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "worktree_create".into(),
            description: "Create a new git worktree for isolated work. Creates a new branch and worktree directory under .maix/worktrees/.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Worktree name (used as directory name and branch suffix)" },
                    "base_ref": { "type": "string", "description": "Base reference: \"fresh\" (default branch) or \"head\" (current HEAD)" }
                },
                "required": ["name"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let name = args["name"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'name'".into()))?;
        let base_ref = args["base_ref"].as_str().unwrap_or("fresh");

        let info = create_worktree(&ctx.working_dir, name, base_ref).await?;

        Ok(format!(
            "Created worktree '{}'\n  path: {}\n  branch: {}",
            info.name,
            info.path.display(),
            info.branch
        ))
    }
}

/// List all git worktrees.
pub struct WorktreeListTool;

#[async_trait]
impl Tool for WorktreeListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "worktree_list".into(),
            description: "List all git worktrees in the current repository.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let worktrees = list_worktrees(&ctx.working_dir).await?;

        if worktrees.is_empty() {
            return Ok("No worktrees found.".to_string());
        }

        let mut lines = vec![format!("Worktrees ({}):", worktrees.len())];
        for wt in &worktrees {
            lines.push(format!("  {} — {} [{}]", wt.name, wt.path.display(), wt.branch));
        }
        Ok(lines.join("\n"))
    }
}

/// Exit (remove) a git worktree.
pub struct WorktreeExitTool;

#[async_trait]
impl Tool for WorktreeExitTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "worktree_exit".into(),
            description: "Exit and remove a git worktree. Use force=true to remove even with uncommitted changes.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Worktree name to remove" },
                    "force": { "type": "boolean", "description": "Force removal even with uncommitted changes (default: false)" }
                },
                "required": ["name"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let name = args["name"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'name'".into()))?;
        let force = args["force"].as_bool().unwrap_or(false);

        remove_worktree(&ctx.working_dir, name, force).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_info_display() {
        let info = WorktreeInfo {
            name: "test".into(),
            path: PathBuf::from("/home/user/project/.maix/worktrees/test"),
            branch: "maix-worktree-test".into(),
            head: "abc123".into(),
        };
        assert_eq!(info.name, "test");
        assert!(info.path.to_string_lossy().contains("worktrees"));
    }
}
