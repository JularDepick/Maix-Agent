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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolCtx {
        ToolCtx {
            session_id: "test".into(),
            working_dir: ".".into(),
            ask_user_tx: None,
        }
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Pending.to_string(), "pending");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
    }

    #[tokio::test]
    async fn test_task_create_and_get() {
        let store = new_task_store();
        let tool = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();

        let result = tool.execute(&ctx, serde_json::json!({
            "subject": "Test task",
            "description": "A test description"
        })).await.unwrap();

        assert!(result.starts_with("Created task task_"));

        let store = store.lock().await;
        assert_eq!(store.len(), 1);
        let task = store.values().next().unwrap();
        assert_eq!(task.subject, "Test task");
        assert_eq!(task.description, "A test description");
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_task_create_empty_subject() {
        let store = new_task_store();
        let tool = TaskCreateTool::new(store);
        let ctx = test_ctx();

        let result = tool.execute(&ctx, serde_json::json!({
            "subject": "",
            "description": "desc"
        })).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_update_status() {
        let store = new_task_store();
        let create = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();
        create.execute(&ctx, serde_json::json!({
            "subject": "Task",
            "description": "desc"
        })).await.unwrap();

        let task_id = store.lock().await.keys().next().cloned().unwrap();

        let update = TaskUpdateTool::new(store.clone());
        update.execute(&ctx, serde_json::json!({
            "task_id": task_id,
            "status": "in_progress"
        })).await.unwrap();

        let store = store.lock().await;
        assert_eq!(store[&task_id].status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_task_update_not_found() {
        let store = new_task_store();
        let update = TaskUpdateTool::new(store);
        let ctx = test_ctx();

        let result = update.execute(&ctx, serde_json::json!({
            "task_id": "nonexistent"
        })).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_list_empty() {
        let store = new_task_store();
        let tool = TaskListTool::new(store);
        let ctx = test_ctx();

        let result = tool.execute(&ctx, serde_json::json!({})).await.unwrap();
        assert_eq!(result, "No tasks.");
    }

    #[tokio::test]
    async fn test_task_list_with_filter() {
        let store = new_task_store();
        let create = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();
        create.execute(&ctx, serde_json::json!({"subject": "A", "description": ""})).await.unwrap();
        create.execute(&ctx, serde_json::json!({"subject": "B", "description": ""})).await.unwrap();

        let task_id = store.lock().await.keys().next().cloned().unwrap();
        let update = TaskUpdateTool::new(store.clone());
        update.execute(&ctx, serde_json::json!({"task_id": task_id, "status": "completed"})).await.unwrap();

        let list = TaskListTool::new(store.clone());
        let completed = list.execute(&ctx, serde_json::json!({"status": "completed"})).await.unwrap();
        assert!(completed.contains("A") || completed.contains("B"));
        let pending = list.execute(&ctx, serde_json::json!({"status": "pending"})).await.unwrap();
        assert!(pending.contains("A") || pending.contains("B"));
    }

    #[tokio::test]
    async fn test_task_get() {
        let store = new_task_store();
        let create = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();
        create.execute(&ctx, serde_json::json!({"subject": "Get me", "description": "details"})).await.unwrap();

        let task_id = store.lock().await.keys().next().cloned().unwrap();
        let get = TaskGetTool::new(store);
        let result = get.execute(&ctx, serde_json::json!({"task_id": task_id})).await.unwrap();
        assert!(result.contains("Get me"));
        assert!(result.contains("details"));
    }

    #[tokio::test]
    async fn test_task_get_not_found() {
        let store = new_task_store();
        let get = TaskGetTool::new(store);
        let ctx = test_ctx();
        let result = get.execute(&ctx, serde_json::json!({"task_id": "nope"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_stop() {
        let store = new_task_store();
        let create = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();
        create.execute(&ctx, serde_json::json!({"subject": "S", "description": ""})).await.unwrap();

        let task_id = store.lock().await.keys().next().cloned().unwrap();
        let update = TaskUpdateTool::new(store.clone());
        update.execute(&ctx, serde_json::json!({"task_id": task_id, "status": "in_progress"})).await.unwrap();

        let stop = TaskStopTool::new(store.clone());
        stop.execute(&ctx, serde_json::json!({"task_id": task_id})).await.unwrap();

        assert_eq!(store.lock().await[&task_id].status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_task_output_empty() {
        let store = new_task_store();
        let create = TaskCreateTool::new(store.clone());
        let ctx = test_ctx();
        create.execute(&ctx, serde_json::json!({"subject": "T", "description": ""})).await.unwrap();

        let task_id = store.lock().await.keys().next().cloned().unwrap();
        let output = TaskOutputTool::new(store);
        let result = output.execute(&ctx, serde_json::json!({"task_id": task_id})).await.unwrap();
        assert!(result.contains("no output yet"));
    }
}
