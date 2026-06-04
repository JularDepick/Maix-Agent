//! Git tools — status, diff, log via shell commands.

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

// ---------------------------------------------------------------------------
// GitStatusTool
// ---------------------------------------------------------------------------

pub struct GitStatusTool;

impl Default for GitStatusTool {
    fn default() -> Self {
        Self
    }
}

impl GitStatusTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_status".into(),
            description: "Show git repository status: branch, staged/unstaged/untracked files, ahead/behind counts."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let path = ctx.working_dir.join(path_str);

        // Branch info
        let branch = run_git(&path, &["branch", "--show-current"])
            .await
            .unwrap_or_else(|| "detached HEAD".into());

        // Ahead/behind
        let tracking = run_git(&path, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .await
            .unwrap_or_default();

        let mut ahead = 0;
        let mut behind = 0;
        if let Some((a, b)) = tracking.split_once('\t') {
            ahead = a.trim().parse().unwrap_or(0);
            behind = b.trim().parse().unwrap_or(0);
        }

        // Status
        let status = run_git(&path, &["status", "--porcelain=v1"])
            .await
            .unwrap_or_default();

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for line in status.lines() {
            if line.len() < 3 {
                continue;
            }
            let x = line.chars().next().unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let file = &line[3..];

            if x == '?' && y == '?' {
                untracked.push(file.to_string());
            } else {
                if x != ' ' && x != '?' {
                    staged.push(format!("{} {}", x, file));
                }
                if y != ' ' && y != '?' {
                    unstaged.push(format!("{} {}", y, file));
                }
            }
        }

        let mut result = format!("Branch: {branch}");
        if ahead > 0 || behind > 0 {
            result.push_str(&format!(" (ahead {ahead}, behind {behind})"));
        }
        result.push('\n');

        if !staged.is_empty() {
            result.push_str(&format!("\nStaged ({}):\n", staged.len()));
            for f in &staged {
                result.push_str(&format!("  {f}\n"));
            }
        }
        if !unstaged.is_empty() {
            result.push_str(&format!("\nUnstaged ({}):\n", unstaged.len()));
            for f in &unstaged {
                result.push_str(&format!("  {f}\n"));
            }
        }
        if !untracked.is_empty() {
            result.push_str(&format!("\nUntracked ({}):\n", untracked.len()));
            for f in &untracked {
                result.push_str(&format!("  {f}\n"));
            }
        }

        if staged.is_empty() && unstaged.is_empty() && untracked.is_empty() {
            result.push_str("\nWorking tree clean.");
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// GitDiffTool
// ---------------------------------------------------------------------------

pub struct GitDiffTool;

impl Default for GitDiffTool {
    fn default() -> Self {
        Self
    }
}

impl GitDiffTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_diff".into(),
            description: "Show git diff. By default shows unstaged changes; use staged=true for staged changes."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "staged": { "type": "boolean", "description": "Show staged changes (default: false)" },
                    "file": { "type": "string", "description": "Specific file to diff (optional)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let staged = args["staged"].as_bool().unwrap_or(false);
        let file = args["file"].as_str();
        let path = ctx.working_dir.join(path_str);

        if let Some(f) = file {
            // File-specific diff
            let stat_args: Vec<&str> = if staged {
                vec!["diff", "--staged", "--stat", "--", f]
            } else {
                vec!["diff", "--stat", "--", f]
            };
            let stat = run_git(&path, &stat_args).await.unwrap_or_default();

            let diff_args: Vec<&str> = if staged {
                vec!["diff", "--staged", "--", f]
            } else {
                vec!["diff", "--", f]
            };
            let diff = run_git(&path, &diff_args).await.unwrap_or_default();
            let diff = truncate_lines(&diff, 500);

            if stat.is_empty() && diff.is_empty() {
                return Ok("No changes.".into());
            }
            return Ok(format!("{stat}\n\n{diff}"));
        }

        // Full repo diff
        let stat_args: Vec<&str> = if staged {
            vec!["diff", "--staged", "--stat"]
        } else {
            vec!["diff", "--stat"]
        };
        let stat = run_git(&path, &stat_args).await.unwrap_or_default();

        let diff_args: Vec<&str> = if staged { vec!["diff", "--staged"] } else { vec!["diff"] };
        let diff = run_git(&path, &diff_args).await.unwrap_or_default();
        let diff = truncate_lines(&diff, 500);

        if stat.is_empty() && diff.is_empty() {
            Ok("No changes.".into())
        } else {
            Ok(format!("{stat}\n\n{diff}"))
        }
    }
}

// ---------------------------------------------------------------------------
// GitLogTool
// ---------------------------------------------------------------------------

pub struct GitLogTool;

impl Default for GitLogTool {
    fn default() -> Self {
        Self
    }
}

impl GitLogTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitLogTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_log".into(),
            description: "Show recent git log with hash, author, date, and message.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "limit": { "type": "integer", "description": "Number of commits to show (default: 20)" },
                    "file": { "type": "string", "description": "Show log for specific file (optional)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let limit = args["limit"].as_u64().unwrap_or(20);
        let file = args["file"].as_str();
        let path = ctx.working_dir.join(path_str);

        let limit_str = limit.to_string();
        let mut cmd_args = vec![
            "log",
            &limit_str,
            "--format=%h %ad %an: %s",
            "--date=short",
        ];

        if let Some(f) = file {
            cmd_args.push("--");
            let mut full_args = cmd_args.clone();
            full_args.push(f);
            let log = run_git(&path, &full_args).await.unwrap_or_default();
            return Ok(if log.is_empty() { "No commits found.".into() } else { log });
        }

        let log = run_git(&path, &cmd_args).await.unwrap_or_default();
        Ok(if log.is_empty() { "No commits found.".into() } else { log })
    }
}

