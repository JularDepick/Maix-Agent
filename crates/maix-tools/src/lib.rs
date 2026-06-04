//! # Maix-Tools
//!
//! Tool system for Maix-Agent — trait definitions, registry, and built-in implementations.
//!
//! This crate provides the extensible tool framework that powers Maix-Agent's
//! capabilities:
//!
//! - **[`Tool`] trait** — interface for all tools (def, execute)
//! - **[`ToolRegistry`]** — tool registration and discovery
//! - **Built-in tools** ([`builtin`]) — filesystem, shell, search, data, info tools
//! - **Network tools** ([`network`]) — web_fetch, web_search, http_request
//! - **Git tools** ([`git`]) — git_status, git_diff, git_log, git_blame, etc.
//! - **Security** ([`security`]) — code scanning, secret detection
//! - **LSP** ([`lsp`]) — language server protocol client
//! - **MCP** ([`mcp`]) — model context protocol support
//!
//! ## Tool Trait
//!
//! ```rust
//! use maix_tools::{Tool, ToolDef, ToolCtx, RiskLevel};
//! use async_trait::async_trait;
//! use serde_json::Value;
//!
//! struct MyTool;
//!
//! #[async_trait]
//! impl Tool for MyTool {
//!     fn def(&self) -> ToolDef {
//!         ToolDef {
//!             name: "my_tool".into(),
//!             description: "Does something useful".into(),
//!             parameters: serde_json::json!({"type": "object", "properties": {}}),
//!             risk_level: RiskLevel::ReadOnly,
//!         }
//!     }
//!
//!     async fn execute(&self, ctx: &ToolCtx, args: Value) -> maix_core::MaixResult<String> {
//!         Ok("result".into())
//!     }
//! }
//! ```

pub mod ast;
pub mod background;
pub mod batch;
pub mod builtin;
pub mod cache_stats;
pub mod collaboration;
pub mod coverage;
pub mod data;
pub mod deps;
pub mod diff_utils;
pub mod event_log;
pub mod file_lock;
pub mod formatter;
pub mod fs;
pub mod git;
pub mod indexer;
pub mod info;
pub mod lsp;
pub mod mcp;
pub mod multi_edit;
pub mod network;
pub mod profiler;
pub mod review;
pub mod sandbox;
pub mod scheduler;
pub mod security;
pub mod shell;
pub mod side_git;
pub mod sub_agent;
pub mod tasks;
pub mod templates;
pub mod test_runner;
pub mod undo;
pub mod worktree;
pub mod workflow;
pub use sandbox::WorkDirSandbox;

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Normalize a path: resolve `.` and `..` without using `canonicalize()`
/// (which adds `\\?\` prefix on Windows causing prefix mismatches).
pub(crate) fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Generate a unified diff between old and new content with context lines.
pub(crate) fn generate_diff(old: &str, new: &str, _path: &str, context: usize) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Find the first and last changed lines by scanning both contents
    // We compare line-by-line to find the changed region
    let old_len = old_lines.len();
    let new_len = new_lines.len();
    let max_len = old_len.max(new_len);

    // Find first differing line
    let mut first_diff = max_len;
    for i in 0..max_len {
        let ol = old_lines.get(i).copied().unwrap_or("");
        let nl = new_lines.get(i).copied().unwrap_or("");
        if ol != nl {
            first_diff = i;
            break;
        }
    }

    if first_diff == max_len {
        return "(no visible line changes)".to_string();
    }

    // Find last differing line (scan from end)
    let mut last_diff_old = 0usize;
    let mut last_diff_new = 0usize;
    {
        let mut oi = old_len;
        let mut ni = new_len;
        loop {
            if oi == 0 || ni == 0 { break; }
            oi -= 1;
            ni -= 1;
            if old_lines[oi] != new_lines[ni] {
                last_diff_old = oi;
                last_diff_new = ni;
                break;
            }
        }
    }

    // Compute context window
    let ctx_start = first_diff.saturating_sub(context);
    let ctx_end_old = (last_diff_old + context + 1).min(old_len);
    let ctx_end_new = (last_diff_new + context + 1).min(new_len);

    let mut diff = String::new();
    diff.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        ctx_start + 1,
        ctx_end_old - ctx_start,
        ctx_start + 1,
        ctx_end_new - ctx_start
    ));

    // Show context before
    for line in &old_lines[ctx_start..first_diff] {
        diff.push_str(&format!(" {}\n", line));
    }

    // Show removed lines
    for line in &old_lines[first_diff..=last_diff_old] {
        diff.push_str(&format!("-{}\n", line));
    }

    // Show added lines
    for line in &new_lines[first_diff..=last_diff_new] {
        diff.push_str(&format!("+{}\n", line));
    }

    // Show context after (use old lines — they should be the same in both)
    for line in &old_lines[last_diff_old + 1..ctx_end_old] {
        diff.push_str(&format!(" {}\n", line));
    }

    diff
}

