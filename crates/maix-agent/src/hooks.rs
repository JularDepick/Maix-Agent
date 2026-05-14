//! Hooks lifecycle system — run user-defined shell commands before/after tool execution.
//!
//! Supports PreToolUse, PostToolUse, and Stop hooks with glob matchers and timeouts.

use std::collections::HashMap;

/// Hook trigger type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
    Stop,
}

impl HookType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PreToolUse" | "pre_tool_use" => Some(Self::PreToolUse),
            "PostToolUse" | "post_tool_use" => Some(Self::PostToolUse),
            "Stop" | "stop" => Some(Self::Stop),
            _ => None,
        }
    }
}

/// A single hook definition.
#[derive(Debug, Clone)]
pub struct Hook {
    /// Tool name glob pattern. Empty string matches all tools.
    pub matcher: String,
    /// Shell command to execute.
    pub command: String,
    /// Timeout in milliseconds. Default: 5000.
    pub timeout_ms: u64,
}

/// Result of a PreToolUse hook that blocked execution.
#[derive(Debug)]
pub struct HookBlock {
    pub reason: String,
}

impl std::fmt::Display for HookBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hook blocked: {}", self.reason)
    }
}

/// Runs hooks at appropriate lifecycle points.
pub struct HookRunner {
    hooks: HashMap<HookType, Vec<Hook>>,
}

impl HookRunner {
    /// Build a HookRunner from settings hooks config.
    pub fn from_config(hooks_config: &HashMap<String, Vec<HookConfig>>) -> Self {
        let mut hooks: HashMap<HookType, Vec<Hook>> = HashMap::new();

        for (key, configs) in hooks_config {
            if let Some(hook_type) = HookType::parse(key) {
                let hook_list: Vec<Hook> = configs
                    .iter()
                    .map(|c| Hook {
                        matcher: c.matcher.clone(),
                        command: c.command.clone(),
                        timeout_ms: c.timeout_ms.unwrap_or(5000),
                    })
                    .collect();
                hooks.insert(hook_type, hook_list);
            }
        }

        Self { hooks }
    }

    /// Create an empty HookRunner (no hooks configured).
    pub fn empty() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Run PreToolUse hooks. Returns Err(HookBlock) if any hook blocks execution.
    pub async fn run_pre_tool(
        &self,
        tool_name: &str,
        tool_input: &str,
        working_dir: &std::path::Path,
    ) -> Result<(), HookBlock> {
        let hooks = match self.hooks.get(&HookType::PreToolUse) {
            Some(h) => h,
            None => return Ok(()),
        };

        for hook in hooks {
            if !matches_name(&hook.matcher, tool_name) {
                continue;
            }

            let env = build_env(tool_name, tool_input, "", working_dir);
            match run_command(&hook.command, &env, hook.timeout_ms, working_dir).await {
                Ok(_output) => {
                    // Non-zero exit is handled inside run_command
                }
                Err(block_reason) => {
                    return Err(HookBlock {
                        reason: block_reason,
                    });
                }
            }
        }

        Ok(())
    }

    /// Run PostToolUse hooks. Failures are logged but don't block.
    pub async fn run_post_tool(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_output: &str,
        working_dir: &std::path::Path,
    ) {
        let hooks = match self.hooks.get(&HookType::PostToolUse) {
            Some(h) => h,
            None => return,
        };

        for hook in hooks {
            if !matches_name(&hook.matcher, tool_name) {
                continue;
            }

            let env = build_env(tool_name, tool_input, tool_output, working_dir);
            if let Err(e) = run_command(&hook.command, &env, hook.timeout_ms, working_dir).await {
                tracing::warn!("PostToolUse hook failed for {}: {}", tool_name, e);
            }
        }
    }

    /// Run Stop hooks. Called when the agent loop finishes.
    pub async fn run_stop(&self, working_dir: &std::path::Path) {
        let hooks = match self.hooks.get(&HookType::Stop) {
            Some(h) => h,
            None => return,
        };

        for hook in hooks {
            let env = HashMap::new();
            if let Err(e) = run_command(&hook.command, &env, hook.timeout_ms, working_dir).await {
                tracing::warn!("Stop hook failed: {}", e);
            }
        }
    }

    /// Check if any hooks are configured.
    pub fn has_hooks(&self) -> bool {
        !self.hooks.is_empty()
    }
}

/// Config-format hook (deserialized from settings.json).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub matcher: String,
    pub command: String,
    pub timeout_ms: Option<u64>,
}

/// Check if a tool name matches a glob-like pattern.
/// Empty pattern matches everything.
fn matches_name(pattern: &str, name: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if pattern == name {
        return true;
    }
    // Simple glob: support trailing *
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    // Simple glob: support leading *
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    false
}

/// Build environment variables for hook execution.
fn build_env(
    tool_name: &str,
    tool_input: &str,
    tool_output: &str,
    working_dir: &std::path::Path,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("MAIX_TOOL_NAME".into(), tool_name.into());
    env.insert("MAIX_TOOL_INPUT".into(), tool_input.into());
    env.insert("MAIX_WORKING_DIR".into(), working_dir.display().to_string());

    if !tool_output.is_empty() {
        env.insert("MAIX_TOOL_OUTPUT".into(), tool_output.into());
    }

    // Extract file path from tool input if available
    if let Ok(args) = serde_json::from_str::<serde_json::Value>(tool_input) {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            env.insert("MAIX_FILE_PATH".into(), path.into());
        } else if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
            env.insert("MAIX_FILE_PATH".into(), file_path.into());
        }
    }

    env
}