// ---------------------------------------------------------------------------
// GitBlameTool
// ---------------------------------------------------------------------------

pub struct GitBlameTool;

impl Default for GitBlameTool {
    fn default() -> Self {
        Self
    }
}

impl GitBlameTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitBlameTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_blame".into(),
            description: "Show git blame for a file: who last modified each line and when.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "file": { "type": "string", "description": "File to blame (required)" },
                    "offset": { "type": "integer", "description": "Start line number (1-based, optional)" },
                    "limit": { "type": "integer", "description": "Number of lines to show (default: 100)" }
                },
                "required": ["file"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let file = args["file"].as_str().unwrap_or_default();
        let offset = args["offset"].as_u64().unwrap_or(1);
        let limit = args["limit"].as_u64().unwrap_or(100);
        let path = ctx.working_dir.join(path_str);

        let range = format!("{}L,{}L", offset, offset + limit - 1);
        let output = run_git(&path, &["blame", "-L", &range, "--porcelain", file]).await;

        match output {
            Some(text) if !text.is_empty() => {
                // Simplify porcelain output to readable format
                let mut result = String::new();
                let mut current_hash = String::new();
                let mut current_author = String::new();
                let mut current_date = String::new();

                for line in text.lines() {
                    if line.len() >= 40 && line.chars().take(40).all(|c| c.is_ascii_hexdigit()) {
                        // Commit hash line
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 && parts[0].len() >= 8 {
                            current_hash = parts[0][..8].to_string();
                        }
                    } else if let Some(val) = line.strip_prefix("author ") {
                        current_author = val.to_string();
                    } else if let Some(val) = line.strip_prefix("author-time ") {
                        let ts: i64 = val.parse().unwrap_or(0);
                        current_date = chrono::DateTime::from_timestamp(ts, 0)
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default();
                    } else if let Some(code) = line.strip_prefix('\t') {
                        // Actual code line
                        result.push_str(&format!(
                            "{} {} {}: {}\n",
                            current_hash,
                            current_date,
                            current_author,
                            code
                        ));
                    }
                }

                if result.is_empty() {
                    Ok("No blame output. Check file path.".into())
                } else {
                    Ok(result)
                }
            }
            _ => Ok(format!("Could not blame {file}. Is it tracked by git?")),
        }
    }
}

// ---------------------------------------------------------------------------
// GitAddTool
// ---------------------------------------------------------------------------

pub struct GitAddTool;

impl Default for GitAddTool {
    fn default() -> Self {
        Self
    }
}