/// Simple glob pattern matching supporting `*`, `**`, `?`, and `{a,b}` alternatives.
/// Matches against path segments (using `/` as separator).
pub(crate) fn simple_glob_match(pattern: &str, text: &str) -> bool {
    // Handle {a,b} alternatives by expanding
    if let Some(brace_start) = pattern.find('{') {
        if let Some(brace_end) = pattern[brace_start..].find('}') {
            let prefix = &pattern[..brace_start];
            let suffix = &pattern[brace_start + brace_end + 1..];
            let alternatives = &pattern[brace_start + 1..brace_start + brace_end];
            for alt in alternatives.split(',') {
                let expanded = format!("{}{}{}", prefix, alt, suffix);
                if simple_glob_match(&expanded, text) {
                    return true;
                }
            }
            return false;
        }
    }

    let pat_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_recursive(&pat_chars, &text_chars)
}

fn glob_match_recursive(pattern: &[char], text: &[char]) -> bool {
    glob_match_impl(pattern, text, 0, 0)
}

fn glob_match_impl(p: &[char], t: &[char], pi: usize, ti: usize) -> bool {
    let mut pi = pi;
    let mut ti = ti;

    loop {
        if pi >= p.len() {
            return ti >= t.len();
        }

        if p[pi] == '*' {
            let is_double = pi + 1 < p.len() && p[pi + 1] == '*';
            if is_double {
                // **: skip past ** and optional trailing /
                let mut next_pi = pi + 2;
                if next_pi < p.len() && p[next_pi] == '/' {
                    next_pi += 1;
                }
                // ** at end matches everything
                if next_pi >= p.len() {
                    return true;
                }
                // Try matching at each path boundary
                let mut try_pos = ti;
                loop {
                    if glob_match_impl(p, t, next_pi, try_pos) {
                        return true;
                    }
                    if try_pos >= t.len() {
                        break;
                    }
                    // Advance to next /
                    match t[try_pos..].iter().position(|&c| c == '/') {
                        Some(offset) => try_pos += offset + 1,
                        None => break,
                    }
                }
                return false;
            } else {
                // *: match 0+ non-/ characters
                let next_pi = pi + 1;
                // Special case: * at end matches all remaining non-/ text
                if next_pi >= p.len() {
                    return !t[ti..].contains(&'/');
                }
                for skip in 0..=(t.len() - ti) {
                    if skip > 0 && t[ti + skip - 1] == '/' {
                        break;
                    }
                    if glob_match_impl(p, t, next_pi, ti + skip) {
                        return true;
                    }
                }
                return false;
            }
        }

        if p[pi] == '?' {
            if ti >= t.len() || t[ti] == '/' {
                return false;
            }
            pi += 1;
            ti += 1;
            continue;
        }

        // Literal
        if ti >= t.len() || p[pi] != t[ti] {
            return false;
        }
        pi += 1;
        ti += 1;
    }
}

/// Provide context-aware suggestions for common tool errors.
pub(crate) fn suggest_fix(tool: &str, error: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("not found") || error_lower.contains("no such file") {
        return format!("{tool}: file not found — check the path is correct and the file exists");
    }
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        return format!("{tool}: permission denied — check file permissions or run with appropriate privileges");
    }
    if error_lower.contains("timed out") || error_lower.contains("timeout") {
        return format!("{tool}: operation timed out — try increasing the timeout or check network connectivity");
    }
    if error_lower.contains("connection refused") || error_lower.contains("connect") {
        return format!("{tool}: connection failed — check if the server is running and the URL is correct");
    }
    if error_lower.contains("invalid regex") {
        return format!("{tool}: invalid regex pattern — check regex syntax (use https://regex101.com to test)");
    }
    if error_lower.contains("binary file") {
        return format!("{tool}: binary file detected — use a binary-aware tool or convert to text first");
    }
    if error_lower.contains("utf-8") || error_lower.contains("invalid utf") {
        return format!("{tool}: encoding error — file may be binary or use a non-UTF-8 encoding");
    }

    String::new()
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Tool definition — metadata exposed to the agent for tool selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Unique tool name (e.g., "fs_read", "shell_exec").
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub parameters: Value,
    /// Risk level determining whether user approval is needed.
    pub risk_level: RiskLevel,
}

