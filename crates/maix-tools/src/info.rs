//! Information tools — system info, directory structure, environment variables.

use crate::{ToolCtx, ToolResult};
use serde_json::Value;

pub async fn sys_info(_ctx: &ToolCtx, _args: Value) -> ToolResult {
    let mut info = Vec::new();
    info.push(format!("os: {}", std::env::consts::OS));
    info.push(format!("arch: {}", std::env::consts::ARCH));
    info.push(format!("cwd: {:?}", std::env::current_dir().unwrap_or_default()));
    if let Ok(home) = std::env::var(if cfg!(target_os = "windows") { "USERPROFILE" } else { "HOME" }) {
        info.push(format!("home: {home}"));
    }
    info.join("\n")
}

pub async fn dir_tree(ctx: &ToolCtx, args: Value) -> ToolResult {
    use crate::sandbox::WorkDirSandbox;
    let path = args["path"].as_str().unwrap_or(".");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let root = match sandbox.resolve(std::path::Path::new(path)) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };

    let mut result = Vec::new();
    walk_dir(&root, &root, &mut result, 0, 3);
    result.join("\n")
}

fn walk_dir(root: &std::path::Path, current: &std::path::Path, out: &mut Vec<String>, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else { return };
    let indent = "  ".repeat(depth);
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            out.push(format!("{indent}{name}/"));
            walk_dir(root, &entry.path(), out, depth + 1, max_depth);
        } else {
            out.push(format!("{indent}{name}"));
        }
    }
}

pub async fn env_vars(_ctx: &ToolCtx, _args: Value) -> ToolResult {
    let mut vars: Vec<String> = std::env::vars()
        .filter(|(k, _)| {
            !k.contains("SECRET") && !k.contains("KEY") && !k.contains("TOKEN") && !k.contains("PASSWORD")
        })
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    vars.sort();
    vars.join("\n")
}