impl GitAddTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitAddTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_add".into(),
            description: "Stage files for commit. Use files=[\".\"] to stage all changes.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Files to stage (e.g., [\"src/main.rs\"] or [\".\"] for all)"
                    }
                },
                "required": ["files"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let files: Vec<&str> = args["files"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if files.is_empty() {
            return Err(maix_core::MaixError::Tool("git_add: no files specified".into()));
        }

        let path = ctx.working_dir.join(path_str);
        let mut cmd_args = vec!["add"];
        for f in &files {
            cmd_args.push(f);
        }

        let output = tokio::process::Command::new("git")
            .args(&cmd_args)
            .current_dir(&path)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("git_add: {e}")))?;

        if output.status.success() {
            Ok(format!("Staged {} file(s): {}", files.len(), files.join(", ")))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(maix_core::MaixError::Tool(format!("git_add failed: {stderr}")))
        }
    }
}

// ---------------------------------------------------------------------------
// GitCommitTool
// ---------------------------------------------------------------------------

pub struct GitCommitTool;

impl Default for GitCommitTool {
    fn default() -> Self {
        Self
    }
}

impl GitCommitTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_commit".into(),
            description: "Create a git commit with a message. Files must be staged first with git_add."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "message": { "type": "string", "description": "Commit message (required)" },
                    "amend": { "type": "boolean", "description": "Amend the last commit (default: false)" }
                },
                "required": ["message"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let message = args["message"].as_str().unwrap_or_default();
        let amend = args["amend"].as_bool().unwrap_or(false);

        if message.is_empty() {
            return Err(maix_core::MaixError::Tool("git_commit: message is required".into()));
        }

        let path = ctx.working_dir.join(path_str);
        let mut cmd_args = vec!["commit", "-m", message];
        if amend {
            cmd_args.push("--amend");
        }

        let output = tokio::process::Command::new("git")
            .args(&cmd_args)
            .current_dir(&path)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("git_commit: {e}")))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!("Committed successfully.\n{stdout}"))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(maix_core::MaixError::Tool(format!("git_commit failed: {stderr}")))
        }
    }
}

// ---------------------------------------------------------------------------
// GitBranchTool
// ---------------------------------------------------------------------------

pub struct GitBranchTool;

impl Default for GitBranchTool {
    fn default() -> Self {
        Self
    }
}

impl GitBranchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitBranchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_branch".into(),
            description: "Manage git branches: list, create, switch, delete.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "action": {
                        "type": "string",
                        "enum": ["list", "create", "switch", "delete"],
                        "description": "Action to perform (default: list)"
                    },
                    "name": { "type": "string", "description": "Branch name (required for create/switch/delete)" }
                }
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let action = args["action"].as_str().unwrap_or("list");
        let name = args["name"].as_str().unwrap_or("");
        let path = ctx.working_dir.join(path_str);

        match action {
            "list" => {
                let output = tokio::process::Command::new("git")
                    .args(["branch", "-a"])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_branch: {e}")))?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_branch list failed: {stderr}")))
                }
            }
            "create" => {
                if name.is_empty() {
                    return Err(maix_core::MaixError::Tool("git_branch create: name is required".into()));
                }
                let output = tokio::process::Command::new("git")
                    .args(["checkout", "-b", name])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_branch: {e}")))?;

                if output.status.success() {
                    Ok(format!("Created and switched to branch '{name}'"))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_branch create failed: {stderr}")))
                }
            }
            "switch" => {
                if name.is_empty() {
                    return Err(maix_core::MaixError::Tool("git_branch switch: name is required".into()));
                }
                let output = tokio::process::Command::new("git")
                    .args(["checkout", name])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_branch: {e}")))?;

                if output.status.success() {
                    Ok(format!("Switched to branch '{name}'"))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_branch switch failed: {stderr}")))
                }
            }
            "delete" => {
                if name.is_empty() {
                    return Err(maix_core::MaixError::Tool("git_branch delete: name is required".into()));
                }
                let output = tokio::process::Command::new("git")
                    .args(["branch", "-d", name])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_branch: {e}")))?;

                if output.status.success() {
                    Ok(format!("Deleted branch '{name}'"))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_branch delete failed: {stderr}")))
                }
            }
            _ => Err(maix_core::MaixError::Tool(format!(
                "git_branch: unknown action '{action}'. Use: list, create, switch, delete"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// GitPrCreateTool
// ---------------------------------------------------------------------------

pub struct GitPrCreateTool;

impl Default for GitPrCreateTool {
    fn default() -> Self {
        Self
    }
}

impl GitPrCreateTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitPrCreateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_pr_create".into(),
            description: "Create a GitHub pull request using gh CLI. Requires gh to be installed and authenticated."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "title": { "type": "string", "description": "PR title (required)" },
                    "body": { "type": "string", "description": "PR body/description" },
                    "base": { "type": "string", "description": "Base branch (default: main)" },
                    "draft": { "type": "boolean", "description": "Create as draft PR (default: false)" }
                },
                "required": ["title"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let title = args["title"].as_str().unwrap_or("");
        let body = args["body"].as_str().unwrap_or("");
        let base = args["base"].as_str().unwrap_or("main");
        let draft = args["draft"].as_bool().unwrap_or(false);
        let path = ctx.working_dir.join(path_str);

        if title.is_empty() {
            return Err(maix_core::MaixError::Tool("git_pr_create: title is required".into()));
        }

        let mut cmd_args = vec![
            "pr", "create",
            "--title", title,
            "--base", base,
        ];
        if !body.is_empty() {
            cmd_args.push("--body");
            cmd_args.push(body);
        }
        if draft {
            cmd_args.push("--draft");
        }

        let output = tokio::process::Command::new("gh")
            .args(&cmd_args)
            .current_dir(&path)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("git_pr_create: gh CLI not found: {e}")))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!("PR created successfully.\n{stdout}"))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(maix_core::MaixError::Tool(format!("git_pr_create failed: {stderr}")))
        }
    }
}