/// Risk level for tool execution approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Read-only operations (no side effects).
    ReadOnly,
    /// Write operations (file changes, etc.).
    Write,
    /// Network operations (HTTP requests, etc.).
    Network,
    /// Shell command execution (highest risk).
    Shell,
}

impl RiskLevel {
    /// Check if this risk level requires user approval.
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
    /// Current session identifier.
    pub session_id: String,
    /// Working directory for file operations.
    pub working_dir: PathBuf,
    /// Channel to ask the user a question. Send (question, response_sender).
    pub ask_user_tx: Option<tokio::sync::mpsc::UnboundedSender<(String, tokio::sync::oneshot::Sender<String>)>>,
}

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

/// Trait for implementing Maix-Agent tools.
///
/// Tools are the primary way for the agent to interact with the environment.
/// Each tool has a definition (name, description, parameters) and an execute method.
///
/// # Example
///
/// ```rust
/// use maix_tools::{Tool, ToolDef, ToolCtx, RiskLevel};
/// use async_trait::async_trait;
/// use serde_json::Value;
///
/// struct MyTool;
///
/// #[async_trait]
/// impl Tool for MyTool {
///     fn def(&self) -> ToolDef {
///         ToolDef {
///             name: "my_tool".into(),
///             description: "Does something useful".into(),
///             parameters: serde_json::json!({"type": "object", "properties": {}}),
///             risk_level: RiskLevel::ReadOnly,
///         }
///     }
///
///     async fn execute(&self, ctx: &ToolCtx, args: Value) -> maix_core::MaixResult<String> {
///         Ok("result".into())
///     }
/// }
/// ```
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool's definition (name, description, parameters, risk level).
    fn def(&self) -> ToolDef;

    /// Execute the tool with given JSON arguments.
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String>;
}

