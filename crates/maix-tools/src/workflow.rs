//! Workflow engine — multi-step task automation with conditions and error handling.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// A workflow definition with ordered steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowStep>,
}

/// A single step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub continue_on_failure: bool,
    #[serde(default)]
    pub retry_count: u32,
    /// Condition expression: if set, step only runs when condition is true.
    /// Simple format: "var_name == value" or "var_name != value"
    #[serde(default)]
    pub condition: Option<String>,
}

/// Status of a workflow or step execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Result of executing a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub status: StepStatus,
    pub output: String,
    pub duration_ms: u64,
    pub attempts: u32,
}

/// A running workflow instance.
#[derive(Debug, Clone)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub status: StepStatus,
    pub current_step: usize,
    pub variables: HashMap<String, String>,
    pub results: Vec<StepResult>,
    pub started_at: Instant,
}

impl WorkflowRun {
    pub fn format_progress(&self) -> String {
        let mut lines = vec![
            format!("Workflow: {}", self.workflow_id),
            format!("Status: {}", self.status),
            format!("Progress: {}/{} steps", self.current_step, self.results.len() + (if self.status == StepStatus::Running { 1 } else { 0 })),
            String::new(),
        ];

        for (i, result) in self.results.iter().enumerate() {
            let icon = match result.status {
                StepStatus::Completed => "✓",
                StepStatus::Failed => "✗",
                StepStatus::Skipped => "○",
                _ => "●",
            };
            lines.push(format!(
                "  {} Step {}: {} ({}ms, {})",
                icon, i + 1, result.step_id, result.duration_ms, result.status
            ));
        }

        if self.status == StepStatus::Running {
            lines.push(format!("  ● Step {}: running...", self.results.len() + 1));
        }

        let elapsed = self.started_at.elapsed().as_secs_f64();
        lines.push(format!("\nElapsed: {:.1}s", elapsed));

        lines.join("\n")
    }
}

