//! Info tools: sys_info, dir_tree, env_vars, session_stats.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};

// ---------------------------------------------------------------------------
// sys_info
// ---------------------------------------------------------------------------

pub struct SysInfoTool;

impl Default for SysInfoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SysInfoTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SysInfoTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "sys_info".into(),
            description: "Get system information (OS, arch, cwd, home)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::info::sys_info(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// dir_tree
// ---------------------------------------------------------------------------

pub struct DirTreeTool;

impl Default for DirTreeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DirTreeTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for DirTreeTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "dir_tree".into(),
            description: "Show directory tree structure (up to 3 levels deep)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Root path (default: .)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::info::dir_tree(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// env_vars
// ---------------------------------------------------------------------------

pub struct EnvVarsTool;

impl Default for EnvVarsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvVarsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for EnvVarsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "env_vars".into(),
            description: "List environment variables (secrets filtered out)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::info::env_vars(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// session_stats
// ---------------------------------------------------------------------------

pub struct SessionStatsTool;

impl Default for SessionStatsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStatsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SessionStatsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "session_stats".into(),
            description: "Show current session statistics and memory usage (for debugging)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, _args: Value) -> MaixResult<String> {
        let mut stats = Vec::new();

        // Session info
        stats.push(format!("Session ID: {}", ctx.session_id));
        stats.push(format!("Working dir: {}", ctx.working_dir.display()));

        // Process info
        stats.push(format!("PID: {}", std::process::id()));

        // Memory info (approximate)
        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", std::process::id()), "/FO", "LIST"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if line.contains("Mem Usage") {
                        stats.push(format!("Memory: {}", line.trim()));
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string(format!("/proc/{}/status", std::process::id())) {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") || line.starts_with("VmSize:") {
                        stats.push(line.trim().to_string());
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(kb) = text.parse::<u64>() {
                    stats.push(format!("Memory: {:.1} MB", kb as f64 / 1024.0));
                }
            }
        }

        // Disk usage of working dir
        if let Ok(entries) = std::fs::read_dir(&ctx.working_dir) {
            let count = entries.count();
            stats.push(format!("Files in working dir: {}", count));
        }

        Ok(stats.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolCtx {
        ToolCtx {
            session_id: "test".into(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| ".".into()),
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn test_sys_info() {
        let tool = SysInfoTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("OS:") || result.contains("os"));
    }

    #[tokio::test]
    async fn test_env_vars() {
        let tool = EnvVarsTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({});
        let result = tool.execute(&ctx, args).await.unwrap();
        // Should not contain common secrets
        assert!(!result.to_lowercase().contains("api_key="));
        assert!(!result.to_lowercase().contains("secret="));
    }

    #[tokio::test]
    async fn test_dir_tree() {
        let tool = DirTreeTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"path": "."});
        let result = tool.execute(&ctx, args).await.unwrap();
        // Should contain some directory structure
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_session_stats() {
        let tool = SessionStatsTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("Session ID:"));
        assert!(result.contains("Working dir:"));
        assert!(result.contains("PID:"));
    }
}
