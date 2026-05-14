//! Tool system — trait + registry + builtins (Phase 1).

pub mod ast;
pub mod background;
pub mod batch;
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
pub mod workflow;
pub use sandbox::WorkDirSandbox;

use async_trait::async_trait;
use base64::Engine;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Normalize a path: resolve `.` and `..` without using `canonicalize()`
/// (which adds `\\?\` prefix on Windows causing prefix mismatches).
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
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
fn generate_diff(old: &str, new: &str, _path: &str, context: usize) -> String {
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
fn simple_glob_match(pattern: &str, text: &str) -> bool {
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
    /// Channel to ask the user a question. Send (question, response_sender).
    pub ask_user_tx: Option<tokio::sync::mpsc::UnboundedSender<(String, tokio::sync::oneshot::Sender<String>)>>,
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
            description: "Read the contents of a file at the given path. Supports line offset/limit for large files.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" },
                    "offset": { "type": "integer", "description": "Line number to start from (0-based, optional)" },
                    "limit": { "type": "integer", "description": "Max lines to read (default: 2000, optional)" }
                },
                "required": ["path"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().unwrap_or(2000) as usize;

        let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
        let path = sandbox.resolve(std::path::Path::new(path_str))
            .map_err(|e| maix_core::MaixError::Tool(format!("sandbox: {e}")))?;

        // Check if it's an image file
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let image_exts = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
        if image_exts.contains(&ext.as_str()) {
            let raw = tokio::fs::read(&path).await.map_err(|e| {
                maix_core::MaixError::Tool(format!("fs_read {path_str}: {e}"))
            })?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                "svg" => "image/svg+xml",
                _ => "application/octet-stream",
            };
            return Ok(serde_json::json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": mime,
                    "data": b64
                }
            }).to_string());
        }

        // Binary detection: read first 8KB and check for null bytes
        let raw = tokio::fs::read(&path).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_read {path_str}: {e}"))
        })?;
        if raw[..raw.len().min(8192)].contains(&0) {
            return Err(maix_core::MaixError::Tool(format!(
                "fs_read {path_str}: binary file detected"
            )));
        }

        let content = String::from_utf8(raw).map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_read {path_str}: invalid UTF-8: {e}"))
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let end = (offset + limit).min(lines.len());
        if offset >= lines.len() {
            return Ok(format!("(file has {} lines, offset {} is past end)", lines.len(), offset));
        }

        let mut result = String::new();
        for (i, line) in lines[offset..end].iter().enumerate() {
            result.push_str(&format!("{}\t{}\n", offset + i + 1, line));
        }
        let total = lines.len();
        if offset > 0 || end < total {
            result.push_str(&format!("\n(showing lines {}-{} of {})", offset + 1, end, total));
        }
        Ok(result)
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

        let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
        let path = sandbox.resolve(std::path::Path::new(path_str))
            .map_err(|e| maix_core::MaixError::Tool(format!("sandbox: {e}")))?;

        // Read old content for diff if file exists
        let old_content = tokio::fs::read_to_string(&path).await.ok();

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                maix_core::MaixError::Tool(format!("fs_write mkdir: {e}"))
            })?;
        }
        tokio::fs::write(&path, content).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_write {path_str}: {e}"))
        })?;

        let mut result = format!("Wrote {} bytes to {path_str}", content.len());

        // Generate diff summary if overwriting
        if let Some(old) = old_content {
            if old != content {
                let old_lines: Vec<&str> = old.lines().collect();
                let new_lines: Vec<&str> = content.lines().collect();
                let removed = old_lines.iter().filter(|l| !new_lines.contains(l)).count();
                let added = new_lines.iter().filter(|l| !old_lines.contains(l)).count();
                result.push_str(&format!("\n[diff: +{added} -{removed} lines]"));

                // Show first few changed lines (max 10)
                let mut shown = 0;
                for line in &new_lines {
                    if !old_lines.contains(line) && shown < 10 {
                        result.push_str(&format!("\n+ {}", line));
                        shown += 1;
                    }
                }
                if added > 10 {
                    result.push_str(&format!("\n... and {} more added lines", added - 10));
                }
                shown = 0;
                for line in &old_lines {
                    if !new_lines.contains(line) && shown < 5 {
                        result.push_str(&format!("\n- {}", line));
                        shown += 1;
                    }
                }
                if removed > 5 {
                    result.push_str(&format!("\n... and {} more removed lines", removed - 5));
                }
            }
        }

        // Run post-write diagnostics
        if let Ok(Some(diag)) = lsp::run_diagnostics(&path, &ctx.working_dir).await {
            result.push('\n');
            result.push_str(&diag);
        }

        Ok(result)
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

        // Block dangerous commands
        let blocked = ["rm -rf /", "mkfs", "dd if=", ":(){ :|:& };:", "format c:"];
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
// Builtin: fs_edit
// ---------------------------------------------------------------------------