/// Run a shell command with environment variables and timeout.
/// Returns Ok(output) on success, Err(reason) on failure/block.
async fn run_command(
    command: &str,
    env: &HashMap<String, String>,
    timeout_ms: u64,
    working_dir: &std::path::Path,
) -> Result<String, String> {
    let shell;
    let shell_arg;
    #[cfg(target_os = "windows")]
    {
        shell = "cmd";
        shell_arg = "/C";
    }
    #[cfg(not(target_os = "windows"))]
    {
        shell = "sh";
        shell_arg = "-c";
    }

    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg(shell_arg)
        .arg(command)
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    for (key, val) in env {
        cmd.env(key, val);
    }

    let timeout = std::time::Duration::from_millis(timeout_ms);

    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                Ok(stdout)
            } else {
                let reason = if stderr.is_empty() {
                    format!(
                        "Hook exited with code {}",
                        output.status.code().unwrap_or(-1)
                    )
                } else {
                    stderr.trim().to_string()
                };
                Err(reason)
            }
        }
        Ok(Err(e)) => Err(format!("Hook command failed: {e}")),
        Err(_) => Err(format!(
            "Hook timed out after {}ms",
            timeout_ms
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_matches_name() {
        assert!(matches_name("", "fs_write"));
        assert!(matches_name("fs_write", "fs_write"));
        assert!(!matches_name("fs_write", "fs_read"));
        assert!(matches_name("fs_*", "fs_write"));
        assert!(matches_name("fs_*", "fs_read"));
        assert!(matches_name("*_write", "fs_write"));
        assert!(!matches_name("*_write", "fs_read"));
    }

    #[test]
    fn test_hook_type_from_str() {
        assert_eq!(HookType::parse("PreToolUse"), Some(HookType::PreToolUse));
        assert_eq!(HookType::parse("pre_tool_use"), Some(HookType::PreToolUse));
        assert_eq!(HookType::parse("PostToolUse"), Some(HookType::PostToolUse));
        assert_eq!(HookType::parse("Stop"), Some(HookType::Stop));
        assert_eq!(HookType::parse("invalid"), None);
    }

    #[test]
    fn test_build_env() {
        let env = build_env("fs_write", r#"{"path":"/tmp/test.rs"}"#, "", PathBuf::from(".").as_path());
        assert_eq!(env.get("MAIX_TOOL_NAME").unwrap(), "fs_write");
        assert_eq!(env.get("MAIX_FILE_PATH").unwrap(), "/tmp/test.rs");
    }

    #[test]
    fn test_hook_runner_empty() {
        let runner = HookRunner::empty();
        assert!(!runner.has_hooks());
    }

    #[tokio::test]
    async fn test_run_command_success() {
        #[cfg(target_os = "windows")]
        let cmd = "echo hello";
        #[cfg(not(target_os = "windows"))]
        let cmd = "echo hello";

        let env = HashMap::new();
        let result = run_command(cmd, &env, 5000, PathBuf::from(".").as_path()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().trim() == "hello");
    }

    #[tokio::test]
    async fn test_run_command_failure() {
        #[cfg(target_os = "windows")]
        let cmd = "exit 1";
        #[cfg(not(target_os = "windows"))]
        let cmd = "false";

        let env = HashMap::new();
        let result = run_command(cmd, &env, 5000, PathBuf::from(".").as_path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_command_timeout() {
        // Use ping as a portable sleep alternative on Windows
        #[cfg(target_os = "windows")]
        let cmd = "ping -n 11 127.0.0.1 >nul";
        #[cfg(not(target_os = "windows"))]
        let cmd = "sleep 10";

        let env = HashMap::new();
        let result = run_command(cmd, &env, 200, PathBuf::from(".").as_path()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[tokio::test]
    async fn test_pre_tool_blocks_on_nonzero() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "PreToolUse".to_string(),
            vec![HookConfig {
                matcher: "fs_write".to_string(),
                command: "exit 1".to_string(),
                timeout_ms: Some(5000),
            }],
        );
        let runner = HookRunner::from_config(&hooks);

        let result = runner
            .run_pre_tool("fs_write", "{}", PathBuf::from(".").as_path())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pre_tool_passes_on_zero() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "PreToolUse".to_string(),
            vec![HookConfig {
                matcher: "fs_write".to_string(),
                command: "echo ok".to_string(),
                timeout_ms: Some(5000),
            }],
        );
        let runner = HookRunner::from_config(&hooks);

        let result = runner
            .run_pre_tool("fs_write", "{}", PathBuf::from(".").as_path())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pre_tool_skips_non_matching() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "PreToolUse".to_string(),
            vec![HookConfig {
                matcher: "fs_read".to_string(),
                command: "exit 1".to_string(),
                timeout_ms: Some(5000),
            }],
        );
        let runner = HookRunner::from_config(&hooks);

        // fs_write should pass because matcher is fs_read
        let result = runner
            .run_pre_tool("fs_write", "{}", PathBuf::from(".").as_path())
            .await;
        assert!(result.is_ok());
    }
}
