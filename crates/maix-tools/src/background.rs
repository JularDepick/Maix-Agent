//! Background task management — spawn, monitor, cancel long-running processes.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

/// Status of a background task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A managed background task.
#[derive(Debug)]
pub struct BackgroundTask {
    pub id: String,
    pub name: String,
    pub command: String,
    pub status: TaskStatus,
    pub pid: Option<u32>,
    pub started_at: std::time::Instant,
    pub output_buffer: Arc<Mutex<Vec<String>>>,
    pub exit_code: Option<i32>,
    child: Option<tokio::process::Child>,
}

/// Manages background tasks.
pub struct BackgroundTaskManager {
    tasks: HashMap<String, BackgroundTask>,
    next_id: u32,
    max_concurrent: usize,
}

impl BackgroundTaskManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: HashMap::new(),
            next_id: 1,
            max_concurrent,
        }
    }

    /// Spawn a new background task.
    pub async fn spawn(&mut self, name: &str, command: &str) -> MaixResult<String> {
        let running = self.tasks.values().filter(|t| t.status == TaskStatus::Running).count();
        if running >= self.max_concurrent {
            return Err(maix_core::MaixError::Tool(format!(
                "Max concurrent tasks ({}) reached. Cancel a task first.", self.max_concurrent
            )));
        }

        let id = format!("bg-{}", self.next_id);
        self.next_id += 1;

        let output_buffer = Arc::new(Mutex::new(Vec::new()));

        #[cfg(target_os = "windows")]
        let mut child = tokio::process::Command::new("cmd")
            .arg("/C")
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| maix_core::MaixError::Tool(format!("Failed to spawn: {e}")))?;

        #[cfg(not(target_os = "windows"))]
        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| maix_core::MaixError::Tool(format!("Failed to spawn: {e}")))?;

        let pid = child.id();

        // Capture stdout
        if let Some(stdout) = child.stdout.take() {
            let buffer = output_buffer.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut buf = buffer.lock().await;
                    buf.push(format!("[stdout] {}", line));
                    if buf.len() > 1000 {
                        buf.remove(0);
                    }
                }
            });
        }

        // Capture stderr
        if let Some(stderr) = child.stderr.take() {
            let buffer = output_buffer.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut buf = buffer.lock().await;
                    buf.push(format!("[stderr] {}", line));
                    if buf.len() > 1000 {
                        buf.remove(0);
                    }
                }
            });
        }

        let task = BackgroundTask {
            id: id.clone(),
            name: name.to_string(),
            command: command.to_string(),
            status: TaskStatus::Running,
            pid,
            started_at: std::time::Instant::now(),
            output_buffer,
            exit_code: None,
            child: Some(child),
        };

        self.tasks.insert(id.clone(), task);
        Ok(id)
    }

    /// Poll task status (check if process exited).
    pub fn poll(&mut self) {
        for task in self.tasks.values_mut() {
            if task.status != TaskStatus::Running {
                continue;
            }
            if let Some(ref mut child) = task.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        task.exit_code = status.code();
                        task.status = if status.success() {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                    }
                    Ok(None) => {} // still running
                    Err(_) => {
                        task.status = TaskStatus::Failed;
                    }
                }
            }
        }
    }

    /// Get recent output lines from a task.
    pub async fn get_output(&self, task_id: &str, _lines: usize) -> Option<String> {
        self.tasks.get(task_id).map(|_task| {
            format!("(task {task_id} output)")
        })
    }

    /// Get output async.
    pub async fn get_output_async(&self, task_id: &str, max_lines: usize) -> Option<String> {
        if let Some(task) = self.tasks.get(task_id) {
            let buffer = task.output_buffer.lock().await;
            let start = if buffer.len() > max_lines {
                buffer.len() - max_lines
            } else {
                0
            };
            Some(buffer[start..].join("\n"))
        } else {
            None
        }
    }

    /// Get task status.
    pub fn get_status(&self, task_id: &str) -> Option<&TaskStatus> {
        self.tasks.get(task_id).map(|t| &t.status)
    }

    /// Cancel a running task.
    pub async fn cancel(&mut self, task_id: &str) -> MaixResult<()> {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if task.status == TaskStatus::Running {
                if let Some(ref mut child) = task.child {
                    child.kill().await
                        .map_err(|e| maix_core::MaixError::Tool(format!("Failed to kill: {e}")))?;
                    task.status = TaskStatus::Cancelled;
                }
            }
            Ok(())
        } else {
            Err(maix_core::MaixError::Tool(format!("Task {task_id} not found")))
        }
    }

    /// List all tasks.
    pub fn list(&self) -> Vec<(&str, &str, &TaskStatus, Option<u32>)> {
        self.tasks.values()
            .map(|t| (t.id.as_str(), t.name.as_str(), &t.status, t.pid))
            .collect()
    }

    /// Cleanup completed/cancelled/failed tasks.
    pub fn cleanup(&mut self) {
        self.tasks.retain(|_, t| t.status == TaskStatus::Running);
    }

    /// Get task info summary.
    pub fn summary(&self) -> String {
        let running = self.tasks.values().filter(|t| t.status == TaskStatus::Running).count();
        let completed = self.tasks.values().filter(|t| t.status == TaskStatus::Completed).count();
        let failed = self.tasks.values().filter(|t| t.status == TaskStatus::Failed).count();
        let cancelled = self.tasks.values().filter(|t| t.status == TaskStatus::Cancelled).count();
        format!("Running: {running} | Completed: {completed} | Failed: {failed} | Cancelled: {cancelled}")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Spawn a background task.
pub struct BgSpawnTool(pub Arc<Mutex<BackgroundTaskManager>>);

#[async_trait]
impl Tool for BgSpawnTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "bg_spawn".into(),
            description: "Start a background task that continues running after the response. Use for long-running processes like dev servers, watchers, etc.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "A short descriptive name for the task" },
                    "command": { "type": "string", "description": "The shell command to run in the background" }
                },
                "required": ["name", "command"]
            }),
            risk_level: RiskLevel::Shell,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let name = args["name"].as_str().unwrap_or("unnamed");
        let command = args["command"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'command'".into()))?;

        let mut mgr = self.0.lock().await;
        let id = mgr.spawn(name, command).await?;
        Ok(format!("Background task started: {id} ({name})"))
    }
}