// ---------------------------------------------------------------------------
// Tool Registry
// ---------------------------------------------------------------------------

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    task_store: tasks::TaskStore,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            task_store: tasks::new_task_store(),
        }
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

    pub async fn execute(&self, name: &str, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let tool = self.get(name)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("tool not found: {name}")))?;
        tool.execute(ctx, args).await
    }

    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.iter().map(|t| t.as_ref()).collect()
    }

    /// Register all builtins.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        // File system
        reg.register(Box::new(builtin::FsReadTool::new()));
        reg.register(Box::new(builtin::FsWriteTool::new()));
        reg.register(Box::new(builtin::FsEditTool::new()));
        reg.register(Box::new(builtin::FsListTool::new()));
        reg.register(Box::new(builtin::FsDeleteTool::new()));
        // Search
        reg.register(Box::new(builtin::GrepTool::new()));
        reg.register(Box::new(builtin::GlobTool::new()));
        // Shell
        reg.register(Box::new(builtin::ShellExecTool::new()));
        reg.register(Box::new(builtin::ShellSpawnTool::new()));
        // Network
        reg.register(Box::new(network::WebFetchTool::new()));
        reg.register(Box::new(network::WebSearchTool::new()));
        reg.register(Box::new(network::HttpRequestTool::new()));
        // Info
        reg.register(Box::new(builtin::SysInfoTool::new()));
        reg.register(Box::new(builtin::DirTreeTool::new()));
        reg.register(Box::new(builtin::EnvVarsTool::new()));
        reg.register(Box::new(builtin::SessionStatsTool::new()));
        // Data
        reg.register(Box::new(builtin::JsonParseTool::new()));
        reg.register(Box::new(builtin::TomlParseTool::new()));
        reg.register(Box::new(builtin::TextTransformTool::new()));
        // Git
        reg.register(Box::new(git::GitStatusTool::new()));
        reg.register(Box::new(git::GitDiffTool::new()));
        reg.register(Box::new(git::GitLogTool::new()));
        reg.register(Box::new(git::GitBlameTool::new()));
        reg.register(Box::new(git::GitAddTool::new()));
        reg.register(Box::new(git::GitCommitTool::new()));
        reg.register(Box::new(git::GitBranchTool::new()));
        reg.register(Box::new(git::GitPrCreateTool::new()));
        reg.register(Box::new(git::GitPrReviewTool::new()));
        reg.register(Box::new(git::GitIssueTool::new()));
        // Tasks
        reg.register(Box::new(tasks::TaskCreateTool::new(reg.task_store.clone())));
        reg.register(Box::new(tasks::TaskUpdateTool::new(reg.task_store.clone())));
        reg.register(Box::new(tasks::TaskListTool::new(reg.task_store.clone())));
        reg.register(Box::new(tasks::TaskGetTool::new(reg.task_store.clone())));
        reg.register(Box::new(tasks::TaskStopTool::new(reg.task_store.clone())));
        reg.register(Box::new(tasks::TaskOutputTool::new(reg.task_store.clone())));
        // User interaction
        reg.register(Box::new(builtin::AskUserTool::new()));
        // Sub-agent
        reg.register(Box::new(sub_agent::SubAgentTool::new()));
        // Side-git snapshots
        reg.register(Box::new(side_git::SnapshotTool::new()));
        reg.register(Box::new(side_git::RestoreTool::new()));
        reg.register(Box::new(side_git::SnapshotListTool::new()));
        // Background tasks
        let bg_mgr = std::sync::Arc::new(tokio::sync::Mutex::new(background::BackgroundTaskManager::new(10)));
        reg.register(Box::new(background::BgSpawnTool(bg_mgr.clone())));
        reg.register(Box::new(background::BgListTool(bg_mgr.clone())));
        reg.register(Box::new(background::BgLogTool(bg_mgr.clone())));
        reg.register(Box::new(background::BgCancelTool(bg_mgr)));
        // Test runner
        reg.register(Box::new(test_runner::TestRunTool));
        // Multi-file edit
        reg.register(Box::new(multi_edit::MultiEditTool));
        // Auto-format
        reg.register(Box::new(formatter::FormatTool));
        // Security scan
        reg.register(Box::new(security::SecurityScanTool));
        // Batch operations
        reg.register(Box::new(batch::BatchEditTool));
        reg.register(Box::new(batch::BatchExecTool));
        // Undo/redo
        let undo_mgr = std::sync::Arc::new(tokio::sync::Mutex::new(undo::UndoManager::new(100)));
        reg.register(Box::new(undo::UndoTool(undo_mgr.clone())));
        reg.register(Box::new(undo::RedoTool(undo_mgr.clone())));
        reg.register(Box::new(undo::UndoHistoryTool(undo_mgr)));
        // File locking
        let lock_mgr = std::sync::Arc::new(tokio::sync::Mutex::new(file_lock::FileLockManager::new()));
        reg.register(Box::new(file_lock::FileLockTool(lock_mgr.clone())));
        reg.register(Box::new(file_lock::FileUnlockTool(lock_mgr.clone())));
        reg.register(Box::new(file_lock::FileLocksTool(lock_mgr)));
        // Code templates
        reg.register(Box::new(templates::TemplateListTool));
        reg.register(Box::new(templates::TemplateExpandTool));
        // Dependency analysis
        reg.register(Box::new(deps::DepGraphTool));
        reg.register(Box::new(deps::DepCyclesTool));
        reg.register(Box::new(deps::DepImpactTool));
        // Coverage
        reg.register(Box::new(coverage::CoverageRunTool));
        // Event log
        let event_log = std::sync::Arc::new(tokio::sync::Mutex::new(event_log::EventLog::new(200)));
        reg.register(Box::new(event_log::EventLogTool(event_log.clone())));
        reg.register(Box::new(event_log::EventStatsTool(event_log)));
        // Diff utilities
        reg.register(Box::new(diff_utils::DiffStatsTool));
        // Token estimation
        reg.register(Box::new(diff_utils::TokenEstimateTool));
        // Profiler
        reg.register(Box::new(profiler::TimeCommandTool));
        reg.register(Box::new(profiler::BenchmarkTool));
        reg.register(Box::new(profiler::CargoBenchTool));
        // Cache statistics
        let cache_stats = std::sync::Arc::new(tokio::sync::Mutex::new(cache_stats::CacheStats::new(200)));
        reg.register(Box::new(cache_stats::CacheStatsTool(cache_stats.clone())));
        reg.register(Box::new(cache_stats::CacheRecordTool(cache_stats)));
        // Code indexer
        let code_index = std::sync::Arc::new(tokio::sync::Mutex::new(indexer::CodeIndex::new()));
        reg.register(Box::new(indexer::IndexBuildTool(code_index.clone())));
        reg.register(Box::new(indexer::SymbolSearchTool(code_index.clone())));
        reg.register(Box::new(indexer::FileSymbolsTool(code_index)));
        // Workflow engine
        let workflow_engine = std::sync::Arc::new(tokio::sync::Mutex::new(workflow::WorkflowEngine::new()));
        reg.register(Box::new(workflow::WorkflowRunTool(workflow_engine.clone())));
        reg.register(Box::new(workflow::WorkflowListTool(workflow_engine.clone())));
        reg.register(Box::new(workflow::WorkflowHistoryTool(workflow_engine)));
        // Review analyzer
        reg.register(Box::new(review::ReviewDiffTool));
        // Multi-agent collaboration
        let collab_mgr = std::sync::Arc::new(tokio::sync::Mutex::new(collaboration::CollaborationManager::new()));
        reg.register(Box::new(collaboration::AgentListTool(collab_mgr.clone())));
        reg.register(Box::new(collaboration::TaskDecomposeTool(collab_mgr.clone())));
        reg.register(Box::new(collaboration::CollabStatusTool(collab_mgr)));
        // Scheduler
        let scheduler = std::sync::Arc::new(tokio::sync::Mutex::new(scheduler::Scheduler::new()));
        reg.register(Box::new(scheduler::ScheduleListTool(scheduler.clone())));
        reg.register(Box::new(scheduler::ScheduleAddTool(scheduler.clone())));
        reg.register(Box::new(scheduler::ScheduleRemoveTool(scheduler.clone())));
        reg.register(Box::new(scheduler::FileWatchTool(scheduler.clone())));
        // Cron scheduler
        reg.register(Box::new(scheduler::CronCreateTool(scheduler.clone())));
        reg.register(Box::new(scheduler::CronDeleteTool(scheduler.clone())));
        reg.register(Box::new(scheduler::CronListTool(scheduler.clone())));
        reg.register(Box::new(scheduler::ScheduleWakeupTool(scheduler)));
        // Worktree management
        reg.register(Box::new(worktree::WorktreeCreateTool));
        reg.register(Box::new(worktree::WorktreeListTool));
        reg.register(Box::new(worktree::WorktreeExitTool));
        // AST-aware editing
        reg.register(Box::new(ast::AstRenameTool));
        reg.register(Box::new(ast::AstFindRefsTool));
        reg.register(Box::new(ast::AstExtractTool));
        reg.register(Box::new(ast::AstDefinitionsTool));
        // LSP client tools
        reg.register(Box::new(lsp::tools::LspGotoDefinitionTool));
        reg.register(Box::new(lsp::tools::LspFindReferencesTool));
        reg.register(Box::new(lsp::tools::LspHoverTool));
        reg.register(Box::new(lsp::tools::LspDocumentSymbolsTool));
        reg.register(Box::new(lsp::tools::LspWorkspaceSymbolsTool));
        reg
    }

    /// Register MCP tools from config. Spawns background tasks to connect to MCP servers.
    pub async fn register_mcp_tools(&mut self, mcp_configs: &[maix_core::config::McpServerConfig]) {
        for cfg in mcp_configs {
            let args: Vec<&str> = cfg.args.iter().map(|s| s.as_str()).collect();
            let env: Vec<(String, String)> = cfg.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            match mcp::connect_mcp_server(&cfg.name, &cfg.command, &args, &env).await {
                Ok(bridges) => {
                    let count = bridges.len();
                    for bridge in bridges {
                        self.register(Box::new(bridge));
                    }
                    tracing::info!("MCP '{}': registered {} tools", cfg.name, count);
                }
                Err(e) => {
                    tracing::warn!("MCP '{}': failed to connect: {e}", cfg.name);
                }
            }
        }
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

