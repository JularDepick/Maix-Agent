//! File system tools — read, write, list, delete, copy, move, stat.

use crate::sandbox::WorkDirSandbox;
use crate::{ToolCtx, ToolResult};
use serde_json::Value;

pub async fn fs_read(ctx: &ToolCtx, args: Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or("");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let resolved = match sandbox.resolve(std::path::Path::new(path)) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };
    match tokio::fs::read_to_string(&resolved).await {
        Ok(content) => content,
        Err(e) => format!("read error: {e}"),
    }
}

pub async fn fs_write(ctx: &ToolCtx, args: Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or("");
    let content = args["content"].as_str().unwrap_or("");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let resolved = match sandbox.resolve(std::path::Path::new(path)) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };
    if let Some(parent) = resolved.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    match tokio::fs::write(&resolved, content).await {
        Ok(()) => format!("wrote {} bytes to {}", content.len(), path),
        Err(e) => format!("write error: {e}"),
    }
}

pub async fn fs_list(ctx: &ToolCtx, args: Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or(".");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let resolved = match sandbox.resolve(std::path::Path::new(path)) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };
    let mut entries = match std::fs::read_dir(&resolved) {
        Ok(iter) => iter,
        Err(e) => return format!("list error: {e}"),
    };
    let mut result = Vec::new();
    while let Some(Ok(entry)) = entries.next() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        result.push(format!("{}{}", name, if is_dir { "/" } else { "" }));
    }
    result.join("\n")
}

pub async fn fs_delete(ctx: &ToolCtx, args: Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or("");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let resolved = match sandbox.resolve(std::path::Path::new(path)) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };
    match tokio::fs::remove_file(&resolved).await {
        Ok(()) => format!("deleted {path}"),
        Err(e) => format!("delete error: {e}"),
    }
}
