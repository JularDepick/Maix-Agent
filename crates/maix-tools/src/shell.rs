//! Shell execution tools — command execution, process management.

use crate::sandbox::WorkDirSandbox;
use crate::{ToolCtx, ToolResult};
use serde_json::Value;

pub async fn shell_exec(ctx: &ToolCtx, args: Value) -> ToolResult {
    let command = args["command"].as_str().unwrap_or("");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let cwd = args["cwd"]
        .as_str()
        .map(|p| sandbox.resolve(std::path::Path::new(p)))
        .unwrap_or_else(|| Ok(ctx.working_dir.clone()));
    let cwd = match cwd {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };

    let output = match std::process::Command::new(if cfg!(target_os = "windows") { "cmd" } else { "sh" })
        .arg(if cfg!(target_os = "windows") { "/C" } else { "-c" })
        .arg(command)
        .current_dir(&cwd)
        .output()
    {
        Ok(o) => o,
        Err(e) => return format!("exec error: {e}"),
    };

    let mut result = String::new();
    if !output.stdout.is_empty() {
        result.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if result.is_empty() {
        result = format!("exit code: {}", output.status.code().unwrap_or(-1));
    }
    result
}

pub async fn shell_spawn(ctx: &ToolCtx, args: Value) -> ToolResult {
    let command = args["command"].as_str().unwrap_or("");
    let sandbox = WorkDirSandbox::new(ctx.working_dir.clone());
    let cwd = match sandbox.resolve(std::path::Path::new(".")) {
        Ok(p) => p,
        Err(e) => return format!("sandbox error: {e}"),
    };

    match std::process::Command::new(if cfg!(target_os = "windows") { "cmd" } else { "sh" })
        .arg(if cfg!(target_os = "windows") { "/C" } else { "-c" })
        .arg(command)
        .current_dir(&cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => format!("spawned pid {}", child.id()),
        Err(e) => format!("spawn error: {e}"),
    }
}
