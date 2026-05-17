//! LSP client and post-edit diagnostics.
//!
//! Provides both CLI-based diagnostics (cargo check, pyright, tsc, etc.)
//! and a full LSP client for go-to-definition, find-references, hover, etc.

mod client;
mod manager;
pub mod tools;

pub use client::LspClient;
pub use manager::LspManager;

use maix_core::MaixResult;
use std::path::Path;

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

/// A single diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: Severity,
    pub message: String,
    pub source: String,
}

impl Diagnostic {
    /// Format for display in tool results.
    pub fn display(&self) -> String {
        let icon = match self.severity {
            Severity::Error => "✗",
            Severity::Warning => "⚠",
            Severity::Info => "ℹ",
            Severity::Hint => "→",
        };
        let loc = match (self.line, self.column) {
            (Some(l), Some(c)) => format!(":{}:{}", l, c),
            (Some(l), None) => format!(":{}", l),
            _ => String::new(),
        };
        format!("  {} {}{} — {}", icon, self.file, loc, self.message)
    }
}

/// Detect the diagnostic tool for a file based on extension.
fn detect_tool(path: &Path) -> Option<(&'static str, Vec<&'static str>)> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(("cargo", vec!["check", "--message-format=short"])),
        "py" => {
            if which("pyright") {
                Some(("pyright", vec!["--outputjson"]))
            } else if which("mypy") {
                Some(("mypy", vec!["--no-color-output", "--show-column-numbers"]))
            } else {
                None
            }
        }
        "ts" | "tsx" => {
            if which("tsc") {
                Some(("tsc", vec!["--noEmit", "--pretty", "false"]))
            } else {
                None
            }
        }
        "js" | "jsx" => {
            if which("eslint") {
                Some(("eslint", vec!["--format=compact"]))
            } else {
                None
            }
        }
        "go" => {
            if which("gopls") {
                Some(("gopls", vec!["check"]))
            } else if which("golangci-lint") {
                Some(("golangci-lint", vec!["run", "--out-format=line-number"]))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if a command is available on PATH.
pub fn which(cmd: &str) -> bool {
    std::process::Command::new(if cfg!(target_os = "windows") { "where" } else { "which" })
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run diagnostics on a file after an edit.
/// Returns a formatted string of diagnostics, or None if no tool is available.
pub async fn run_diagnostics(path: &Path, working_dir: &Path) -> MaixResult<Option<String>> {
    let (cmd, args) = match detect_tool(path) {
        Some(t) => t,
        None => return Ok(None),
    };

    let output = tokio::process::Command::new(cmd)
        .args(&args)
        .current_dir(working_dir)
        .output()
        .await
        .map_err(maix_core::MaixError::Io)?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    let diagnostics = parse_diagnostics(cmd, &combined, path);

    if diagnostics.is_empty() {
        if output.status.success() {
            return Ok(None);
        }
        let trimmed = combined.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        return Ok(Some(format!("Diagnostic output ({}):\n{}", cmd, trimmed)));
    }

    let mut result = format!("Diagnostics ({}):\n", cmd);
    for diag in diagnostics.iter().take(20) {
        result.push_str(&diag.display());
        result.push('\n');
    }
    if diagnostics.len() > 20 {
        result.push_str(&format!("  ... and {} more\n", diagnostics.len() - 20));
    }

    Ok(Some(result))
}

/// Parse diagnostics from tool output.
fn parse_diagnostics(tool: &str, output: &str, target_file: &Path) -> Vec<Diagnostic> {
    match tool {
        "cargo" => parse_cargo_short(output, target_file),
        "pyright" => parse_pyright_json(output, target_file),
        "mypy" => parse_mypy(output, target_file),
        "tsc" => parse_tsc(output, target_file),
        "eslint" => parse_eslint(output, target_file),
        "gopls" => parse_gopls(output, target_file),
        "golangci-lint" => parse_golangci_lint(output, target_file),
        _ => Vec::new(),
    }
}

/// Parse `cargo check --message-format=short` output.
fn parse_cargo_short(output: &str, _target: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('{') {
            continue;
        }

        let (loc_and_severity, message) = if let Some(pos) = line.find(": error") {
            (&line[..pos], &line[pos + 2..])
        } else if let Some(pos) = line.find(": warning") {
            (&line[..pos], &line[pos + 2..])
        } else if let Some(pos) = line.find(": note") {
            (&line[..pos], &line[pos + 2..])
        } else {
            continue;
        };

        let loc_parts: Vec<&str> = loc_and_severity.split(':').collect();
        if loc_parts.len() < 2 {
            continue;
        }

        let (severity, msg) = if message.starts_with("error") {
            (Severity::Error, &message[message.find(": ").map(|p| p + 2).unwrap_or(0)..])
        } else if message.starts_with("warning") {
            (Severity::Warning, &message[message.find(": ").map(|p| p + 2).unwrap_or(0)..])
        } else if message.starts_with("note") || message.starts_with("help") {
            (Severity::Info, &message[message.find(": ").map(|p| p + 2).unwrap_or(0)..])
        } else {
            (Severity::Warning, message)
        };

        diags.push(Diagnostic {
            file: loc_parts[0].to_string(),
            line: loc_parts.get(1).and_then(|l| l.parse().ok()),
            column: loc_parts.get(2).and_then(|c| c.parse().ok()),
            severity,
            message: msg.to_string(),
            source: "cargo".into(),
        });
    }
    diags
}

/// Parse pyright JSON output.
fn parse_pyright_json(output: &str, _target: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let json: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return parse_fallback(output),
    };

    if let Some(diagnostics) = json.get("generalDiagnostics").and_then(|d| d.as_array()) {
        for d in diagnostics {
            let file = d.get("file").and_then(|f| f.as_str()).unwrap_or("unknown");
            let range = d.get("range");
            let line = range.and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).map(|l| l as u32 + 1);
            let col = range.and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).map(|c| c as u32 + 1);
            let severity = match d.get("severity").and_then(|s| s.as_str()) {
                Some("error") => Severity::Error,
                Some("warning") => Severity::Warning,
                Some("information") => Severity::Info,
                _ => Severity::Hint,
            };
            let message = d.get("message").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
            diags.push(Diagnostic { file: file.to_string(), line, column: col, severity, message, source: "pyright".into() });
        }
    }
    diags
}

