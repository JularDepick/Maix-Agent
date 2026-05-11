//! Tool system — trait + registry + builtins (Phase 1).

pub mod data;
pub mod fs;
pub mod info;
pub mod mcp;
pub mod network;
pub mod sandbox;
pub mod shell;
pub use sandbox::WorkDirSandbox;

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    Write,
    Network,
    Shell,
}

impl RiskLevel {
    pub fn needs_approval(&self, auto_approve: bool) -> bool {
        if auto_approve {
            return false;
        }
        matches!(self, RiskLevel::Shell | RiskLevel::Write)
    }
}

/// Tool result type — plain string for simplicity.
pub type ToolResult = String;

/// Context passed to every tool execution.
pub struct ToolCtx {
    pub session_id: String,
    pub working_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Tool: Send + Sync {
    fn def(&self) -> ToolDef;

    /// Execute the tool with given JSON arguments.
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String>;
}

// ---------------------------------------------------------------------------
// Builtin: fs_read
// ---------------------------------------------------------------------------

pub struct FsReadTool;

impl Default for FsReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FsReadTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fs_read".into(),
            description: "Read the contents of a file at the given path".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" }
                },
                "required": ["path"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let path = ctx.working_dir.join(path_str);
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_read {path_str}: {e}"))
        })?;
        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// Builtin: fs_write
// ---------------------------------------------------------------------------

pub struct FsWriteTool;

impl Default for FsWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FsWriteTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fs_write".into(),
            description: "Write content to a file at the given path. Creates parent directories if needed."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to write" },
                    "content": { "type": "string", "description": "Content to write" }
                },
                "required": ["path", "content"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let content = args["content"].as_str().unwrap_or_default();
        let path = ctx.working_dir.join(path_str);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                maix_core::MaixError::Tool(format!("fs_write mkdir: {e}"))
            })?;
        }
        tokio::fs::write(&path, content).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_write {path_str}: {e}"))
        })?;
        Ok(format!("Wrote {} bytes to {path_str}", content.len()))
    }
}

// ---------------------------------------------------------------------------
// Builtin: shell_exec
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
            description: "Execute a shell command and return stdout + stderr".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" }
                },
                "required": ["command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let command = args["command"].as_str().unwrap_or_default();
        let output = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .arg(if cfg!(windows) { "/C" } else { "-c" })
            .arg(command)
            .current_dir(&ctx.working_dir)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("shell_exec: {e}")))?;

        let mut result = String::new();
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
            Ok(format!("[exit code: {}]\n{}", output.status.code().unwrap_or(-1), result))
        }
    }
}

// ---------------------------------------------------------------------------
// Builtin: web_fetch
// ---------------------------------------------------------------------------

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_fetch".into(),
            description: "Fetch content from a URL".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" }
                },
                "required": ["url"]
            }),
            risk_level: RiskLevel::Network,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let url = args["url"].as_str().unwrap_or_default();
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_fetch: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Ok(format!("HTTP {status}\n{text}"))
    }
}

// ---------------------------------------------------------------------------
// Tool Registry
// ---------------------------------------------------------------------------

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.def().name == name).map(|t| t.as_ref())
    }

    pub fn get_defs(&self) -> Vec<ToolDef> {
        self.tools.iter().map(|t| t.def()).collect()
    }

    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.iter().map(|t| t.as_ref()).collect()
    }

    /// Register all Phase 1 builtins.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(FsReadTool::new()));
        reg.register(Box::new(FsWriteTool::new()));
        reg.register(Box::new(ShellExecTool::new()));
        reg.register(Box::new(WebFetchTool::new()));
        reg
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers: ToolDef → OpenAI-compatible format
// ---------------------------------------------------------------------------

impl ToolDef {
    pub fn to_openai(&self) -> maix_core::ToolDef {
        maix_core::ToolDef::new(&self.name, &self.description, self.parameters.clone())
    }
}
