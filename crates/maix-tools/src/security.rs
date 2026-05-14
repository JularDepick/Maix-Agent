//! Security scanning — detect code vulnerabilities, secrets, and common security issues.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Severity of a security finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

/// A security finding.
#[derive(Debug)]
pub struct Finding {
    pub severity: Severity,
    pub category: String,
    pub file: String,
    pub line: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Secret detection pattern.
struct SecretPattern {
    regex: regex::Regex,
    name: &'static str,
}

impl SecretPattern {
    fn new(pattern: &str, name: &'static str) -> Self {
        Self {
            regex: regex::Regex::new(pattern).expect("invalid regex"),
            name,
        }
    }
}

/// Code vulnerability pattern.
struct CodeRule {
    regex: regex::Regex,
    severity: Severity,
    category: &'static str,
    message: &'static str,
    suggestion: &'static str,
}

impl CodeRule {
    fn new(pattern: &str, severity: Severity, category: &'static str, message: &'static str, suggestion: &'static str) -> Self {
        Self {
            regex: regex::Regex::new(pattern).expect("invalid regex"),
            severity, category, message, suggestion,
        }
    }
}

/// Security scanner.
pub struct SecurityScanner;

impl SecurityScanner {
    /// Get secret detection patterns.
    fn secret_patterns() -> Vec<SecretPattern> {
        vec![
            SecretPattern::new(r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*['"][A-Za-z0-9+/=_-]{20,}['"]"#, "API Key"),
            SecretPattern::new(r#"(?i)(secret|password|passwd|pwd)\s*[:=]\s*['"][^'"]{8,}['"]"#, "Password/Secret"),
            SecretPattern::new(r#"(?i)(token)\s*[:=]\s*['"][A-Za-z0-9+/=_-]{20,}['"]"#, "Token"),
            SecretPattern::new(r"sk-[A-Za-z0-9]{20,}", "OpenAI API Key"),
            SecretPattern::new(r"ghp_[A-Za-z0-9]{36}", "GitHub Personal Access Token"),
            SecretPattern::new(r"gho_[A-Za-z0-9]{36}", "GitHub OAuth Token"),
            SecretPattern::new(r"glpat-[A-Za-z0-9-]{20,}", "GitLab Personal Access Token"),
            SecretPattern::new(r"AKIA[0-9A-Z]{16}", "AWS Access Key ID"),
            SecretPattern::new(r#"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[:=]\s*['"][A-Za-z0-9+/]{40}['"]"#, "AWS Secret Access Key"),
            SecretPattern::new(r"xox[baprs]-[A-Za-z0-9-]{10,}", "Slack Token"),
            SecretPattern::new(r"Bearer\s+[A-Za-z0-9._~+/]+=*", "Bearer Token"),
        ]
    }

    /// Get code vulnerability rules.
    fn code_rules() -> Vec<CodeRule> {
        vec![
            // SQL injection
            CodeRule::new(
                r#"(?i)(format!|format\()\s*["'].*SELECT.*WHERE.*\{\}"#,
                Severity::High, "SQL Injection",
                "Potential SQL injection via string formatting",
                "Use parameterized queries instead of string formatting",
            ),
            CodeRule::new(
                r#"(?i)(query|execute)\s*\(\s*&?\s*format!"#,
                Severity::High, "SQL Injection",
                "SQL query built with format! — vulnerable to injection",
                "Use parameterized queries",
            ),
            // Command injection
            CodeRule::new(
                r#"Command::new\([^)]*\)\s*\.arg\(\s*format!"#,
                Severity::Medium, "Command Injection",
                "Shell command built with user-controlled format string",
                "Validate and sanitize input before passing to commands",
            ),
            // Unsafe code
            CodeRule::new(
                r"unsafe\s*\{",
                Severity::Medium, "Unsafe Code",
                "Unsafe code block — review for memory safety",
                "Document safety invariants or use safe alternatives",
            ),
            // unwrap() in non-test code
            CodeRule::new(
                r"\.unwrap\(\)",
                Severity::Low, "Panic Risk",
                "unwrap() will panic on None/Err — use proper error handling",
                "Use .ok_or(), .unwrap_or(), or the ? operator",
            ),
            // expect() with generic message
            CodeRule::new(
                r#"\.expect\("[^"]*"\)"#,
                Severity::Low, "Panic Risk",
                "expect() will panic — consider proper error handling",
                "Use ? operator or map_err for better error context",
            ),
            // Hardcoded localhost/0.0.0.0 binding
            CodeRule::new(
                r#"(?i)(bind|listen)\s*\(\s*["']0\.0\.0\.0"#,
                Severity::Medium, "Network Exposure",
                "Binding to 0.0.0.0 exposes to all network interfaces",
                "Bind to 127.0.0.1 for local-only access",
            ),
            // TLS verification disabled
            CodeRule::new(
                r"(?i)(danger_accept_invalid_certs|verify\s*=\s*false|InsecureSkipVerify)",
                Severity::High, "TLS Verification",
                "TLS certificate verification is disabled",
                "Enable certificate verification in production",
            ),
            // Logging sensitive data
            CodeRule::new(
                r#"(?i)(log|tracing|debug|info|warn|error)!\(.*(?:password|secret|token|key)"#,
                Severity::Medium, "Sensitive Data Logging",
                "Potentially logging sensitive data",
                "Redact sensitive fields before logging",
            ),
        ]
    }

    /// Scan source files for secrets.
    async fn scan_secrets(&self, root: &Path) -> MaixResult<Vec<Finding>> {
        let patterns = Self::secret_patterns();
        let files = Self::collect_source_files(root);
        let mut findings = Vec::new();

        for file_path in &files {
            let content = match tokio::fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = file_path.strip_prefix(root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            for (line_no, line) in content.lines().enumerate() {
                // Skip comments and common false positives
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("*") {
                    continue;
                }

                for pattern in &patterns {
                    if pattern.regex.is_match(line) {
                        findings.push(Finding {
                            severity: Severity::High,
                            category: format!("Secret: {}", pattern.name),
                            file: rel_path.clone(),
                            line: line_no + 1,
                            message: format!("Possible {} detected", pattern.name),
                            suggestion: Some("Move secrets to environment variables or a secrets manager".into()),
                        });
                    }
                }
            }
        }

        Ok(findings)
    }

    /// Scan source files for code vulnerabilities.
    async fn scan_code(&self, root: &Path) -> MaixResult<Vec<Finding>> {
        let rules = Self::code_rules();
        let files = Self::collect_source_files(root);
        let mut findings = Vec::new();

        for file_path in &files {
            let content = match tokio::fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = file_path.strip_prefix(root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            // Skip test files for some rules
            let is_test = rel_path.contains("test") || rel_path.contains("spec");

            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                // Skip comments
                if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with("/*") {
                    continue;
                }

                for rule in &rules {
                    // Skip unwrap/expect warnings in test code
                    if is_test && (rule.category == "Panic Risk") {
                        continue;
                    }

                    if rule.regex.is_match(line) {
                        findings.push(Finding {
                            severity: rule.severity.clone(),
                            category: rule.category.to_string(),
                            file: rel_path.clone(),
                            line: line_no + 1,
                            message: rule.message.to_string(),
                            suggestion: Some(rule.suggestion.to_string()),
                        });
                    }
                }
            }
        }

        Ok(findings)
    }

    /// Collect source files to scan.
    fn collect_source_files(root: &Path) -> Vec<PathBuf> {
        let extensions = ["rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "rb", "php", "cs", "c", "cpp", "h", "hpp", "toml", "yaml", "yml", "json", "env", "ini", "cfg", "conf"];

        let skip_dirs = ["node_modules", ".git", "target", "dist", "build", "__pycache__", ".venv", "vendor"];

        let mut files = Vec::new();
        Self::walk_dir(root, &extensions, &skip_dirs, &mut files);
        files
    }

    fn walk_dir(dir: &Path, extensions: &[&str], skip_dirs: &[&str], files: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if skip_dirs.contains(&dir_name) {
                    continue;
                }
                Self::walk_dir(&path, extensions, skip_dirs, files);
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if extensions.contains(&ext) || name.starts_with(".env") {
                    files.push(path);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

pub struct SecurityScanTool;

#[async_trait]
impl Tool for SecurityScanTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "security_scan".into(),
            description: "Scan source code for security vulnerabilities, hardcoded secrets, and common security issues. Returns findings with severity, location, and fix suggestions.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "scan_type": {
                        "type": "string",
                        "description": "Type of scan: 'all', 'secrets', or 'code' (default: 'all')",
                        "enum": ["all", "secrets", "code"]
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let scan_type = args["scan_type"].as_str().unwrap_or("all");
        let scanner = SecurityScanner;

        let mut findings = Vec::new();

        if scan_type == "all" || scan_type == "secrets" {
            findings.extend(scanner.scan_secrets(&ctx.working_dir).await?);
        }
        if scan_type == "all" || scan_type == "code" {
            findings.extend(scanner.scan_code(&ctx.working_dir).await?);
        }

        if findings.is_empty() {
            return Ok("Security scan complete: no issues found.".into());
        }

        // Sort by severity
        findings.sort_by(|a, b| {
            let order = |s: &Severity| match s {
                Severity::Critical => 0,
                Severity::High => 1,
                Severity::Medium => 2,
                Severity::Low => 3,
                Severity::Info => 4,
            };
            order(&a.severity).cmp(&order(&b.severity))
        });

        let critical = findings.iter().filter(|f| f.severity == Severity::Critical).count();
        let high = findings.iter().filter(|f| f.severity == Severity::High).count();
        let medium = findings.iter().filter(|f| f.severity == Severity::Medium).count();
        let low = findings.iter().filter(|f| f.severity == Severity::Low).count();

        let mut lines = vec![
            format!("Security scan: {} findings (Critical: {} | High: {} | Medium: {} | Low: {})", findings.len(), critical, high, medium, low),
            "".to_string(),
        ];

        for (i, finding) in findings.iter().enumerate().take(20) {
            let icon = match finding.severity {
                Severity::Critical | Severity::High => "!!",
                Severity::Medium => "! ",
                Severity::Low | Severity::Info => "  ",
            };
            lines.push(format!("{}. [{}] {} {}:{}", i + 1, icon, finding.severity, finding.file, finding.line));
            lines.push(format!("   {}: {}", finding.category, finding.message));
            if let Some(ref suggestion) = finding.suggestion {
                lines.push(format!("   Fix: {}", suggestion));
            }
            lines.push("".to_string());
        }

        if findings.len() > 20 {
            lines.push(format!("... and {} more findings", findings.len() - 20));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_patterns_match() {
        let patterns = SecurityScanner::secret_patterns();

        let test_cases = vec![
            (r#"api_key = "sk-abc123def456ghi789jkl012mno345pqr678stu901v""#, true),
            (r#"password = "supersecretpassword123""#, true),
            (r#"ghp_1234567890abcdef1234567890abcdef1234"#, true),
            (r#"AKIA1234567890ABCDEF"#, true),
            (r#"let name = "hello world""#, false),
        ];

        for (input, should_match) in test_cases {
            let matched = patterns.iter().any(|p| p.regex.is_match(input));
            assert_eq!(matched, should_match, "Failed for: {input}");
        }
    }

    #[test]
    fn test_code_rules_match() {
        let rules = SecurityScanner::code_rules();

        assert!(rules.iter().any(|r| r.regex.is_match("unsafe { ptr::read(p) }")));
        assert!(rules.iter().any(|r| r.regex.is_match("something.unwrap()")));
        assert!(rules.iter().any(|r| r.regex.is_match(r#"bind("0.0.0.0:8080")"#)));
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::High.to_string(), "HIGH");
        assert_eq!(Severity::Low.to_string(), "LOW");
    }
}