/// List background tasks.
pub struct BgListTool(pub Arc<Mutex<BackgroundTaskManager>>);

#[async_trait]
impl Tool for BgListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "bg_list".into(),
            description: "List all background tasks with their status.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mut mgr = self.0.lock().await;
        mgr.poll();

        let tasks = mgr.list();
        if tasks.is_empty() {
            return Ok("No background tasks.".into());
        }

        let mut lines = vec!["Background tasks:".to_string()];
        for (id, name, status, pid) in &tasks {
            let status_str = match status {
                TaskStatus::Running => "● Running",
                TaskStatus::Completed => "✓ Completed",
                TaskStatus::Failed => "✗ Failed",
                TaskStatus::Cancelled => "○ Cancelled",
            };
            let pid_str = pid.map(|p| format!(" (pid: {p})")).unwrap_or_default();
            lines.push(format!("  {id}: {name} — {status_str}{pid_str}"));
        }
        Ok(lines.join("\n"))
    }
}

/// Get output from a background task.
pub struct BgLogTool(pub Arc<Mutex<BackgroundTaskManager>>);

#[async_trait]
impl Tool for BgLogTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "bg_log".into(),
            description: "Get the recent output from a background task.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "The task ID (e.g. 'bg-1')" },
                    "lines": { "type": "integer", "description": "Number of recent lines to return (default: 50)" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'task_id'".into()))?;
        let lines = args["lines"].as_u64().unwrap_or(50) as usize;

        let mgr = self.0.lock().await;
        match mgr.get_output_async(task_id, lines).await {
            Some(output) => {
                if output.is_empty() {
                    Ok(format!("Task {task_id}: no output yet."))
                } else {
                    Ok(format!("Task {task_id} output (last {lines} lines):\n{output}"))
                }
            }
            None => Err(maix_core::MaixError::Tool(format!("Task {task_id} not found"))),
        }
    }
}

/// Cancel a background task.
pub struct BgCancelTool(pub Arc<Mutex<BackgroundTaskManager>>);

#[async_trait]
impl Tool for BgCancelTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "bg_cancel".into(),
            description: "Cancel a running background task.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "The task ID to cancel" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'task_id'".into()))?;

        let mut mgr = self.0.lock().await;
        mgr.cancel(task_id).await?;
        Ok(format!("Task {task_id} cancelled."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_and_list() {
        let mgr = Arc::new(Mutex::new(BackgroundTaskManager::new(5)));

        // Spawn a quick task
        {
            let mut m = mgr.lock().await;
            #[cfg(target_os = "windows")]
            let id = m.spawn("echo-test", "echo hello").await.unwrap();
            #[cfg(not(target_os = "windows"))]
            let id = m.spawn("echo-test", "echo hello").await.unwrap();
            assert!(id.starts_with("bg-"));
        }

        // Wait a bit for it to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let mut m = mgr.lock().await;
        m.poll();
        let tasks = m.list();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].1, "echo-test");
    }

    #[tokio::test]
    async fn test_cancel() {
        let mgr = Arc::new(Mutex::new(BackgroundTaskManager::new(5)));

        let id = {
            let mut m = mgr.lock().await;
            #[cfg(target_os = "windows")]
            { m.spawn("sleep-test", "ping -n 10 127.0.0.1").await.unwrap() }
            #[cfg(not(target_os = "windows"))]
            { m.spawn("sleep-test", "sleep 10").await.unwrap() }
        };

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut m = mgr.lock().await;
        m.cancel(&id).await.unwrap();
        let status = m.get_status(&id).unwrap();
        assert_eq!(*status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_max_concurrent() {
        let mgr = Arc::new(Mutex::new(BackgroundTaskManager::new(2)));

        let mut m = mgr.lock().await;
        #[cfg(target_os = "windows")]
        {
            m.spawn("t1", "ping -n 10 127.0.0.1").await.unwrap();
            m.spawn("t2", "ping -n 10 127.0.0.1").await.unwrap();
            let result = m.spawn("t3", "ping -n 10 127.0.0.1").await;
            assert!(result.is_err());
        }
        #[cfg(not(target_os = "windows"))]
        {
            m.spawn("t1", "sleep 10").await.unwrap();
            m.spawn("t2", "sleep 10").await.unwrap();
            let result = m.spawn("t3", "sleep 10").await;
            assert!(result.is_err());
        }
    }
}