pub struct FsEditTool;

impl Default for FsEditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsEditTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FsEditTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fs_edit".into(),
            description: "Edit a file by replacing old_text with new_text (find-and-replace). More efficient than fs_write for small changes."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit" },
                    "old_text": { "type": "string", "description": "Text to find and replace" },
                    "new_text": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or_default();
        let old_text = args["old_text"].as_str().unwrap_or_default();
        let new_text = args["new_text"].as_str().unwrap_or_default();

        let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
        let path = sandbox.resolve(std::path::Path::new(path_str))
            .map_err(|e| maix_core::MaixError::Tool(format!("sandbox: {e}")))?;

        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_edit read {path_str}: {e}"))
        })?;

        let count = content.matches(old_text).count();
        if count == 0 {
            return Err(maix_core::MaixError::Tool(format!(
                "fs_edit: old_text not found in {path_str}"
            )));
        }
        if count > 1 {
            return Err(maix_core::MaixError::Tool(format!(
                "fs_edit: old_text matches {count} times in {path_str}. Provide more surrounding context to make it unique."
            )));
        }

        let new_content = content.replacen(old_text, new_text, 1);
        tokio::fs::write(&path, &new_content).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("fs_edit write {path_str}: {e}"))
        })?;

        // Generate unified diff with 3 lines of context
        let diff = generate_diff(&content, &new_content, path_str, 3);
        let mut result = format!("Edited {path_str}: replaced 1 occurrence\n{diff}");

        // Run post-edit diagnostics
        if let Ok(Some(diag)) = lsp::run_diagnostics(&path, &ctx.working_dir).await {
            result.push('\n');
            result.push_str(&diag);
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Builtin: fs_list
// ---------------------------------------------------------------------------

pub struct FsListTool;

impl Default for FsListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsListTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FsListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fs_list".into(),
            description: "List files and directories at the given path".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list (default: .)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(fs::fs_list(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: fs_delete
// ---------------------------------------------------------------------------

pub struct FsDeleteTool;

impl Default for FsDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsDeleteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FsDeleteTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fs_delete".into(),
            description: "Delete a file at the given path".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to delete" }
                },
                "required": ["path"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(fs::fs_delete(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: grep (content search)
// ---------------------------------------------------------------------------

pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "grep".into(),
            description: "Search file contents using regex patterns. Returns matching file paths or matching lines with context.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory to search in (default: .)" },
                    "glob": { "type": "string", "description": "File glob filter, e.g. \"*.rs\" or \"*.{ts,tsx}\"" },
                    "output_mode": { "type": "string", "description": "Output mode: \"files_with_matches\" (default) or \"content\"" },
                    "head_limit": { "type": "integer", "description": "Max results (default: 250)" }
                },
                "required": ["pattern"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let pattern_str = args["pattern"].as_str().unwrap_or_default();
        let path_str = args["path"].as_str().unwrap_or(".");
        let glob_filter = args["glob"].as_str();
        let output_mode = args["output_mode"].as_str().unwrap_or("files_with_matches");
        let head_limit = args["head_limit"].as_u64().unwrap_or(250) as usize;

        let re = regex::Regex::new(pattern_str)
            .map_err(|e| maix_core::MaixError::Tool(format!("grep: invalid regex: {e}")))?;

        let root = normalize_path(&ctx.working_dir.join(path_str));

        let skip_dirs: &[&str] = &[".git", "node_modules", "target", ".venv", "__pycache__"];
        let mut results: Vec<String> = Vec::new();

        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if results.len() >= head_limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                if results.len() >= head_limit {
                    break;
                }
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if file_type.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                // Apply glob filter (match against path relative to working_dir)
                if let Some(glob_pat) = glob_filter {
                    let match_path = entry.path().strip_prefix(&ctx.working_dir)
                        .unwrap_or(&entry.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    if !simple_glob_match(glob_pat, &match_path) {
                        continue;
                    }
                }

                // Read file, skip binary
                let raw = match std::fs::read(entry.path()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if !raw.is_empty() && raw[..raw.len().min(8192)].contains(&0) {
                    continue;
                }
                let content = match String::from_utf8(raw) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let rel_path = entry.path().strip_prefix(&root)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                if output_mode == "files_with_matches" {
                    if re.is_match(&content) {
                        results.push(rel_path.to_string());
                    }
                } else {
                    for (line_no, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            results.push(format!("{}:{}: {}", rel_path, line_no + 1, line));
                            if results.len() >= head_limit {
                                break;
                            }
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No matches found for pattern: {pattern_str}"))
        } else {
            let mut out = results.join("\n");
            if results.len() >= head_limit {
                out.push_str(&format!("\n(truncated at {head_limit} results)"));
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// Builtin: glob (file pattern matching)
// ---------------------------------------------------------------------------

pub struct GlobTool;

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "glob".into(),
            description: "Find files by glob pattern (e.g. \"**/*.rs\", \"src/**/*.ts\"). Returns matching file paths sorted by modification time.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern, e.g. \"**/*.rs\" or \"src/**/*.ts\"" },
                    "path": { "type": "string", "description": "Base directory (default: .)" },
                    "head_limit": { "type": "integer", "description": "Max results (default: 250)" }
                },
                "required": ["pattern"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let pattern_str = args["pattern"].as_str().unwrap_or_default();
        let path_str = args["path"].as_str().unwrap_or(".");
        let head_limit = args["head_limit"].as_u64().unwrap_or(250) as usize;

        // Use join + normalize instead of sandbox.resolve() to avoid Windows \\?\ prefix
        let root = normalize_path(&ctx.working_dir.join(path_str));

        let skip_dirs: &[&str] = &[".git", "node_modules", "target", ".venv", "__pycache__"];
        let mut paths: Vec<(std::time::SystemTime, String)> = Vec::new();

        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if paths.len() >= head_limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                if paths.len() >= head_limit {
                    break;
                }
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if file_type.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                // Match path relative to working_dir so patterns like
                // "crates/maix-tools/**/*.rs" match correctly
                let match_path = entry.path().strip_prefix(&ctx.working_dir)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                // Display path relative to root
                let rel = entry.path().strip_prefix(&root)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                if simple_glob_match(pattern_str, &match_path) {
                    let mtime = entry.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    paths.push((mtime, rel));
                }
            }
        }

        // Sort by modification time, newest first
        paths.sort_by_key(|b| std::cmp::Reverse(b.0));

        if paths.is_empty() {
            Ok(format!("No files matched pattern: {pattern_str}"))
        } else {
            let count = paths.len();
            let list: Vec<String> = paths.into_iter().map(|(_, p)| p).collect();
            Ok(format!("{} files matched:\n{}", count, list.join("\n")))
        }
    }
}

// ---------------------------------------------------------------------------
// Builtin: shell_spawn
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
        Ok(shell::shell_spawn(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: sys_info
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
        Ok(info::sys_info(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: dir_tree
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
        Ok(info::dir_tree(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: env_vars
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
        Ok(info::env_vars(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: json_parse
// ---------------------------------------------------------------------------

pub struct JsonParseTool;

impl Default for JsonParseTool {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonParseTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for JsonParseTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "json_parse".into(),
            description: "Parse and pretty-print a JSON string".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "JSON string to parse" }
                },
                "required": ["input"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(data::json_parse(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: toml_parse
// ---------------------------------------------------------------------------

pub struct TomlParseTool;

impl Default for TomlParseTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TomlParseTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TomlParseTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "toml_parse".into(),
            description: "Parse and pretty-print a TOML string".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "TOML string to parse" }
                },
                "required": ["input"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(data::toml_parse(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: text_transform
// ---------------------------------------------------------------------------

pub struct TextTransformTool;

impl Default for TextTransformTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TextTransformTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TextTransformTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "text_transform".into(),
            description: "Transform text (uppercase, lowercase, trim, count lines/words)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Text to transform" },
                    "operation": { "type": "string", "description": "Operation: uppercase, lowercase, trim, lines, words" }
                },
                "required": ["input", "operation"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(data::text_transform(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// Builtin: ask_user
// ---------------------------------------------------------------------------

pub struct AskUserTool;

impl Default for AskUserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl AskUserTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ask_user".into(),
            description: "Ask the user a question and wait for their response. Use for clarifications, preferences, or choices.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask the user" },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of choices for the user to pick from"
                    }
                },
                "required": ["question"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let question = args["question"].as_str().unwrap_or_default();
        let options: Vec<String> = args["options"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let display = if options.is_empty() {
            question.to_string()
        } else {
            let opts: String = options.iter().enumerate()
                .map(|(i, o)| format!("  {}. {}", i + 1, o))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}\n\n{}", question, opts)
        };

        if let Some(tx) = &ctx.ask_user_tx {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            tx.send((display, resp_tx)).map_err(|_| {
                maix_core::MaixError::Tool("failed to send question to user".into())
            })?;
            let response = resp_rx.await.map_err(|_| {
                maix_core::MaixError::Tool("user response channel closed".into())
            })?;
            Ok(response)
        } else {
            // No interactive channel — return the question as-is for non-interactive mode
            Ok(format!("[ask_user] {}", display))
        }
    }
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
        reg.register(Box::new(FsReadTool::new()));
        reg.register(Box::new(FsWriteTool::new()));
        reg.register(Box::new(FsEditTool::new()));
        reg.register(Box::new(FsListTool::new()));
        reg.register(Box::new(FsDeleteTool::new()));
        // Search
        reg.register(Box::new(GrepTool::new()));
        reg.register(Box::new(GlobTool::new()));
        // Shell
        reg.register(Box::new(ShellExecTool::new()));
        reg.register(Box::new(ShellSpawnTool::new()));
        // Network
        reg.register(Box::new(network::WebFetchTool::new()));
        reg.register(Box::new(network::WebSearchTool::new()));
        reg.register(Box::new(network::HttpRequestTool::new()));
        // Info
        reg.register(Box::new(SysInfoTool::new()));
        reg.register(Box::new(DirTreeTool::new()));
        reg.register(Box::new(EnvVarsTool::new()));
        // Data
        reg.register(Box::new(JsonParseTool::new()));
        reg.register(Box::new(TomlParseTool::new()));
        reg.register(Box::new(TextTransformTool::new()));
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
        // User interaction
        reg.register(Box::new(AskUserTool::new()));
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
        reg.register(Box::new(scheduler::FileWatchTool(scheduler)));
        // AST-aware editing
        reg.register(Box::new(ast::AstRenameTool));
        reg.register(Box::new(ast::AstFindRefsTool));
        reg.register(Box::new(ast::AstExtractTool));
        reg.register(Box::new(ast::AstDefinitionsTool));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_double_star_basic() {
        assert!(simple_glob_match("**/*.rs", "src/lib.rs"));
        assert!(simple_glob_match("**/*.rs", "lib.rs"));
        assert!(simple_glob_match("**/*.rs", "a/b/c/d.rs"));
        assert!(!simple_glob_match("**/*.rs", "a/b/c/d.txt"));
    }

    #[test]
    fn glob_double_star_with_prefix() {
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/lib.rs"));
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/git.rs"));
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/mcp/client.rs"));
        assert!(!simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-core/src/lib.rs"));
    }

    #[test]
    fn glob_single_star() {
        assert!(simple_glob_match("*.rs", "lib.rs"));
        assert!(!simple_glob_match("*.rs", "src/lib.rs"));
        assert!(simple_glob_match("src/*.rs", "src/lib.rs"));
        assert!(!simple_glob_match("src/*.rs", "src/sub/lib.rs"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(simple_glob_match("?.rs", "a.rs"));
        assert!(!simple_glob_match("?.rs", "ab.rs"));
        assert!(!simple_glob_match("?.rs", "/.rs"));
    }

    #[test]
    fn glob_brace_alternatives() {
        assert!(simple_glob_match("*.{rs,toml}", "lib.rs"));
        assert!(simple_glob_match("*.{rs,toml}", "Cargo.toml"));
        assert!(!simple_glob_match("*.{rs,toml}", "lib.txt"));
    }

    #[test]
    fn glob_double_star_only() {
        assert!(simple_glob_match("**", "anything/at/all"));
        assert!(simple_glob_match("**", "file.txt"));
    }
}