/// Workflow engine — defines, loads, and executes workflows.
pub struct WorkflowEngine {
    workflows: HashMap<String, Workflow>,
    runs: Vec<WorkflowRun>,
    max_history: usize,
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowEngine {
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            runs: Vec::new(),
            max_history: 50,
        }
    }

    /// Register a workflow definition.
    pub fn register(&mut self, workflow: Workflow) {
        self.workflows.insert(workflow.id.clone(), workflow);
    }

    /// List all registered workflows.
    pub fn list_workflows(&self) -> Vec<&Workflow> {
        self.workflows.values().collect()
    }

    /// Get a workflow by ID.
    pub fn get_workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.get(id)
    }

    /// Execute a workflow with given variables. Returns the run ID.
    pub async fn execute(
        &mut self,
        workflow_id: &str,
        variables: HashMap<String, String>,
        ctx: &ToolCtx,
    ) -> MaixResult<String> {
        let workflow = self.workflows.get(workflow_id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("workflow '{}' not found", workflow_id)))?
            .clone();

        let run_id = format!("{}_{}", workflow_id, chrono::Utc::now().timestamp_millis());
        let mut run = WorkflowRun {
            id: run_id.clone(),
            workflow_id: workflow_id.to_string(),
            status: StepStatus::Running,
            current_step: 0,
            variables,
            results: Vec::new(),
            started_at: Instant::now(),
        };

        for (i, step) in workflow.steps.iter().enumerate() {
            run.current_step = i;

            // Check condition
            if let Some(ref condition) = step.condition {
                if !evaluate_condition(condition, &run.variables) {
                    run.results.push(StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Skipped,
                        output: format!("Condition not met: {}", condition),
                        duration_ms: 0,
                        attempts: 0,
                    });
                    continue;
                }
            }

            // Execute with retries
            let max_attempts = if step.retry_count > 0 { step.retry_count + 1 } else { 1 };
            let mut last_output = String::new();
            let mut success = false;
            let step_start = Instant::now();

            for attempt in 0..max_attempts {
                // Substitute variables in command
                let command = substitute_variables(&step.command, &run.variables);

                // Execute command via shell
                let shell = if cfg!(windows) { "cmd" } else { "sh" };
                let flag = if cfg!(windows) { "/C" } else { "-c" };

                let output = tokio::process::Command::new(shell)
                    .arg(flag)
                    .arg(&command)
                    .current_dir(&ctx.working_dir)
                    .output()
                    .await;

                match output {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        last_output = format!("{}{}", stdout, if stderr.is_empty() { String::new() } else { format!("\n[stderr]\n{}", stderr) });

                        if output.status.success() {
                            success = true;
                            // Store output in variables for next steps
                            run.variables.insert(format!("{}.output", step.id), stdout.trim().to_string());
                            run.variables.insert(format!("{}.exit_code", step.id), "0".to_string());
                            break;
                        } else {
                            let exit_code = output.status.code().unwrap_or(-1);
                            run.variables.insert(format!("{}.exit_code", step.id), exit_code.to_string());
                            last_output = format!("{}\n[exit code: {}]", last_output, exit_code);
                        }
                    }
                    Err(e) => {
                        last_output = format!("Failed to execute: {}", e);
                    }
                }

                if attempt + 1 < max_attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1) as u64)).await;
                }
            }

            let duration_ms = step_start.elapsed().as_millis() as u64;

            if success {
                run.results.push(StepResult {
                    step_id: step.id.clone(),
                    status: StepStatus::Completed,
                    output: last_output,
                    duration_ms,
                    attempts: max_attempts,
                });
            } else if step.continue_on_failure {
                run.results.push(StepResult {
                    step_id: step.id.clone(),
                    status: StepStatus::Failed,
                    output: last_output,
                    duration_ms,
                    attempts: max_attempts,
                });
            } else {
                let error_output = last_output.clone();
                run.results.push(StepResult {
                    step_id: step.id.clone(),
                    status: StepStatus::Failed,
                    output: last_output,
                    duration_ms,
                    attempts: max_attempts,
                });
                run.status = StepStatus::Failed;
                let step_id = step.id.clone();
                self.runs.push(run);
                if self.runs.len() > self.max_history {
                    self.runs.remove(0);
                }
                return Err(maix_core::MaixError::Tool(format!(
                    "Workflow '{}' failed at step '{}': {}",
                    workflow_id, step_id, error_output
                )));
            }
        }

        run.status = StepStatus::Completed;
        let summary = run.format_progress();
        self.runs.push(run);
        if self.runs.len() > self.max_history {
            self.runs.remove(0);
        }

        Ok(summary)
    }

    /// Get the most recent run.
    pub fn last_run(&self) -> Option<&WorkflowRun> {
        self.runs.last()
    }

    /// Get run history.
    pub fn run_history(&self) -> &[WorkflowRun] {
        &self.runs
    }

    /// Parse a workflow from TOML string.
    pub fn parse_toml(content: &str) -> MaixResult<Workflow> {
        let value: toml::Value = toml::from_str(content)
            .map_err(|e| maix_core::MaixError::Tool(format!("invalid workflow TOML: {}", e)))?;

        let id = value.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        let name = value.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let description = value.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut steps = Vec::new();
        if let Some(steps_arr) = value.get("steps").and_then(|v| v.as_array()) {
            for step_val in steps_arr {
                let step_id = step_val.get("id").and_then(|v| v.as_str()).unwrap_or("step").to_string();
                let step_name = step_val.get("name").and_then(|v| v.as_str()).unwrap_or(&step_id).to_string();
                let command = step_val.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let continue_on_failure = step_val.get("continue_on_failure").and_then(|v| v.as_bool()).unwrap_or(false);
                let retry_count = step_val.get("retry_count").and_then(|v| v.as_integer()).unwrap_or(0) as u32;
                let condition = step_val.get("condition").and_then(|v| v.as_str()).map(|s| s.to_string());

                steps.push(WorkflowStep {
                    id: step_id,
                    name: step_name,
                    command,
                    continue_on_failure,
                    retry_count,
                    condition,
                });
            }
        }

        Ok(Workflow { id, name, description, steps })
    }
}

/// Substitute ${var_name} placeholders in a string with variable values.
fn substitute_variables(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        result = result.replace(&format!("${{{}}}", key), value);
    }
    result
}

