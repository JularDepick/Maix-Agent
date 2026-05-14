//! Test runner integration — detect framework, run tests, parse results.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Detected test framework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestFramework {
    CargoTest,
    Jest,
    Pytest,
    GoTest,
    Maven,
    Gradle,
}

impl std::fmt::Display for TestFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CargoTest => write!(f, "cargo-test"),
            Self::Jest => write!(f, "jest"),
            Self::Pytest => write!(f, "pytest"),
            Self::GoTest => write!(f, "go-test"),
            Self::Maven => write!(f, "maven"),
            Self::Gradle => write!(f, "gradle"),
        }
    }
}

/// Parsed test result.
#[derive(Debug)]
pub struct TestResult {
    pub framework: TestFramework,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub duration: Duration,
    pub failures: Vec<TestFailure>,
    pub raw_output: String,
}

/// A single test failure.
#[derive(Debug)]
pub struct TestFailure {
    pub name: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

/// Detect and run project tests.
pub struct TestRunner {
    pub framework: TestFramework,
    pub project_root: PathBuf,
}

impl TestRunner {
    /// Auto-detect test framework from project files.
    pub fn detect(project_root: &Path) -> Option<Self> {
        if project_root.join("Cargo.toml").exists() {
            return Some(Self { framework: TestFramework::CargoTest, project_root: project_root.to_path_buf() });
        }
        if project_root.join("package.json").exists() {
            return Some(Self { framework: TestFramework::Jest, project_root: project_root.to_path_buf() });
        }
        if project_root.join("go.mod").exists() {
            return Some(Self { framework: TestFramework::GoTest, project_root: project_root.to_path_buf() });
        }
        if project_root.join("pyproject.toml").exists() || project_root.join("pytest.ini").exists() {
            return Some(Self { framework: TestFramework::Pytest, project_root: project_root.to_path_buf() });
        }
        if project_root.join("pom.xml").exists() {
            return Some(Self { framework: TestFramework::Maven, project_root: project_root.to_path_buf() });
        }
        if project_root.join("build.gradle").exists() || project_root.join("build.gradle.kts").exists() {
            return Some(Self { framework: TestFramework::Gradle, project_root: project_root.to_path_buf() });
        }
        None
    }

    /// Run tests with optional filter.
    pub async fn run(&self, filter: Option<&str>) -> MaixResult<TestResult> {
        let (cmd, args) = match self.framework {
            TestFramework::CargoTest => {
                let mut a = vec!["test", "--color=never"];
                if let Some(f) = filter {
                    a.push(f);
                }
                ("cargo", a)
            }
            TestFramework::Jest => {
                let mut a = vec!["test", "--no-color"];
                if let Some(f) = filter {
                    a.push("--testNamePattern");
                    a.push(f);
                }
                ("npx", a)
            }
            TestFramework::Pytest => {
                let mut a = vec!["--tb=short", "--no-header"];
                if let Some(f) = filter {
                    a.push("-k");
                    a.push(f);
                }
                ("pytest", a)
            }
            TestFramework::GoTest => {
                let mut a = vec!["test", "./..."];
                if let Some(f) = filter {
                    a.push("-run");
                    a.push(f);
                }
                ("go", a)
            }
            TestFramework::Maven => {
                ("mvn", vec!["test"])
            }
            TestFramework::Gradle => {
                ("gradle", vec!["test"])
            }
        };

        let output = tokio::process::Command::new(cmd)
            .args(&args)
            .current_dir(&self.project_root)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("Failed to run {cmd}: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        let result = match self.framework {
            TestFramework::CargoTest => Self::parse_cargo_test(&combined),
            TestFramework::Jest => Self::parse_jest(&combined),
            TestFramework::Pytest => Self::parse_pytest(&combined),
            TestFramework::GoTest => Self::parse_go_test(&combined),
            _ => Self::parse_generic(&combined, self.framework.clone()),
        };

        Ok(result)
    }

    fn parse_cargo_test(output: &str) -> TestResult {
        let mut total = 0u32;
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut ignored = 0u32;
        let mut failures = Vec::new();

        for line in output.lines() {
            // "test result: ok. 42 passed; 2 failed; 1 ignored"
            if line.contains("test result:") {
                for part in line.split(';') {
                    let part = part.trim();
                    if let Some(n) = parse_count_before(part, "passed") {
                        passed = n;
                    } else if let Some(n) = parse_count_before(part, "failed") {
                        failed = n;
                    } else if let Some(n) = parse_count_before(part, "ignored") {
                        ignored = n;
                    }
                }
                total = passed + failed + ignored;
            }

            // "test auth::tests::test_login ... FAILED"
            if line.contains("FAILED") && line.contains("test ") {
                if let Some(name) = line.split_whitespace().nth(1) {
                    failures.push(TestFailure {
                        name: name.to_string(),
                        file: None,
                        line: None,
                        message: String::new(),
                    });
                }
            }
        }

        // Extract failure messages
        let mut in_failure = false;
        let mut current_failure_idx = 0;
        for line in output.lines() {
            if line.starts_with("---- ") && line.contains("stdout") {
                in_failure = true;
                continue;
            }
            if in_failure && line.starts_with("---- ") {
                in_failure = false;
                current_failure_idx = failures.len().saturating_sub(1);
                continue;
            }
            if in_failure && current_failure_idx < failures.len() {
                if !failures[current_failure_idx].message.is_empty() {
                    failures[current_failure_idx].message.push('\n');
                }
                failures[current_failure_idx].message.push_str(line);
            }
        }

        TestResult {
            framework: TestFramework::CargoTest,
            total, passed, failed, ignored,
            duration: Duration::default(),
            failures,
            raw_output: output.to_string(),
        }
    }

