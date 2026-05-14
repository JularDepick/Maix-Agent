//! Code coverage — run tests with coverage and parse results.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Detected coverage tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageTool {
    CargoTarpaulin,
    JestCoverage,
    PytestCov,
    GoCover,
}

impl std::fmt::Display for CoverageTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CargoTarpaulin => write!(f, "cargo-tarpaulin"),
            Self::JestCoverage => write!(f, "jest --coverage"),
            Self::PytestCov => write!(f, "pytest-cov"),
            Self::GoCover => write!(f, "go test -cover"),
        }
    }
}

/// Coverage result for a single file.
#[derive(Debug, Clone)]
pub struct FileCoverage {
    pub path: String,
    pub total_lines: u32,
    pub covered_lines: u32,
    pub coverage_percent: f32,
}

/// Overall coverage report.
#[derive(Debug)]
pub struct CoverageReport {
    pub tool: CoverageTool,
    pub total_lines: u32,
    pub covered_lines: u32,
    pub coverage_percent: f32,
    pub files: Vec<FileCoverage>,
    pub raw_output: String,
}

impl CoverageReport {
    /// Format the report for display.
    pub fn format(&self) -> String {
        let mut lines = vec![
            format!("Coverage tool: {}", self.tool),
            format!(
                "Total: {:.1}% ({}/{})",
                self.coverage_percent, self.covered_lines, self.total_lines
            ),
        ];

        if !self.files.is_empty() {
            lines.push("".to_string());
            lines.push("Per-file coverage:".to_string());

            let mut sorted = self.files.clone();
            sorted.sort_by(|a, b| a.coverage_percent.partial_cmp(&b.coverage_percent).unwrap());

            for f in &sorted {
                let bar_len = 20;
                let filled = (f.coverage_percent / 100.0 * bar_len as f32) as usize;
                let bar: String = "█".repeat(filled) + &"░".repeat(bar_len - filled);
                lines.push(format!(
                    "  {:>5.1}% {} {}",
                    f.coverage_percent, bar, f.path
                ));
            }
        }

        lines.join("\n")
    }
}

/// Detect and run coverage tools.
pub struct CoverageRunner {
    project_root: PathBuf,
    tool: CoverageTool,
}

impl CoverageRunner {
    /// Auto-detect coverage tool from project files.
    pub fn detect(project_root: &Path) -> Option<Self> {
        if project_root.join("Cargo.toml").exists() {
            return Some(Self {
                project_root: project_root.to_path_buf(),
                tool: CoverageTool::CargoTarpaulin,
            });
        }
        if project_root.join("package.json").exists() {
            return Some(Self {
                project_root: project_root.to_path_buf(),
                tool: CoverageTool::JestCoverage,
            });
        }
        if project_root.join("go.mod").exists() {
            return Some(Self {
                project_root: project_root.to_path_buf(),
                tool: CoverageTool::GoCover,
            });
        }
        if project_root.join("pyproject.toml").exists() || project_root.join("pytest.ini").exists() {
            return Some(Self {
                project_root: project_root.to_path_buf(),
                tool: CoverageTool::PytestCov,
            });
        }
        None
    }

