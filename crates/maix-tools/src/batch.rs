//! Batch operations — execute multiple similar operations with concurrency control.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Result of a single batch operation.
#[derive(Debug, Clone)]
pub struct BatchItemResult {
    pub target: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

/// Summary of a batch operation.
#[derive(Debug)]
pub struct BatchSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchItemResult>,
}

impl BatchSummary {
    pub fn format(&self) -> String {
        let mut lines = vec![
            format!("Batch complete: {}/{} succeeded, {} failed", self.succeeded, self.total, self.failed),
        ];

        for r in &self.results {
            let icon = if r.success { "+" } else { "!" };
            let detail = if r.output.is_empty() {
                String::new()
            } else {
                format!(" — {}", r.output.lines().next().unwrap_or(""))
            };
            lines.push(format!("  [{icon}] {}{} ({:.1}s)", r.target, detail, r.duration_ms as f64 / 1000.0));
        }

        lines.join("\n")
    }
}

/// Find files matching a glob pattern under root.
fn find_files(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let glob_pattern = if pattern.contains('/') || pattern.contains('\\') {
        root.join(pattern).to_string_lossy().to_string()
    } else {
        root.join("**").join(pattern).to_string_lossy().to_string()
    };

    glob::glob(&glob_pattern)
        .ok()
        .map(|paths| paths.filter_map(|p| p.ok()).filter(|p| p.is_file()).collect())
        .unwrap_or_default()
}

/// Execute a batch file edit (search/replace across multiple files).
pub async fn batch_file_edit(
    root: &Path,
    search: &str,
    replace: &str,
    file_pattern: &str,
    max_concurrent: usize,
) -> MaixResult<BatchSummary> {
    let files = find_files(root, file_pattern);
    let total = files.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for file_path in files {
        let search = search.to_string();
        let replace = replace.to_string();
        let sem = semaphore.clone();
        let rel = file_path.strip_prefix(root).unwrap_or(&file_path).to_string_lossy().to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let start = std::time::Instant::now();

            let result = async {
                let content = tokio::fs::read_to_string(&file_path).await?;
                let count = content.matches(&*search).count();
                if count == 0 {
                    return Ok::<_, std::io::Error>((0, false));
                }
                let new_content = content.replace(&*search, &replace);
                tokio::fs::write(&file_path, &new_content).await?;
                Ok((count, true))
            }.await;

            let duration_ms = start.elapsed().as_millis() as u64;

            match result {
                Ok((count, modified)) => BatchItemResult {
                    target: rel,
                    success: true,
                    output: if modified {
                        format!("{count} replacement(s)")
                    } else {
                        "no matches".into()
                    },
                    duration_ms,
                },
                Err(e) => BatchItemResult {
                    target: rel,
                    success: false,
                    output: e.to_string(),
                    duration_ms,
                },
            }
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(r) = handle.await {
            results.push(r);
        }
    }

    let succeeded = results.iter().filter(|r| r.success).count();
    let failed = total - succeeded;

    Ok(BatchSummary { total, succeeded, failed, results })
}

/// Execute a batch shell command on multiple files.
pub async fn batch_shell_exec(
    root: &Path,
    command_template: &str,
    file_pattern: &str,
    max_concurrent: usize,
) -> MaixResult<BatchSummary> {
    let files = find_files(root, file_pattern);
    let total = files.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for file_path in files {
        let cmd = command_template.replace("{file}", &file_path.to_string_lossy());
        let rel = file_path.strip_prefix(root).unwrap_or(&file_path).to_string_lossy().to_string();
        let sem = semaphore.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let start = std::time::Instant::now();

            #[cfg(target_os = "windows")]
            let output = tokio::process::Command::new("cmd")
                .arg("/C").arg(&cmd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output().await;

            #[cfg(not(target_os = "windows"))]
            let output = tokio::process::Command::new("sh")
                .arg("-c").arg(&cmd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output().await;

            let duration_ms = start.elapsed().as_millis() as u64;

            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let combined = format!("{stdout}{stderr}");
                    let first_line = combined.lines().next().unwrap_or("").to_string();
                    BatchItemResult {
                        target: rel,
                        success: o.status.success(),
                        output: first_line,
                        duration_ms,
                    }
                }
                Err(e) => BatchItemResult {
                    target: rel,
                    success: false,
                    output: e.to_string(),
                    duration_ms,
                },
            }
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(r) = handle.await {
            results.push(r);
        }
    }

    let succeeded = results.iter().filter(|r| r.success).count();
    let failed = total - succeeded;

    Ok(BatchSummary { total, succeeded, failed, results })
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

pub struct BatchEditTool;

#[async_trait]
impl Tool for BatchEditTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "batch_edit".into(),
            description: "Search and replace text across multiple files matching a glob pattern, with concurrency control.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "search": { "type": "string", "description": "Text to search for" },
                    "replace": { "type": "string", "description": "Replacement text" },
                    "file_pattern": { "type": "string", "description": "File glob pattern (e.g. '*.rs')" },
                    "max_concurrent": { "type": "integer", "description": "Max concurrent file operations (default: 8)" }
                },
                "required": ["search", "replace", "file_pattern"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let search = args["search"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'search'".into()))?;
        let replace = args["replace"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'replace'".into()))?;
        let file_pattern = args["file_pattern"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'file_pattern'".into()))?;
        let max_concurrent = args["max_concurrent"].as_u64().unwrap_or(8) as usize;

        let summary = batch_file_edit(&ctx.working_dir, search, replace, file_pattern, max_concurrent).await?;
        Ok(summary.format())
    }
}

pub struct BatchExecTool;

#[async_trait]
impl Tool for BatchExecTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "batch_exec".into(),
            description: "Execute a shell command for each file matching a glob pattern. Use {file} as placeholder for the file path.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command template with {file} placeholder" },
                    "file_pattern": { "type": "string", "description": "File glob pattern (e.g. '*.rs')" },
                    "max_concurrent": { "type": "integer", "description": "Max concurrent operations (default: 4)" }
                },
                "required": ["command", "file_pattern"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let command = args["command"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'command'".into()))?;
        let file_pattern = args["file_pattern"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'file_pattern'".into()))?;
        let max_concurrent = args["max_concurrent"].as_u64().unwrap_or(4) as usize;

        let summary = batch_shell_exec(&ctx.working_dir, command, file_pattern, max_concurrent).await?;
        Ok(summary.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_files() {
        let root = std::path::Path::new(".");
        // Just verify it doesn't panic
        let _ = find_files(root, "*.toml");
    }
}