/// Evaluate a simple condition expression.
/// Format: "var_name == value" or "var_name != value"
fn evaluate_condition(condition: &str, variables: &HashMap<String, String>) -> bool {
    let parts: Vec<&str> = condition.splitn(3, ['=', '!']).collect();
    if parts.len() < 2 {
        return true; // Invalid condition, default to true
    }

    let var_name = parts[0].trim();
    let expected = parts.last().map(|s| s.trim().trim_matches('"')).unwrap_or("");

    let actual = variables.get(var_name).map(|s| s.as_str()).unwrap_or("");

    if condition.contains("!=") {
        actual != expected
    } else {
        actual == expected
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Execute a workflow by ID with optional variables.
pub struct WorkflowRunTool(pub Arc<Mutex<WorkflowEngine>>);

#[async_trait]
impl Tool for WorkflowRunTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "workflow_run".into(),
            description: "Execute a registered workflow with optional variables. Returns progress summary.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "workflow_id": { "type": "string", "description": "Workflow ID to execute" },
                    "variables": { "type": "object", "description": "Variables to pass to the workflow (key-value pairs)", "additionalProperties": { "type": "string" } }
                },
                "required": ["workflow_id"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let workflow_id = args["workflow_id"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'workflow_id'".into()))?;

        let variables: HashMap<String, String> = args.get("variables")
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();

        let mut engine = self.0.lock().await;
        engine.execute(workflow_id, variables, ctx).await
    }
}

/// List all registered workflows.
pub struct WorkflowListTool(pub Arc<Mutex<WorkflowEngine>>);

#[async_trait]
impl Tool for WorkflowListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "workflow_list".into(),
            description: "List all registered workflows.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let engine = self.0.lock().await;
        let workflows = engine.list_workflows();

        if workflows.is_empty() {
            return Ok("No workflows registered.".to_string());
        }

        let mut lines = vec![format!("{} workflow(s):", workflows.len())];
        for wf in &workflows {
            lines.push(format!("  {} - {} ({} steps)", wf.id, wf.name, wf.steps.len()));
            if !wf.description.is_empty() {
                lines.push(format!("    {}", wf.description));
            }
        }

        Ok(lines.join("\n"))
    }
}

/// Show workflow run history.
pub struct WorkflowHistoryTool(pub Arc<Mutex<WorkflowEngine>>);

#[async_trait]
impl Tool for WorkflowHistoryTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "workflow_history".into(),
            description: "Show workflow execution history.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max entries to show (default: 10)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let limit = args["limit"].as_u64().unwrap_or(10) as usize;
        let engine = self.0.lock().await;
        let history = engine.run_history();

        if history.is_empty() {
            return Ok("No workflow runs in history.".to_string());
        }

        let mut lines = vec![format!("Recent workflow runs ({} total):", history.len())];
        for run in history.iter().rev().take(limit) {
            let elapsed = run.started_at.elapsed().as_secs_f64();
            lines.push(format!(
                "  {} - {} ({}, {:.1}s, {} steps)",
                run.id, run.workflow_id, run.status, elapsed, run.results.len()
            ));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_variables() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        vars.insert("count".to_string(), "42".to_string());

        let result = substitute_variables("hello ${name}, count=${count}", &vars);
        assert_eq!(result, "hello world, count=42");
    }

    #[test]
    fn test_evaluate_condition_eq() {
        let mut vars = HashMap::new();
        vars.insert("env".to_string(), "production".to_string());

        assert!(evaluate_condition("env == production", &vars));
        assert!(!evaluate_condition("env == staging", &vars));
    }

    #[test]
    fn test_evaluate_condition_neq() {
        let mut vars = HashMap::new();
        vars.insert("env".to_string(), "production".to_string());

        assert!(evaluate_condition("env != staging", &vars));
        assert!(!evaluate_condition("env != production", &vars));
    }

    #[test]
    fn test_parse_workflow_toml() {
        let toml = r#"
id = "test"
name = "Test Workflow"
description = "A test workflow"

[[steps]]
id = "step1"
name = "First Step"
command = "echo hello"
continue_on_failure = false
retry_count = 0

[[steps]]
id = "step2"
name = "Second Step"
command = "echo world"
condition = "step1.exit_code == 0"
"#;
        let wf = WorkflowEngine::parse_toml(toml).unwrap();
        assert_eq!(wf.id, "test");
        assert_eq!(wf.steps.len(), 2);
        assert_eq!(wf.steps[0].id, "step1");
        assert!(wf.steps[1].condition.is_some());
    }

    #[test]
    fn test_workflow_engine_register_and_list() {
        let mut engine = WorkflowEngine::new();
        assert!(engine.list_workflows().is_empty());

        engine.register(Workflow {
            id: "test".into(),
            name: "Test".into(),
            description: "desc".into(),
            steps: vec![],
        });

        assert_eq!(engine.list_workflows().len(), 1);
        assert!(engine.get_workflow("test").is_some());
        assert!(engine.get_workflow("nonexistent").is_none());
    }

    #[test]
    fn test_step_status_display() {
        assert_eq!(StepStatus::Completed.to_string(), "completed");
        assert_eq!(StepStatus::Failed.to_string(), "failed");
    }
}
