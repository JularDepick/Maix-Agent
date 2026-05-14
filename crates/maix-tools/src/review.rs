//! PR review analyzer — detect common issues in code diffs.
//!
//! Pattern-based analysis for security, performance, and quality issues.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Severity of a review finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Suggestion,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
            Self::Suggestion => write!(f, "suggestion"),
        }
    }
}

/// Category of a review finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    Security,
    Performance,
    Quality,
    Style,
    Bug,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Security => write!(f, "security"),
            Self::Performance => write!(f, "performance"),
            Self::Quality => write!(f, "quality"),
            Self::Style => write!(f, "style"),
            Self::Bug => write!(f, "bug"),
        }
    }
}

/// A single review finding.
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Review result with findings and summary.
#[derive(Debug)]
pub struct ReviewResult {
    pub findings: Vec<Finding>,
    pub files_reviewed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

impl ReviewResult {
    pub fn format(&self) -> String {
        let mut lines = vec![
            format!("Review Summary:"),
            format!("  Files reviewed: {}", self.files_reviewed),
            format!("  Lines: +{} -{}", self.lines_added, self.lines_removed),
            format!("  Findings: {}", self.findings.len()),
            String::new(),
        ];

        if self.findings.is_empty() {
            lines.push("No issues found.".to_string());
            return lines.join("\n");
        }

        // Group by severity
        let errors = self.findings.iter().filter(|f| f.severity == Severity::Error).count();
        let warnings = self.findings.iter().filter(|f| f.severity == Severity::Warning).count();
        let infos = self.findings.iter().filter(|f| f.severity == Severity::Info).count();
        let suggestions = self.findings.iter().filter(|f| f.severity == Severity::Suggestion).count();

        lines.push(format!("  {} error(s), {} warning(s), {} info, {} suggestion(s)", errors, warnings, infos, suggestions));
        lines.push(String::new());

        for finding in &self.findings {
            let icon = match finding.severity {
                Severity::Error => "✗",
                Severity::Warning => "⚠",
                Severity::Info => "ℹ",
                Severity::Suggestion => "💡",
            };
            lines.push(format!("{} {}:{} [{}] [{}] {}",
                icon, finding.file, finding.line, finding.severity, finding.category, finding.message));
            if let Some(ref suggestion) = finding.suggestion {
                lines.push(format!("    Suggestion: {}", suggestion));
            }
        }

        lines.join("\n")
    }

    pub fn error_count(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Error).count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Warning).count()
    }
}

/// Analyze a unified diff for common issues.
pub fn analyze_diff(diff: &str) -> ReviewResult {
    let mut findings = Vec::new();
    let mut files_reviewed = 0usize;
    let mut lines_added = 0usize;
    let mut lines_removed = 0usize;
    let mut current_file = String::new();
    let mut current_line = 0usize;

    for line in diff.lines() {
        // Track file changes
        if line.starts_with("+++") {
            current_file = line.trim_start_matches("+++ ").to_string();
            if current_file.starts_with("b/") {
                current_file = current_file[2..].to_string();
            }
            files_reviewed += 1;
            continue;
        }

        if line.starts_with("---") || line.starts_with("@@") {
            // Parse line number from @@ header
            if line.starts_with("@@") {
                if let Some(start) = parse_diff_line_number(line) {
                    current_line = start;
                }
            }
            continue;
        }

        if line.starts_with('+') && !line.starts_with("+++") {
            lines_added += 1;
            let added = &line[1..];
            analyze_line(&current_file, current_line, added, &mut findings);
            current_line += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            lines_removed += 1;
        } else {
            current_line += 1;
        }
    }

    ReviewResult {
        findings,
        files_reviewed,
        lines_added,
        lines_removed,
    }
}

/// Analyze a single added line for issues.
fn analyze_line(file: &str, line: usize, content: &str, findings: &mut Vec<Finding>) {
    let trimmed = content.trim();

    // Security: SQL injection patterns
    if is_sql_injection_risk(trimmed) {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Error,
            category: Category::Security,
            message: "Potential SQL injection: string concatenation in SQL query".to_string(),
            suggestion: Some("Use parameterized queries or prepared statements".to_string()),
        });
    }

    // Security: hardcoded secrets
    if is_hardcoded_secret(trimmed) {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Error,
            category: Category::Security,
            message: "Hardcoded secret or credential detected".to_string(),
            suggestion: Some("Use environment variables or a secrets manager".to_string()),
        });
    }

    // Security: unsafe unwrap
    if trimmed.contains(".unwrap()") && !file.contains("test") {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Warning,
            category: Category::Quality,
            message: "unwrap() may panic in production".to_string(),
            suggestion: Some("Use unwrap_or, unwrap_or_else, or proper error handling with ?".to_string()),
        });
    }

    // Performance: clone in loop
    if trimmed.contains(".clone()") && (trimmed.contains("for ") || trimmed.contains("while ")) {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Warning,
            category: Category::Performance,
            message: "Clone inside loop may cause unnecessary allocations".to_string(),
            suggestion: Some("Consider borrowing or moving the clone outside the loop".to_string()),
        });
    }

    // Performance: collect then iterate
    if trimmed.contains(".collect()") && trimmed.contains(".iter()") {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Info,
            category: Category::Performance,
            message: "Collecting then iterating may be avoidable with direct chaining".to_string(),
            suggestion: None,
        });
    }

    // Quality: TODO/FIXME comments
    if trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("HACK") {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Info,
            category: Category::Quality,
            message: "TODO/FIXME comment found".to_string(),
            suggestion: None,
        });
    }

    // Quality: empty catch block
    if trimmed.contains("catch") && trimmed.contains("{}") {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Warning,
            category: Category::Quality,
            message: "Empty catch block silently swallows errors".to_string(),
            suggestion: Some("Log the error or handle it appropriately".to_string()),
        });
    }

    // Style: very long line
    if content.len() > 200 {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Suggestion,
            category: Category::Style,
            message: format!("Line is very long ({} chars)", content.len()),
            suggestion: Some("Consider breaking into multiple lines".to_string()),
        });
    }

    // Bug: == null vs is_null (Rust-specific)
    if file.ends_with(".rs") && trimmed.contains("== None") {
        findings.push(Finding {
            file: file.to_string(),
            line,
            severity: Severity::Suggestion,
            category: Category::Style,
            message: "Use .is_none() instead of == None".to_string(),
            suggestion: Some("Replace with .is_none()".to_string()),
        });
    }
}