    fn parse_jest(output: &str) -> TestResult {
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut failures = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.starts_with("Tests:") || line.starts_with("Test Suites:") {
                for part in line.split(',') {
                    let part = part.trim();
                    if let Some(n) = parse_count_before(part, "passed") {
                        passed += n;
                    } else if let Some(n) = parse_count_before(part, "failed") {
                        failed += n;
                    }
                }
            }
            if let Some(rest) = line.strip_prefix("FAIL ") {
                failures.push(TestFailure {
                    name: rest.to_string(),
                    file: Some(rest.to_string()),
                    line: None,
                    message: String::new(),
                });
            }
        }
        let total = passed + failed;

        TestResult {
            framework: TestFramework::Jest,
            total, passed, failed, ignored: 0,
            duration: Duration::default(),
            failures,
            raw_output: output.to_string(),
        }
    }

    fn parse_pytest(output: &str) -> TestResult {
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut failures = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            // "=== 2 failed, 5 passed in 1.23s ==="
            if line.starts_with("===") && line.contains("in ") {
                for part in line.trim_start_matches('=').split(',') {
                    let part = part.trim();
                    if let Some(n) = parse_count_before(part, "passed") {
                        passed = n;
                    } else if let Some(n) = parse_count_before(part, "failed") {
                        failed = n;
                    }
                }
            }
            // "FAILED test_file.py::test_name - AssertionError"
            if line.starts_with("FAILED ") {
                let parts: Vec<&str> = line.splitn(3, " - ").collect();
                let name = parts[0].strip_prefix("FAILED ").unwrap_or(parts[0]);
                failures.push(TestFailure {
                    name: name.to_string(),
                    file: None,
                    line: None,
                    message: parts.get(1).unwrap_or(&"").to_string(),
                });
            }
        }
        let total = passed + failed;

        TestResult {
            framework: TestFramework::Pytest,
            total, passed, failed, ignored: 0,
            duration: Duration::default(),
            failures,
            raw_output: output.to_string(),
        }
    }

    fn parse_go_test(output: &str) -> TestResult {
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut failures = Vec::new();

        for line in output.lines() {
            // "--- FAIL: TestName (0.00s)"
            if line.starts_with("--- FAIL:") {
                let name = line.trim_start_matches("--- FAIL:").trim();
                let name = name.split_whitespace().next().unwrap_or(name);
                failures.push(TestFailure {
                    name: name.to_string(),
                    file: None,
                    line: None,
                    message: String::new(),
                });
                failed += 1;
            }
            if line.starts_with("--- PASS:") {
                passed += 1;
            }
        }
        let total = passed + failed;

        TestResult {
            framework: TestFramework::GoTest,
            total, passed, failed, ignored: 0,
            duration: Duration::default(),
            failures,
            raw_output: output.to_string(),
        }
    }

    fn parse_generic(output: &str, framework: TestFramework) -> TestResult {
        TestResult {
            framework,
            total: 0, passed: 0, failed: 0, ignored: 0,
            duration: Duration::default(),
            failures: Vec::new(),
            raw_output: output.to_string(),
        }
    }
}

/// Parse a number before a keyword: "42 passed" → Some(42).
/// Also handles "test result: ok. 42 passed" by finding the keyword.
fn parse_count_before(text: &str, keyword: &str) -> Option<u32> {
    let text = text.trim();
    // Find the keyword and look backwards for the number
    if let Some(pos) = text.find(keyword) {
        let before = text[..pos].trim();
        // The number should be the last whitespace-separated token before the keyword
        before.split_whitespace().last()?.parse().ok()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Run project tests.
pub struct TestRunTool;

#[async_trait]
impl Tool for TestRunTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "test_run".into(),
            description: "Run the project's test suite. Auto-detects the testing framework (cargo test, jest, pytest, go test, etc). Returns structured results with pass/fail counts and failure details.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Optional test name filter" }
                }
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let filter = args["filter"].as_str();
        let runner = TestRunner::detect(&ctx.working_dir)
            .ok_or_else(|| maix_core::MaixError::Tool("No test framework detected in this project.".into()))?;

        let result = runner.run(filter).await?;

        let mut lines = vec![
            format!("Test framework: {}", result.framework),
            format!("Total: {} | Passed: {} | Failed: {} | Ignored: {}", result.total, result.passed, result.failed, result.ignored),
        ];

        if !result.failures.is_empty() {
            lines.push("".to_string());
            lines.push("Failures:".to_string());
            for (i, f) in result.failures.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, f.name));
                if !f.message.is_empty() {
                    for msg_line in f.message.lines().take(3) {
                        lines.push(format!("     {msg_line}"));
                    }
                }
            }
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_test() {
        let output = "test result: ok. 42 passed; 2 failed; 1 ignored";
        let result = TestRunner::parse_cargo_test(output);
        assert_eq!(result.passed, 42);
        assert_eq!(result.failed, 2);
        assert_eq!(result.ignored, 1);
    }

    #[test]
    fn test_parse_pytest() {
        let output = "=== 2 failed, 5 passed in 1.23s ===";
        let result = TestRunner::parse_pytest(output);
        assert_eq!(result.passed, 5);
        assert_eq!(result.failed, 2);
    }

    #[test]
    fn test_parse_count_before() {
        assert_eq!(parse_count_before("42 passed", "passed"), Some(42));
        assert_eq!(parse_count_before("2 failed", "failed"), Some(2));
        assert_eq!(parse_count_before("test result: ok. 42 passed", "passed"), Some(42));
        assert_eq!(parse_count_before("nope", "passed"), None);
    }
}