// ---------------------------------------------------------------------------
// GitPrReviewTool
// ---------------------------------------------------------------------------

pub struct GitPrReviewTool;

impl Default for GitPrReviewTool {
    fn default() -> Self {
        Self
    }
}

impl GitPrReviewTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitPrReviewTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_pr_review".into(),
            description: "Review a GitHub PR: view PR info and diff using gh CLI."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "pr": { "type": "string", "description": "PR number or URL (optional, uses current branch if omitted)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let pr = args["pr"].as_str().unwrap_or("");
        let path = ctx.working_dir.join(path_str);

        // Get PR view
        let mut view_args = vec!["pr", "view"];
        if !pr.is_empty() {
            view_args.push(pr);
        }
        let view_output = tokio::process::Command::new("gh")
            .args(&view_args)
            .current_dir(&path)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("git_pr_review: gh CLI not found: {e}")))?;

        // Get PR diff
        let mut diff_args = vec!["pr", "diff"];
        if !pr.is_empty() {
            diff_args.push(pr);
        }
        let diff_output = tokio::process::Command::new("gh")
            .args(&diff_args)
            .current_dir(&path)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("git_pr_review: {e}")))?;

        let view = if view_output.status.success() {
            String::from_utf8_lossy(&view_output.stdout).to_string()
        } else {
            let stderr = String::from_utf8_lossy(&view_output.stderr);
            return Err(maix_core::MaixError::Tool(format!("git_pr_review view failed: {stderr}")));
        };

        let diff = if diff_output.status.success() {
            let d = String::from_utf8_lossy(&diff_output.stdout).to_string();
            truncate_lines(&d, 500)
        } else {
            String::new()
        };

        Ok(format!("PR Info:\n{view}\n\nDiff:\n{diff}"))
    }
}

// ---------------------------------------------------------------------------
// GitIssueTool
// ---------------------------------------------------------------------------

pub struct GitIssueTool;

impl Default for GitIssueTool {
    fn default() -> Self {
        Self
    }
}