fn is_sql_injection_risk(line: &str) -> bool {
    let lower = line.to_lowercase();
    // Look for SQL keywords combined with string formatting
    let has_sql = lower.contains("select") || lower.contains("insert")
        || lower.contains("update") || lower.contains("delete")
        || lower.contains("where");
    let has_concat = lower.contains("format!") || lower.contains("format!(")
        || lower.contains("+ \"") || lower.contains("'+ '")
        || lower.contains("${") || lower.contains("f\"");
    has_sql && has_concat
}

fn is_hardcoded_secret(line: &str) -> bool {
    let lower = line.to_lowercase();
    let secret_patterns = ["password", "secret", "api_key", "apikey", "token", "credential"];
    let has_secret_keyword = secret_patterns.iter().any(|p| lower.contains(p));
    let has_assignment = lower.contains("= \"") || lower.contains("= '");
    let has_value = line.matches('"').count() >= 2 || line.matches('\'').count() >= 2;

    has_secret_keyword && has_assignment && has_value
}

fn parse_diff_line_number(header: &str) -> Option<usize> {
    // Parse @@ -old,count +new,count @@
    let parts: Vec<&str> = header.split_whitespace().collect();
    for part in &parts {
        if part.starts_with('+') {
            let num_str = part.trim_start_matches('+').split(',').next()?;
            return num_str.parse().ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Analyze a diff for potential issues (security, performance, quality).
pub struct ReviewDiffTool;

#[async_trait]
impl Tool for ReviewDiffTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "review_diff".into(),
            description: "Analyze a unified diff for potential issues: security vulnerabilities, performance problems, code quality concerns.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "diff": { "type": "string", "description": "Unified diff text to analyze" }
                },
                "required": ["diff"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let diff = args["diff"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'diff'".into()))?;

        let result = analyze_diff(diff);
        Ok(result.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_injection_detection() {
        let diff = r#"+++ b/src/auth.rs
@@ -1,0 +1,3 @@
+fn get_user(id: &str) {
+    let query = format!("SELECT * FROM users WHERE id = {}", id);
+}"#;
        let result = analyze_diff(diff);
        assert!(result.findings.iter().any(|f| f.category == Category::Security));
    }

    #[test]
    fn test_hardcoded_secret_detection() {
        let diff = r#"+++ b/src/config.rs
@@ -1,0 +1,2 @@
+const API_KEY = "sk-1234567890abcdef";
+const password = "admin123";"#;
        let result = analyze_diff(diff);
        assert!(result.findings.iter().any(|f| f.category == Category::Security));
    }

    #[test]
    fn test_unwrap_warning() {
        let diff = r#"+++ b/src/lib.rs
@@ -1,0 +1,2 @@
+fn parse(input: &str) -> Value {
+    serde_json::from_str(input).unwrap()
+}"#;
        let result = analyze_diff(diff);
        assert!(result.findings.iter().any(|f| f.message.contains("unwrap")));
    }

    #[test]
    fn test_todo_detection() {
        let diff = r#"+++ b/src/main.rs
@@ -1,0 +1,2 @@
+// TODO: implement this properly
+fn placeholder() {}"#;
        let result = analyze_diff(diff);
        assert!(result.findings.iter().any(|f| f.message.contains("TODO")));
    }

    #[test]
    fn test_clean_diff_no_findings() {
        let diff = r#"+++ b/src/lib.rs
@@ -1,0 +1,3 @@
+fn add(a: i32, b: i32) -> i32 {
+    a + b
+}"#;
        let result = analyze_diff(diff);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn test_review_result_format() {
        let result = ReviewResult {
            findings: vec![
                Finding {
                    file: "src/main.rs".into(),
                    line: 10,
                    severity: Severity::Error,
                    category: Category::Security,
                    message: "SQL injection".into(),
                    suggestion: Some("Use params".into()),
                },
            ],
            files_reviewed: 1,
            lines_added: 5,
            lines_removed: 2,
        };
        let formatted = result.format();
        assert!(formatted.contains("SQL injection"));
        assert!(formatted.contains("error"));
    }

    #[test]
    fn test_parse_diff_line_number() {
        assert_eq!(parse_diff_line_number("@@ -1,5 +10,8 @@"), Some(10));
        assert_eq!(parse_diff_line_number("@@ -0,0 +1,3 @@"), Some(1));
    }
}
