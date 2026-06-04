//! Auto-format integration — detect project formatters and format after edits.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::Path;

/// A detected formatter.
#[derive(Debug, Clone)]
pub struct Formatter {
    pub name: String,
    pub command: String,
    pub extensions: Vec<String>,
}

impl Formatter {
    /// Detect formatters for the given project root.
    pub fn detect_all(project_root: &Path) -> Vec<Formatter> {
        let mut formatters = Vec::new();

        // Rust: rustfmt
        if project_root.join("Cargo.toml").exists() {
            formatters.push(Formatter {
                name: "rustfmt".into(),
                command: "rustfmt".into(),
                extensions: vec!["rs".into()],
            });
        }

        // JavaScript/TypeScript: prettier
        if project_root.join(".prettierrc").exists()
            || project_root.join("prettier.config.js").exists()
            || project_root.join("prettier.config.mjs").exists()
            || project_root.join("package.json").exists()
        {
            formatters.push(Formatter {
                name: "prettier".into(),
                command: "npx prettier --write".into(),
                extensions: vec!["js".into(), "ts".into(), "tsx".into(), "jsx".into(), "json".into(), "css".into(), "md".into()],
            });
        }

        // Python: black, ruff
        if project_root.join("pyproject.toml").exists() || project_root.join("setup.py").exists() {
            // Prefer ruff if available
            formatters.push(Formatter {
                name: "ruff".into(),
                command: "ruff format".into(),
                extensions: vec!["py".into()],
            });
            formatters.push(Formatter {
                name: "black".into(),
                command: "black".into(),
                extensions: vec!["py".into()],
            });
        }

        // Go: gofmt
        if project_root.join("go.mod").exists() {
            formatters.push(Formatter {
                name: "gofmt".into(),
                command: "gofmt -w".into(),
                extensions: vec!["go".into()],
            });
        }

        formatters
    }

    /// Check if this formatter handles the given file extension.
    pub fn handles(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext)
    }
}

/// Auto-format a file using the appropriate project formatter.
pub async fn auto_format(file_path: &Path, formatters: &[Formatter]) -> MaixResult<Option<String>> {
    let ext = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    for formatter in formatters {
        if !formatter.handles(ext) {
            continue;
        }

        // Check if the command exists
        let cmd_name = formatter.command.split_whitespace().next().unwrap_or("");
        if !crate::lsp::which(cmd_name) {
            continue;
        }

        let args: Vec<&str> = formatter.command.split_whitespace().skip(1).collect();
        let output = tokio::process::Command::new(cmd_name)
            .args(&args)
            .arg(file_path)
            .output()
            .await;

        match output {
            Ok(o) if o.status.success() => {
                return Ok(Some(format!("Formatted with {}", formatter.name)));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                if !stderr.is_empty() {
                    return Ok(Some(format!("{} warning: {}", formatter.name, stderr.trim())));
                }
            }
            Err(_) => continue,
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Format a file using the project's formatter.
pub struct FormatTool;

#[async_trait]
impl Tool for FormatTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "format".into(),
            description: "Format a source file using the project's configured formatter (rustfmt, prettier, black, gofmt, etc).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Path to the file to format" }
                },
                "required": ["file_path"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let file_path = args["file_path"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'file_path'".into()))?;

        let path = crate::normalize_path(&ctx.working_dir.join(file_path));
        let formatters = Formatter::detect_all(&ctx.working_dir);

        match auto_format(&path, &formatters).await? {
            Some(msg) => Ok(msg),
            None => Ok("No formatter found for this file type.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatter_handles() {
        let f = Formatter {
            name: "rustfmt".into(),
            command: "rustfmt".into(),
            extensions: vec!["rs".into()],
        };
        assert!(f.handles("rs"));
        assert!(!f.handles("py"));
    }

    #[test]
    fn test_formatter_handles_multiple_extensions() {
        let f = Formatter {
            name: "prettier".into(),
            command: "npx prettier --write".into(),
            extensions: vec!["js".into(), "ts".into(), "tsx".into()],
        };
        assert!(f.handles("js"));
        assert!(f.handles("ts"));
        assert!(f.handles("tsx"));
        assert!(!f.handles("py"));
    }

    #[test]
    fn test_formatter_handles_empty_extensions() {
        let f = Formatter {
            name: "custom".into(),
            command: "custom-fmt".into(),
            extensions: vec![],
        };
        assert!(!f.handles("rs"));
    }
}
