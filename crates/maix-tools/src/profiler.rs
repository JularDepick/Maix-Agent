//! Performance profiling — run benchmarks, measure execution time, analyze performance.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Result of a benchmark run.
#[derive(Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: u32,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub raw_output: String,
}

impl BenchmarkResult {
    pub fn format(&self) -> String {
        format!(
            "Benchmark: {}\n  Iterations: {}\n  Total: {:.3}s\n  Avg: {:.3}ms\n  Min: {:.3}ms\n  Max: {:.3}ms\n  P50: {:.3}ms\n  P95: {:.3}ms\n  P99: {:.3}ms",
            self.name,
            self.iterations,
            self.total_duration.as_secs_f64(),
            self.avg_duration.as_secs_f64() * 1000.0,
            self.min_duration.as_secs_f64() * 1000.0,
            self.max_duration.as_secs_f64() * 1000.0,
            self.p50.as_secs_f64() * 1000.0,
            self.p95.as_secs_f64() * 1000.0,
            self.p99.as_secs_f64() * 1000.0,
        )
    }
}

/// Result of a command timing.
#[derive(Debug)]
pub struct TimingResult {
    pub command: String,
    pub exit_code: i32,
    pub wall_time: Duration,
    pub stdout_lines: usize,
    pub stderr_lines: usize,
    pub stdout_preview: String,
    pub stderr_preview: String,
}

impl TimingResult {
    pub fn format(&self) -> String {
        let mut lines = vec![
            format!("Command: {}", self.command),
            format!("Exit code: {}", self.exit_code),
            format!("Wall time: {:.3}s", self.wall_time.as_secs_f64()),
            format!("Output: {} lines stdout, {} lines stderr", self.stdout_lines, self.stderr_lines),
        ];

        if !self.stdout_preview.is_empty() {
            lines.push("".to_string());
            lines.push("Stdout preview:".to_string());
            for line in self.stdout_preview.lines().take(10) {
                lines.push(format!("  {}", line));
            }
        }

        if !self.stderr_preview.is_empty() {
            lines.push("".to_string());
            lines.push("Stderr preview:".to_string());
            for line in self.stderr_preview.lines().take(5) {
                lines.push(format!("  {}", line));
            }
        }

        lines.join("\n")
    }
}

/// Performance profiler.
pub struct Profiler;

impl Profiler {
    /// Run a command and measure its execution time.
    pub async fn time_command(command: &str, working_dir: &Path) -> MaixResult<TimingResult> {
        let start = Instant::now();

        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let flag = if cfg!(windows) { "/C" } else { "-c" };

        let output = tokio::process::Command::new(shell)
            .arg(flag)
            .arg(command)
            .current_dir(working_dir)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("profiler: {e}")))?;

        let wall_time = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(TimingResult {
            command: command.to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            wall_time,
            stdout_lines: stdout.lines().count(),
            stderr_lines: stderr.lines().count(),
            stdout_preview: stdout.chars().take(2000).collect(),
            stderr_preview: stderr.chars().take(1000).collect(),
        })
    }

    /// Run a benchmark: execute a command multiple times and compute statistics.
    pub async fn benchmark(
        name: &str,
        command: &str,
        working_dir: &Path,
        iterations: u32,
    ) -> MaixResult<BenchmarkResult> {
        let mut durations = Vec::with_capacity(iterations as usize);

        for _ in 0..iterations {
            let start = Instant::now();

            let shell = if cfg!(windows) { "cmd" } else { "sh" };
            let flag = if cfg!(windows) { "/C" } else { "-c" };

            let output = tokio::process::Command::new(shell)
                .arg(flag)
                .arg(command)
                .current_dir(working_dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| maix_core::MaixError::Tool(format!("benchmark: {e}")))?;

            if !output.status.success() {
                return Err(maix_core::MaixError::Tool(format!(
                    "benchmark iteration failed with exit code {}",
                    output.status.code().unwrap_or(-1)
                )));
            }

            durations.push(start.elapsed());
        }

        durations.sort();

        let total_duration: Duration = durations.iter().sum();
        let avg_duration = total_duration / iterations;
        let min_duration = durations[0];
        let max_duration = durations[durations.len() - 1];

        let p50_idx = (durations.len() as f64 * 0.50) as usize;
        let p95_idx = (durations.len() as f64 * 0.95) as usize;
        let p99_idx = (durations.len() as f64 * 0.99) as usize;

        Ok(BenchmarkResult {
            name: name.to_string(),
            iterations,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            p50: durations[p50_idx.min(durations.len() - 1)],
            p95: durations[p95_idx.min(durations.len() - 1)],
            p99: durations[p99_idx.min(durations.len() - 1)],
            raw_output: String::new(),
        })
    }

    /// Run cargo bench if available.
    pub async fn cargo_bench(project_root: &Path) -> MaixResult<String> {
        let output = tokio::process::Command::new("cargo")
            .args(["bench", "--color=never"])
            .current_dir(project_root)
            .output()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("cargo bench: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        // Take first 50 lines
        let preview: String = combined.lines().take(50).collect::<Vec<_>>().join("\n");
        Ok(preview)
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Time a shell command — measure wall-clock execution time.
pub struct TimeCommandTool;

#[async_trait]
impl Tool for TimeCommandTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "time_command".into(),
            description: "Run a shell command and measure its wall-clock execution time. Returns timing info and output preview.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command to time" }
                },
                "required": ["command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'command'".into()))?;

        let result = Profiler::time_command(command, &ctx.working_dir).await?;
        Ok(result.format())
    }
}

/// Run a benchmark — execute a command multiple times and compute statistics.
pub struct BenchmarkTool;

#[async_trait]
impl Tool for BenchmarkTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "benchmark".into(),
            description: "Run a command multiple times and compute timing statistics (avg, min, max, p50, p95, p99).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command to benchmark" },
                    "iterations": { "type": "integer", "description": "Number of iterations (default: 5)" },
                    "name": { "type": "string", "description": "Benchmark name (default: command)" }
                },
                "required": ["command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'command'".into()))?;
        let iterations = args["iterations"].as_u64().unwrap_or(5) as u32;
        let name = args["name"].as_str().unwrap_or(command);

        let result = Profiler::benchmark(name, command, &ctx.working_dir, iterations).await?;
        Ok(result.format())
    }
}

/// Run cargo bench for the project.
pub struct CargoBenchTool;

#[async_trait]
impl Tool for CargoBenchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cargo_bench".into(),
            description: "Run cargo bench for the Rust project and show benchmark results.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        Profiler::cargo_bench(&ctx.working_dir).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_result_format() {
        let result = TimingResult {
            command: "echo hello".into(),
            exit_code: 0,
            wall_time: Duration::from_millis(50),
            stdout_lines: 1,
            stderr_lines: 0,
            stdout_preview: "hello".into(),
            stderr_preview: String::new(),
        };
        let formatted = result.format();
        assert!(formatted.contains("echo hello"));
        assert!(formatted.contains("Exit code: 0"));
    }

    #[test]
    fn test_benchmark_result_format() {
        let result = BenchmarkResult {
            name: "test".into(),
            iterations: 10,
            total_duration: Duration::from_secs(1),
            avg_duration: Duration::from_millis(100),
            min_duration: Duration::from_millis(80),
            max_duration: Duration::from_millis(150),
            p50: Duration::from_millis(95),
            p95: Duration::from_millis(145),
            p99: Duration::from_millis(150),
            raw_output: String::new(),
        };
        let formatted = result.format();
        assert!(formatted.contains("Iterations: 10"));
        assert!(formatted.contains("Avg:"));
    }
}