impl GitIssueTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitIssueTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "git_issue".into(),
            description: "Create or view GitHub issues using gh CLI."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path (default: .)" },
                    "action": {
                        "type": "string",
                        "enum": ["create", "view", "list"],
                        "description": "Action to perform (default: list)"
                    },
                    "title": { "type": "string", "description": "Issue title (required for create)" },
                    "body": { "type": "string", "description": "Issue body (for create)" },
                    "labels": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Labels to add (for create)"
                    },
                    "issue": { "type": "string", "description": "Issue number (required for view)" },
                    "limit": { "type": "integer", "description": "Number of issues to list (default: 20)" }
                }
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let action = args["action"].as_str().unwrap_or("list");
        let path = ctx.working_dir.join(path_str);

        match action {
            "create" => {
                let title = args["title"].as_str().unwrap_or("");
                if title.is_empty() {
                    return Err(maix_core::MaixError::Tool("git_issue create: title is required".into()));
                }
                let body = args["body"].as_str().unwrap_or("");
                let labels: Vec<&str> = args["labels"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                let mut cmd_args = vec!["issue", "create", "--title", title];
                if !body.is_empty() {
                    cmd_args.push("--body");
                    cmd_args.push(body);
                }
                for label in &labels {
                    cmd_args.push("--label");
                    cmd_args.push(label);
                }

                let output = tokio::process::Command::new("gh")
                    .args(&cmd_args)
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_issue: gh CLI not found: {e}")))?;

                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Ok(format!("Issue created.\n{stdout}"))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_issue create failed: {stderr}")))
                }
            }
            "view" => {
                let issue = args["issue"].as_str().unwrap_or("");
                if issue.is_empty() {
                    return Err(maix_core::MaixError::Tool("git_issue view: issue number is required".into()));
                }
                let output = tokio::process::Command::new("gh")
                    .args(["issue", "view", issue])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_issue: {e}")))?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_issue view failed: {stderr}")))
                }
            }
            "list" => {
                let limit = args["limit"].as_u64().unwrap_or(20);
                let limit_str = limit.to_string();
                let output = tokio::process::Command::new("gh")
                    .args(["issue", "list", "--limit", &limit_str])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| maix_core::MaixError::Tool(format!("git_issue: gh CLI not found: {e}")))?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(maix_core::MaixError::Tool(format!("git_issue list failed: {stderr}")))
                }
            }
            _ => Err(maix_core::MaixError::Tool(format!(
                "git_issue: unknown action '{action}'. Use: create, view, list"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn run_git(cwd: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        // Git commands may fail for valid reasons (not a repo, no upstream, etc.)
        None
    }
}

fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        text.to_string()
    } else {
        let mut result: String = lines[..max_lines].join("\n");
        result.push_str(&format!("\n... ({} more lines truncated)", lines.len() - max_lines));
        result
    }
}

// ---------------------------------------------------------------------------
// Git Worktree management (for sub-agent isolation)
// ---------------------------------------------------------------------------

/// Add a git worktree at `.maix/worktrees/<name>` relative to repo root.
pub async fn worktree_add(repo_path: &std::path::Path, name: &str) -> MaixResult<std::path::PathBuf> {
    let worktree_dir = repo_path.join(".maix").join("worktrees").join(name);

    // Create parent dir
    if let Some(parent) = worktree_dir.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| maix_core::MaixError::Tool(format!("worktree_add mkdir: {e}")))?;
    }

    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", worktree_dir.to_str().unwrap_or(""), "-b", name])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("worktree_add: {e}")))?;

    if output.status.success() {
        Ok(worktree_dir)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(maix_core::MaixError::Tool(format!("worktree_add failed: {stderr}")))
    }
}

/// Remove a git worktree.
pub async fn worktree_remove(repo_path: &std::path::Path, name: &str) -> MaixResult<()> {
    let worktree_dir = repo_path.join(".maix").join("worktrees").join(name);

    let output = tokio::process::Command::new("git")
        .args(["worktree", "remove", worktree_dir.to_str().unwrap_or(""), "--force"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("worktree_remove: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(maix_core::MaixError::Tool(format!("worktree_remove failed: {stderr}")))
    }
}

/// List all worktrees.
pub async fn worktree_list(repo_path: &std::path::Path) -> MaixResult<Vec<String>> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("worktree_list: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(maix_core::MaixError::Tool(format!("worktree_list failed: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            worktrees.push(path.to_string());
        }
    }
    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_lines_short_text() {
        let text = "line1\nline2\nline3";
        assert_eq!(truncate_lines(text, 5), text);
    }

    #[test]
    fn test_truncate_lines_exact() {
        let text = "line1\nline2\nline3";
        assert_eq!(truncate_lines(text, 3), text);
    }

    #[test]
    fn test_truncate_lines_truncated() {
        let text = "line1\nline2\nline3\nline4\nline5";
        let result = truncate_lines(text, 2);
        assert!(result.starts_with("line1\nline2"));
        assert!(result.contains("3 more lines truncated"));
    }

    #[test]
    fn test_truncate_lines_empty() {
        assert_eq!(truncate_lines("", 5), "");
    }

    #[test]
    fn test_truncate_lines_single_line() {
        assert_eq!(truncate_lines("hello", 1), "hello");
    }
}
