//! Shell tools: shell_exec, shell_spawn.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

pub struct ShellExecTool;

impl Default for ShellExecTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellExecTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "shell_exec".into(),
            description: "Execute a shell command and return stdout + stderr. Supports timeout (default 120s)."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default: 120)" }
                },
                "required": ["command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let command = args["command"].as_str().unwrap_or_default();
        let timeout_secs = args["timeout"].as_u64().unwrap_or(120);

        // Input validation
        if command.is_empty() {
            return Err(maix_core::MaixError::Tool("shell_exec: command is required".into()));
        }
        if command.len() > 10_000 {
            return Err(maix_core::MaixError::Tool("shell_exec: command too long (max 10KB)".into()));
        }
        if command.contains('\0') {
            return Err(maix_core::MaixError::Tool("shell_exec: null bytes in command".into()));
        }
        if timeout_secs == 0 || timeout_secs > 3600 {
            return Err(maix_core::MaixError::Tool("shell_exec: timeout must be 1-3600 seconds".into()));
        }

        // Block dangerous commands
        let blocked = [
            "rm -rf /", "rm -rf /*", "mkfs", "dd if=", ":(){ :|:& };:", "format c:",
            "> /dev/sda", "chmod -R 777 /", "wget -O- | sh", "curl | sh",
        ];
        let cmd_lower = command.to_lowercase();
        for b in &blocked {
            if cmd_lower.contains(b) {
                return Err(maix_core::MaixError::Tool(format!(
                    "blocked dangerous command: {command}"
                )));
            }
        }

        tracing::info!("shell_exec: {command} (timeout: {timeout_secs}s)");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
                .arg(if cfg!(windows) { "/C" } else { "-c" })
                .arg(command)
                .current_dir(&ctx.working_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let mut result = format!("[executed: {command}]\n");
                if !output.stdout.is_empty() {
                    result.push_str(&String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    result.push_str("\n[stderr]\n");
                    result.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                if output.status.success() {
                    Ok(result)
                } else {
                    Ok(format!(
                        "{result}\n[exit code: {}]",
                        output.status.code().unwrap_or(-1)
                    ))
                }
            }
            Ok(Err(e)) => Err(maix_core::MaixError::Tool(format!("shell_exec: {e}"))),
            Err(_) => Err(maix_core::MaixError::Tool(format!(
                "shell_exec: command timed out after {timeout_secs}s: {command}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// shell_spawn
// ---------------------------------------------------------------------------

pub struct ShellSpawnTool;

impl Default for ShellSpawnTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellSpawnTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellSpawnTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "shell_spawn".into(),
            description: "Spawn a background shell command (fire-and-forget)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to spawn" }
                },
                "required": ["command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::shell::shell_spawn(ctx, args).await)
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
    async fn test_shell_exec_echo() {
        let tool = ShellExecTool::new();
        let ctx = test_ctx();
        let cmd = "echo hello";
        let args = serde_json::json!({"command": cmd});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_exec_blocked_command() {
        let tool = ShellExecTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"command": "rm -rf /"});
        let result = tool.execute(&ctx, args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_exec_timeout() {
        let tool = ShellExecTool::new();
        let ctx = test_ctx();
        let cmd = if cfg!(windows) { "ping -n 10 127.0.0.1" } else { "sleep 10" };
        let args = serde_json::json!({"command": cmd, "timeout": 1});
        let result = tool.execute(&ctx, args).await;
        assert!(result.is_err());
    }
}
