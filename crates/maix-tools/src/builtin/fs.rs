//! Filesystem tools: fs_read, fs_write, fs_edit, fs_list, fs_delete.

use async_trait::async_trait;
use base64::Engine;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{generate_diff, RiskLevel, Tool, ToolCtx, ToolDef};
use crate::sandbox::WorkDirSandbox;

// ---------------------------------------------------------------------------
// fs_read
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

        // Input validation
        if path_str.is_empty() {
            return Err(maix_core::MaixError::Tool("fs_read: path is required".into()));
        }
        if path_str.contains('\0') {
            return Err(maix_core::MaixError::Tool("fs_read: null bytes in path".into()));
        }
        if limit == 0 || limit > 50_000 {
            return Err(maix_core::MaixError::Tool("fs_read: limit must be 1-50000".into()));
        }

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
            let suggestion = crate::suggest_fix("fs_read", &e.to_string());
            maix_core::MaixError::Tool(format!("fs_read {path_str}: {e}\n{suggestion}"))
        })?;
        if raw[..raw.len().min(8192)].contains(&0) {
            let suggestion = crate::suggest_fix("fs_read", "binary file");
            return Err(maix_core::MaixError::Tool(format!(
                "fs_read {path_str}: binary file detected\n{suggestion}"
            )));
        }

        let content = String::from_utf8(raw).map_err(|e| {
            let suggestion = crate::suggest_fix("fs_read", &e.to_string());
            maix_core::MaixError::Tool(format!("fs_read {path_str}: invalid UTF-8: {e}\n{suggestion}"))
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
// fs_write
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

        // Input validation
        if path_str.is_empty() {
            return Err(maix_core::MaixError::Tool("fs_write: path is required".into()));
        }
        if path_str.contains('\0') {
            return Err(maix_core::MaixError::Tool("fs_write: null bytes in path".into()));
        }

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
        if let Ok(Some(diag)) = crate::lsp::run_diagnostics(&path, &ctx.working_dir).await {
            result.push('\n');
            result.push_str(&diag);
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// fs_edit
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

        // Input validation
        if path_str.is_empty() {
            return Err(maix_core::MaixError::Tool("fs_edit: path is required".into()));
        }
        if path_str.contains('\0') {
            return Err(maix_core::MaixError::Tool("fs_edit: null bytes in path".into()));
        }
        if old_text.is_empty() {
            return Err(maix_core::MaixError::Tool("fs_edit: old_text cannot be empty".into()));
        }

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
        if let Ok(Some(diag)) = crate::lsp::run_diagnostics(&path, &ctx.working_dir).await {
            result.push('\n');
            result.push_str(&diag);
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// fs_list
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
        Ok(crate::fs::fs_list(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// fs_delete
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
        Ok(crate::fs::fs_delete(ctx, args).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx(dir: std::path::PathBuf) -> ToolCtx {
        ToolCtx {
            session_id: "test".into(),
            working_dir: dir,
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn test_fs_read_basic() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello\nworld\nfoo\n").unwrap();

        let tool = FsReadTool::new();
        let ctx = test_ctx(dir.path().to_path_buf());
        let args = serde_json::json!({"path": "test.txt"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("hello"));
        assert!(result.contains("world"));
    }

    #[tokio::test]
    async fn test_fs_read_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let tool = FsReadTool::new();
        let ctx = test_ctx(dir.path().to_path_buf());
        let args = serde_json::json!({"path": "test.txt", "offset": 1, "limit": 2});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("line2"));
        assert!(result.contains("line3"));
        assert!(!result.contains("line1"));
    }

    #[tokio::test]
    async fn test_fs_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        // Create the file first so sandbox.canonicalize() works
        let file = dir.path().join("output.txt");
        std::fs::write(&file, "").unwrap();
        let ctx = test_ctx(dir.path().to_path_buf());

        // Write
        let write_tool = FsWriteTool::new();
        let args = serde_json::json!({"path": "output.txt", "content": "test content"});
        let result = write_tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("Wrote"));

        // Read back
        let read_tool = FsReadTool::new();
        let args = serde_json::json!({"path": "output.txt"});
        let result = read_tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("test content"));
    }

    #[tokio::test]
    async fn test_fs_read_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path().to_path_buf());

        let tool = FsReadTool::new();
        let args = serde_json::json!({"path": "nonexistent.txt"});
        let result = tool.execute(&ctx, args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fs_edit_replace() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("edit.txt");
        std::fs::write(&file, "hello world").unwrap();
        let ctx = test_ctx(dir.path().to_path_buf());

        let tool = FsEditTool::new();
        let args = serde_json::json!({"path": "edit.txt", "old_text": "world", "new_text": "rust"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("replaced 1 occurrence"));

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello rust");
    }

    #[tokio::test]
    async fn test_fs_edit_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("edit.txt");
        std::fs::write(&file, "hello world").unwrap();
        let ctx = test_ctx(dir.path().to_path_buf());

        let tool = FsEditTool::new();
        let args = serde_json::json!({"path": "edit.txt", "old_text": "missing", "new_text": "new"});
        let result = tool.execute(&ctx, args).await;
        assert!(result.is_err());
    }
}
