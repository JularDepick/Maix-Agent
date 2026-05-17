//! Task tracking tools — create, update, list tasks.

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
        }
    }
}

pub type TaskStore = Arc<Mutex<HashMap<String, Task>>>;

pub fn new_task_store() -> TaskStore {
    Arc::new(Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// TaskCreateTool
// ---------------------------------------------------------------------------

pub struct TaskCreateTool {
    store: TaskStore,
}

impl TaskCreateTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_create".into(),
            description: "Create a new task to track progress. Returns task ID.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Brief task title" },
                    "description": { "type": "string", "description": "Detailed task description" }
                },
                "required": ["subject", "description"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let subject = args["subject"].as_str().unwrap_or_default().to_string();
        let description = args["description"].as_str().unwrap_or_default().to_string();

        if subject.is_empty() {
            return Err(maix_core::MaixError::Tool("task_create: subject is required".into()));
        }

        let id = format!("task_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let task = Task {
            id: id.clone(),
            subject: subject.clone(),
            description,
            status: TaskStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            output: String::new(),
        };

        let mut store = self.store.lock().await;
        store.insert(id.clone(), task);

        Ok(format!("Created task {}: {}", id, subject))
    }
}

// ---------------------------------------------------------------------------
// TaskUpdateTool
// ---------------------------------------------------------------------------

pub struct TaskUpdateTool {
    store: TaskStore,
}

impl TaskUpdateTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_update".into(),
            description: "Update a task's status or description.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to update" },
                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"], "description": "New status" },
                    "description": { "type": "string", "description": "New description (optional)" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str().unwrap_or_default().to_string();
        let status_str = args["status"].as_str();
        let description = args["description"].as_str();

        let mut store = self.store.lock().await;
        let task = store.get_mut(&task_id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("task not found: {task_id}")))?;

        if let Some(s) = status_str {
            task.status = match s {
                "pending" => TaskStatus::Pending,
                "in_progress" => TaskStatus::InProgress,
                "completed" => TaskStatus::Completed,
                _ => return Err(maix_core::MaixError::Tool(format!("invalid status: {s}"))),
            };
        }

        if let Some(d) = description {
            task.description = d.to_string();
        }

        task.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        Ok(format!("Updated task {}: status={}", task_id, task.status))
    }
}

// ---------------------------------------------------------------------------
// TaskListTool
// ---------------------------------------------------------------------------

pub struct TaskListTool {
    store: TaskStore,
}

impl TaskListTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_list".into(),
            description: "List all tasks with their status.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"], "description": "Filter by status (optional)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let status_filter = args["status"].as_str();

        let store = self.store.lock().await;
        let tasks: Vec<&Task> = store.values().collect();

        if tasks.is_empty() {
            return Ok("No tasks.".to_string());
        }

        let mut result = String::new();
        for task in &tasks {
            if let Some(filter) = status_filter {
                let filter_status = match filter {
                    "pending" => TaskStatus::Pending,
                    "in_progress" => TaskStatus::InProgress,
                    "completed" => TaskStatus::Completed,
                    _ => continue,
                };
                if task.status != filter_status {
                    continue;
                }
            }
            result.push_str(&format!(
                "[{}] {} ({})\n",
                task.id, task.subject, task.status
            ));
            if !task.description.is_empty() {
                result.push_str(&format!("  {}\n", task.description));
            }
        }

        if result.is_empty() {
            Ok("No matching tasks.".to_string())
        } else {
            Ok(result)
        }
    }
}

// ---------------------------------------------------------------------------
// TaskGetTool
// ---------------------------------------------------------------------------

pub struct TaskGetTool {
    store: TaskStore,
}

impl TaskGetTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_get".into(),
            description: "Get full details of a specific task by ID.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to retrieve" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str().unwrap_or_default();

        let store = self.store.lock().await;
        let task = store.get(task_id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("task not found: {task_id}")))?;

        Ok(format!(
            "Task: {}\nSubject: {}\nStatus: {}\nDescription: {}\nCreated: {}\nUpdated: {}",
            task.id, task.subject, task.status, task.description, task.created_at, task.updated_at
        ))
    }
}

// ---------------------------------------------------------------------------
// TaskStopTool
// ---------------------------------------------------------------------------

pub struct TaskStopTool {
    store: TaskStore,
}

impl TaskStopTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_stop".into(),
            description: "Stop a task — mark it as pending (reset from in_progress).".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to stop" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str().unwrap_or_default();

        let mut store = self.store.lock().await;
        let task = store.get_mut(task_id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("task not found: {task_id}")))?;

        task.status = TaskStatus::Pending;
        task.updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        Ok(format!("Stopped task {}: reset to pending", task_id))
    }
}

// ---------------------------------------------------------------------------
// TaskOutputTool
// ---------------------------------------------------------------------------

pub struct TaskOutputTool {
    store: TaskStore,
}

impl TaskOutputTool {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TaskOutputTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_output".into(),
            description: "Get the output/result of a task.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID" }
                },
                "required": ["task_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str().unwrap_or_default();

        let store = self.store.lock().await;
        let task = store.get(task_id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("task not found: {task_id}")))?;

        if task.output.is_empty() {
            Ok(format!("Task {} has no output yet. Status: {}", task_id, task.status))
        } else {
            Ok(format!("Task {} output:\n{}", task_id, task.output))
        }
    }
}