/// Parse mypy output.
fn parse_mypy(output: &str, _target: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let (loc_part, rest) = if let Some(pos) = line.find(": error: ") {
            (&line[..pos], &line[pos + 9..])
        } else if let Some(pos) = line.find(": warning: ") {
            (&line[..pos], &line[pos + 11..])
        } else if let Some(pos) = line.find(": note: ") {
            (&line[..pos], &line[pos + 8..])
        } else { continue; };
        let severity = if line.contains(": error: ") { Severity::Error }
            else if line.contains(": warning: ") { Severity::Warning }
            else { Severity::Info };
        let loc_parts: Vec<&str> = loc_part.split(':').collect();
        diags.push(Diagnostic {
            file: loc_parts[0].to_string(),
            line: loc_parts.get(1).and_then(|l| l.parse().ok()),
            column: loc_parts.get(2).and_then(|c| c.parse().ok()),
            severity, message: rest.to_string(), source: "mypy".into(),
        });
    }
    diags
}

/// Parse TypeScript tsc output.
fn parse_tsc(output: &str, _target: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some(paren_start) = line.find('(') {
            let file = &line[..paren_start];
            let rest = &line[paren_start + 1..];
            if let Some(paren_end) = rest.find(')') {
                let loc = &rest[..paren_end];
                let after = &rest[paren_end + 2..];
                let loc_parts: Vec<&str> = loc.split(',').collect();
                let after_parts: Vec<&str> = after.splitn(2, ": ").collect();
                if after_parts.len() >= 2 {
                    let severity = if after_parts[0].contains("error") { Severity::Error }
                        else if after_parts[0].contains("warning") { Severity::Warning }
                        else { Severity::Info };
                    diags.push(Diagnostic {
                        file: file.to_string(),
                        line: loc_parts.first().and_then(|l| l.parse().ok()),
                        column: loc_parts.get(1).and_then(|c| c.parse().ok()),
                        severity, message: after_parts[1].to_string(), source: "tsc".into(),
                    });
                }
            }
        }
    }
    diags
}