    /// Run coverage collection.
    pub async fn run(&self, filter: Option<&str>) -> MaixResult<CoverageReport> {
        let (cmd, args) = match self.tool {
            CoverageTool::CargoTarpaulin => {
                let mut a = vec![
                    "tarpaulin",
                    "--skip-clean",
                    "--color=never",
                    "--out",
                    "Stdout",
                ];
                if let Some(f) = filter {
                    a.push("--");
                    a.push(f);
                }
                ("cargo", a)
            }
            CoverageTool::JestCoverage => {
                let mut a = vec!["test", "--coverage", "--coverageReporters", "text", "--no-color"];
                if let Some(f) = filter {
                    a.push("--testNamePattern");
                    a.push(f);
                }
                ("npx", a)
            }
            CoverageTool::PytestCov => {
                let mut a = vec!["--cov=.", "--cov-report=term", "--no-header", "-q"];
                if let Some(f) = filter {
                    a.push("-k");
                    a.push(f);
                }
                ("pytest", a)
            }
            CoverageTool::GoCover => {
                let mut a = vec!["test", "-coverprofile=coverage.out", "./..."];
                if let Some(f) = filter {
                    a.push("-run");
                    a.push(f);
                }
                ("go", a)
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

        let report = match self.tool {
            CoverageTool::CargoTarpaulin => Self::parse_tarpaulin(&combined),
            CoverageTool::JestCoverage => Self::parse_jest_coverage(&combined),
            CoverageTool::PytestCov => Self::parse_pytest_cov(&combined),
            CoverageTool::GoCover => Self::parse_go_cover(&combined),
        };

        Ok(report)
    }

    fn parse_tarpaulin(output: &str) -> CoverageReport {
        let mut total_lines = 0u32;
        let mut covered_lines = 0u32;
        let mut files = Vec::new();

        for line in output.lines() {
            // "src/lib.rs: 85.71%"
            let line = line.trim();
            if let Some(colon_pos) = line.rfind(':') {
                let path_part = line[..colon_pos].trim();
                let percent_part = line[colon_pos + 1..].trim().trim_end_matches('%');

                if let Ok(percent) = percent_part.parse::<f32>() {
                    if path_part.ends_with(".rs") || path_part.ends_with(".toml") {
                        files.push(FileCoverage {
                            path: path_part.to_string(),
                            total_lines: 0,
                            covered_lines: 0,
                            coverage_percent: percent,
                        });
                    }
                }
            }

            // "Coverage Results: 42/50 lines covered"
            if line.contains("lines covered") || line.contains("coverage") {
                for part in line.split_whitespace() {
                    if part.contains('/') {
                        let nums: Vec<&str> = part.split('/').collect();
                        if nums.len() == 2 {
                            if let (Ok(cov), Ok(total)) =
                                (nums[0].parse::<u32>(), nums[1].parse::<u32>())
                            {
                                covered_lines = cov;
                                total_lines = total;
                            }
                        }
                    }
                }
            }
        }

        let coverage_percent = if total_lines > 0 {
            covered_lines as f32 / total_lines as f32 * 100.0
        } else {
            // Average from file results
            if !files.is_empty() {
                files.iter().map(|f| f.coverage_percent).sum::<f32>() / files.len() as f32
            } else {
                0.0
            }
        };

        CoverageReport {
            tool: CoverageTool::CargoTarpaulin,
            total_lines,
            covered_lines,
            coverage_percent,
            files,
            raw_output: output.to_string(),
        }
    }

    fn parse_jest_coverage(output: &str) -> CoverageReport {
        let total_lines = 0u32;
        let covered_lines = 0u32;
        let mut coverage_percent = 0.0f32;
        let mut files = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            // "All files |   72.5 |   65.3 |   80.1 |   70.2"
            if line.starts_with("All files") {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    if let Ok(pct) = parts[1].parse::<f32>() {
                        coverage_percent = pct;
                    }
                }
            }
            // "src/auth.ts |   85.2 |   70.0 |   90.0 |   82.1 |   42"
            if line.contains('|') && !line.starts_with("All files") && !line.starts_with("File") {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    if let Ok(pct) = parts[1].parse::<f32>() {
                        files.push(FileCoverage {
                            path: parts[0].to_string(),
                            total_lines: 0,
                            covered_lines: 0,
                            coverage_percent: pct,
                        });
                    }
                }
            }
        }

        if total_lines == 0 && !files.is_empty() {
            coverage_percent =
                files.iter().map(|f| f.coverage_percent).sum::<f32>() / files.len() as f32;
        }

        CoverageReport {
            tool: CoverageTool::JestCoverage,
            total_lines,
            covered_lines,
            coverage_percent,
            files,
            raw_output: output.to_string(),
        }
    }

    fn parse_pytest_cov(output: &str) -> CoverageReport {
        let mut coverage_percent = 0.0f32;
        let mut files = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            // "TOTAL                  1234    567    54%"
            if line.starts_with("TOTAL") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(last) = parts.last() {
                    let pct_str = last.trim_end_matches('%');
                    if let Ok(pct) = pct_str.parse::<f32>() {
                        coverage_percent = pct;
                    }
                }
            }
            // "src/auth.py              50     10    80%"
            if !line.starts_with("TOTAL") && !line.starts_with("--") && !line.starts_with("Name")
                && line.contains(".py")
            {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Some(last) = parts.last() {
                        let pct_str = last.trim_end_matches('%');
                        if let Ok(pct) = pct_str.parse::<f32>() {
                            files.push(FileCoverage {
                                path: parts[0].to_string(),
                                total_lines: 0,
                                covered_lines: 0,
                                coverage_percent: pct,
                            });
                        }
                    }
                }
            }
        }

        CoverageReport {
            tool: CoverageTool::PytestCov,
            total_lines: 0,
            covered_lines: 0,
            coverage_percent,
            files,
            raw_output: output.to_string(),
        }
    }

    fn parse_go_cover(output: &str) -> CoverageReport {
        let mut coverage_percent = 0.0f32;

        for line in output.lines() {
            // "coverage: 72.5% of statements"
            if line.contains("coverage:") && line.contains("%") {
                if let Some(pos) = line.find("coverage:") {
                    let rest = &line[pos + 9..];
                    if let Some(pct_pos) = rest.find('%') {
                        if let Ok(pct) = rest[..pct_pos].trim().parse::<f32>() {
                            coverage_percent = pct;
                        }
                    }
                }
            }
        }

        CoverageReport {
            tool: CoverageTool::GoCover,
            total_lines: 0,
            covered_lines: 0,
            coverage_percent,
            files: Vec::new(),
            raw_output: output.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Run tests with coverage and display results.
pub struct CoverageRunTool;

#[async_trait]
impl Tool for CoverageRunTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "coverage_run".into(),
            description: "Run the project's test suite with code coverage. Auto-detects the coverage tool (tarpaulin, jest --coverage, pytest-cov, go test -cover).".into(),
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
        let runner = CoverageRunner::detect(&ctx.working_dir).ok_or_else(|| {
            maix_core::MaixError::Tool(
                "No coverage tool detected. Install cargo-tarpaulin, jest, pytest-cov, or use Go."
                    .into(),
            )
        })?;

        let report = runner.run(filter).await?;
        Ok(report.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tarpaulin() {
        let output = "Coverage Results:\nsrc/lib.rs: 85.71%\nsrc/auth.rs: 62.50%\n42/50 lines covered";
        let report = CoverageRunner::parse_tarpaulin(output);
        assert_eq!(report.covered_lines, 42);
        assert_eq!(report.total_lines, 50);
        assert_eq!(report.files.len(), 2);
    }

    #[test]
    fn test_parse_pytest_cov() {
        let output = "Name             Stmts   Miss  Cover\n------------------------------------\nsrc/auth.py         50     10    80%\nsrc/db.py           30     15    50%\n------------------------------------\nTOTAL               80     25    69%";
        let report = CoverageRunner::parse_pytest_cov(output);
        assert_eq!(report.coverage_percent, 69.0);
        assert_eq!(report.files.len(), 2);
    }

    #[test]
    fn test_parse_go_cover() {
        let output = "ok  \tmyapp/pkg/auth\t0.005s\tcoverage: 72.5% of statements";
        let report = CoverageRunner::parse_go_cover(output);
        assert_eq!(report.coverage_percent, 72.5);
    }

    #[test]
    fn test_report_format() {
        let report = CoverageReport {
            tool: CoverageTool::CargoTarpaulin,
            total_lines: 100,
            covered_lines: 75,
            coverage_percent: 75.0,
            files: vec![
                FileCoverage {
                    path: "src/lib.rs".into(),
                    total_lines: 50,
                    covered_lines: 45,
                    coverage_percent: 90.0,
                },
                FileCoverage {
                    path: "src/auth.rs".into(),
                    total_lines: 50,
                    covered_lines: 30,
                    coverage_percent: 60.0,
                },
            ],
            raw_output: String::new(),
        };
        let formatted = report.format();
        assert!(formatted.contains("75.0%"));
        assert!(formatted.contains("src/lib.rs"));
    }
}