/// Parse eslint compact output.
fn parse_eslint(output: &str, _target: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('/') { continue; }
        if let Some(line_pos) = line.find(" line ") {
            let file = &line[..line_pos];
            let rest = &line[line_pos + 6..];
            let parts: Vec<&str> = rest.splitn(4, ", ").collect();
            if parts.len() >= 4 {
                let severity = if parts[2].contains("Error") { Severity::Error }
                    else if parts[2].contains("Warning") { Severity::Warning }
                    else { Severity::Info };
                let msg = parts[3].trim_end_matches(')');
                let msg = if let Some(paren) = msg.rfind(" (") { &msg[..paren] } else { msg };
                diags.push(Diagnostic {
                    file: file.to_string(),
                    line: parts[0].trim().parse().ok(),
                    column: parts[1].trim().strip_prefix("col ").and_then(|c| c.parse().ok()),
                    severity, message: msg.to_string(), source: "eslint".into(),
                });
            }
        }
    }
    diags
}

fn parse_gopls(output: &str, _target: &Path) -> Vec<Diagnostic> { parse_cargo_short(output, _target) }
fn parse_golangci_lint(output: &str, _target: &Path) -> Vec<Diagnostic> { parse_cargo_short(output, _target) }

/// Fallback parser for unknown formats.
fn parse_fallback(output: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("error") || lower.contains("warning") {
            let severity = if lower.contains("error") { Severity::Error } else { Severity::Warning };
            diags.push(Diagnostic {
                file: String::new(), line: None, column: None, severity,
                message: line.trim().to_string(), source: "unknown".into(),
            });
        }
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_short() {
        let output = r#"src/main.rs:15:5: warning: unused variable `x`
src/main.rs:23:10: error[E0308]: mismatched types
src/lib.rs:5:1: warning: unused import `std::io`"#;
        let diags = parse_cargo_short(output, Path::new("src/main.rs"));
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].line, Some(15));
        assert_eq!(diags[1].severity, Severity::Error);
        assert_eq!(diags[1].line, Some(23));
    }

    #[test]
    fn test_parse_mypy() {
        let output = r#"main.py:5:1: error: Incompatible types in assignment  [assignment]
main.py:10:5: warning: Unused import  [unused-import]"#;
        let diags = parse_mypy(output, Path::new("main.py"));
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, Some(5));
    }

    #[test]
    fn test_parse_tsc() {
        let output = r#"src/app.ts(15,5): error TS2322: Type 'string' is not assignable to type 'number'.
src/utils.ts(3,1): warning TS6133: 'foo' is declared but its value is never read."#;
        let diags = parse_tsc(output, Path::new("src/app.ts"));
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].file, "src/app.ts");
        assert_eq!(diags[0].line, Some(15));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn test_diagnostic_display() {
        let diag = Diagnostic {
            file: "src/main.rs".into(), line: Some(15), column: Some(5),
            severity: Severity::Warning, message: "unused variable `x`".into(), source: "cargo".into(),
        };
        let display = diag.display();
        assert!(display.contains("⚠"));
        assert!(display.contains("15:5"));
        assert!(display.contains("unused variable"));
    }

    #[test]
    fn test_detect_tool() {
        assert!(detect_tool(Path::new("main.rs")).is_some());
        detect_tool(Path::new("main.py"));
        detect_tool(Path::new("main.ts"));
        detect_tool(Path::new("main.go"));
        assert!(detect_tool(Path::new("main.rb")).is_none());
    }
}
