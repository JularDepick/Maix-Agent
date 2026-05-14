//! Multi-agent collaboration — agent profiles, task decomposition, and message bus.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Agent status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Error,
    Offline,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Busy => write!(f, "busy"),
            Self::Error => write!(f, "error"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// An agent profile with specializations and capabilities.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub specialization: Vec<String>,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    pub current_task: Option<String>,
}

/// Task type for decomposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskType {
    Analysis,
    Implementation,
    Review,
    Testing,
    Documentation,
    Research,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Analysis => write!(f, "analysis"),
            Self::Implementation => write!(f, "implementation"),
            Self::Review => write!(f, "review"),
            Self::Testing => write!(f, "testing"),
            Self::Documentation => write!(f, "documentation"),
            Self::Research => write!(f, "research"),
        }
    }
}

/// A collaborative task that can be assigned to agents.
#[derive(Debug, Clone)]
pub struct CollaborativeTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub task_type: TaskType,
    pub assigned_agent: Option<String>,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
}

/// Task execution status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Assigned => write!(f, "assigned"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Message sent between agents.
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub from: String,
    pub to: String,
    pub content: String,
    pub message_type: String,
}

/// Collaboration manager — tracks agents, tasks, and messages.
pub struct CollaborationManager {
    agents: HashMap<String, AgentProfile>,
    tasks: Vec<CollaborativeTask>,
    messages: HashMap<String, VecDeque<AgentMessage>>,
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CollaborationManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            agents: HashMap::new(),
            tasks: Vec::new(),
            messages: HashMap::new(),
        };
        mgr.register_default_agents();
        mgr
    }

    /// Register default specialized agents.
    fn register_default_agents(&mut self) {
        let agents = vec![
            AgentProfile {
                id: "coder".into(),
                name: "Code Writer".into(),
                specialization: vec!["rust".into(), "typescript".into(), "python".into(), "implementation".into()],
                capabilities: vec!["write_code".into(), "refactor".into(), "optimize".into()],
                status: AgentStatus::Idle,
                current_task: None,
            },
            AgentProfile {
                id: "reviewer".into(),
                name: "Code Reviewer".into(),
                specialization: vec!["code_review".into(), "security".into(), "performance".into()],
                capabilities: vec!["review_code".into(), "find_bugs".into(), "suggest_improvements".into()],
                status: AgentStatus::Idle,
                current_task: None,
            },
            AgentProfile {
                id: "tester".into(),
                name: "Test Writer".into(),
                specialization: vec!["testing".into(), "coverage".into(), "integration".into()],
                capabilities: vec!["write_tests".into(), "run_tests".into(), "analyze_coverage".into()],
                status: AgentStatus::Idle,
                current_task: None,
            },
            AgentProfile {
                id: "researcher".into(),
                name: "Researcher".into(),
                specialization: vec!["documentation".into(), "api_reference".into(), "best_practices".into()],
                capabilities: vec!["search_docs".into(), "analyze_code".into(), "write_docs".into()],
                status: AgentStatus::Idle,
                current_task: None,
            },
        ];

        for agent in agents {
            self.agents.insert(agent.id.clone(), agent);
        }
    }

    /// List all agents.
    pub fn list_agents(&self) -> Vec<&AgentProfile> {
        self.agents.values().collect()
    }

    /// Get an agent by ID.
    pub fn get_agent(&self, id: &str) -> Option<&AgentProfile> {
        self.agents.get(id)
    }

    /// Decompose a complex task into sub-tasks.
    pub fn decompose_task(&self, task_id: &str, name: &str, description: &str, task_type: TaskType) -> Vec<CollaborativeTask> {
        match task_type {
            TaskType::Implementation => {
                vec![
                    CollaborativeTask {
                        id: format!("{}-design", task_id),
                        name: format!("{}: Design", name),
                        description: format!("Design the implementation for: {}", description),
                        task_type: TaskType::Analysis,
                        assigned_agent: None,
                        dependencies: vec![],
                        status: TaskStatus::Pending,
                    },
                    CollaborativeTask {
                        id: format!("{}-implement", task_id),
                        name: format!("{}: Implement", name),
                        description: format!("Implement: {}", description),
                        task_type: TaskType::Implementation,
                        assigned_agent: None,
                        dependencies: vec![format!("{}-design", task_id)],
                        status: TaskStatus::Pending,
                    },
                    CollaborativeTask {
                        id: format!("{}-test", task_id),
                        name: format!("{}: Test", name),
                        description: format!("Write tests for: {}", description),
                        task_type: TaskType::Testing,
                        assigned_agent: None,
                        dependencies: vec![format!("{}-implement", task_id)],
                        status: TaskStatus::Pending,
                    },
                    CollaborativeTask {
                        id: format!("{}-review", task_id),
                        name: format!("{}: Review", name),
                        description: format!("Review: {}", description),
                        task_type: TaskType::Review,
                        assigned_agent: None,
                        dependencies: vec![format!("{}-test", task_id)],
                        status: TaskStatus::Pending,
                    },
                ]
            }
            _ => {
                vec![CollaborativeTask {
                    id: task_id.to_string(),
                    name: name.to_string(),
                    description: description.to_string(),
                    task_type,
                    assigned_agent: None,
                    dependencies: vec![],
                    status: TaskStatus::Pending,
                }]
            }
        }
    }

    /// Add tasks to the manager.
    pub fn add_tasks(&mut self, tasks: Vec<CollaborativeTask>) {
        self.tasks.extend(tasks);
    }

    /// Assign pending tasks to idle agents based on specialization.
    pub fn auto_assign(&mut self) -> Vec<(String, String)> {
        let mut assignments = Vec::new();

        // Find pending tasks with all dependencies completed
        let ready_task_ids: Vec<String> = self.tasks.iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| {
                t.dependencies.iter().all(|dep_id| {
                    self.tasks.iter().any(|dt| dt.id == *dep_id && dt.status == TaskStatus::Completed)
                })
            })
            .map(|t| t.id.clone())
            .collect();

        for task_id in ready_task_ids {
            let task_type = self.tasks.iter().find(|t| t.id == task_id).unwrap().task_type.clone();

            // Find best idle agent
            let best_agent = self.agents.values()
                .filter(|a| a.status == AgentStatus::Idle)
                .max_by_key(|agent| {
                    let type_match = match task_type {
                        TaskType::Implementation => agent.specialization.iter().any(|s| s.contains("implementation") || s.contains("rust") || s.contains("typescript")),
                        TaskType::Review => agent.specialization.iter().any(|s| s.contains("review") || s.contains("security")),
                        TaskType::Testing => agent.specialization.iter().any(|s| s.contains("test")),
                        TaskType::Documentation => agent.specialization.iter().any(|s| s.contains("doc")),
                        TaskType::Analysis => agent.specialization.iter().any(|s| s.contains("analyze") || s.contains("research")),
                        TaskType::Research => agent.specialization.iter().any(|s| s.contains("research") || s.contains("doc")),
                    };
                    if type_match { 1 } else { 0 }
                })
                .map(|a| a.id.clone());

            if let Some(agent_id) = best_agent {
                // Assign task
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.assigned_agent = Some(agent_id.clone());
                    task.status = TaskStatus::Assigned;
                }
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.status = AgentStatus::Busy;
                    agent.current_task = Some(task_id.clone());
                }
                assignments.push((task_id, agent_id));
            }
        }

        assignments
    }

    /// Complete a task and free the agent.
    pub fn complete_task(&mut self, task_id: &str) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Completed;
            if let Some(ref agent_id) = task.assigned_agent.clone() {
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.status = AgentStatus::Idle;
                    agent.current_task = None;
                }
            }
        }
    }

    /// Send a message between agents.
    pub fn send_message(&mut self, from: &str, to: &str, content: &str, msg_type: &str) {
        let msg = AgentMessage {
            from: from.to_string(),
            to: to.to_string(),
            content: content.to_string(),
            message_type: msg_type.to_string(),
        };
        self.messages
            .entry(to.to_string())
            .or_default()
            .push_back(msg);
    }

    /// Receive messages for an agent.
    pub fn receive_messages(&mut self, agent_id: &str) -> Vec<AgentMessage> {
        self.messages
            .get_mut(agent_id)
            .map(|queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    /// Get task status summary.
    pub fn task_summary(&self) -> String {
        let total = self.tasks.len();
        let pending = self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let assigned = self.tasks.iter().filter(|t| t.status == TaskStatus::Assigned).count();
        let in_progress = self.tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let completed = self.tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
        let failed = self.tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();

        let mut lines = vec![
            format!("Tasks: {} total", total),
            format!("  Pending: {} | Assigned: {} | In Progress: {} | Completed: {} | Failed: {}",
                pending, assigned, in_progress, completed, failed),
            String::new(),
        ];

        for task in &self.tasks {
            let agent = task.assigned_agent.as_deref().unwrap_or("-");
            lines.push(format!("  {} [{}] {} - {}", task.id, task.status, task.name, agent));
        }

        lines.join("\n")
    }

    /// Get agent status summary.
    pub fn agent_summary(&self) -> String {
        let mut lines = vec![format!("Agents ({}):", self.agents.len())];
        for agent in self.agents.values() {
            let task = agent.current_task.as_deref().unwrap_or("-");
            lines.push(format!("  {} [{}] {} - task: {}", agent.id, agent.status, agent.name, task));
        }
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// List all agents and their status.
pub struct AgentListTool(pub Arc<Mutex<CollaborationManager>>);

#[async_trait]
impl Tool for AgentListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "agent_list".into(),
            description: "List all available agents and their current status.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mgr = self.0.lock().await;
        Ok(mgr.agent_summary())
    }
}

/// Decompose a complex task into sub-tasks and auto-assign to agents.
pub struct TaskDecomposeTool(pub Arc<Mutex<CollaborationManager>>);

#[async_trait]
impl Tool for TaskDecomposeTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "task_decompose".into(),
            description: "Decompose a complex task into sub-tasks (design, implement, test, review) and assign to specialized agents.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Unique task ID" },
                    "name": { "type": "string", "description": "Task name" },
                    "description": { "type": "string", "description": "Task description" },
                    "task_type": { "type": "string", "description": "Type: implementation, analysis, review, testing, documentation, research" }
                },
                "required": ["task_id", "name", "description"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let task_id = args["task_id"].as_str().unwrap_or("task");
        let name = args["name"].as_str().unwrap_or("Task");
        let description = args["description"].as_str().unwrap_or("");
        let task_type = match args["task_type"].as_str().unwrap_or("implementation") {
            "implementation" => TaskType::Implementation,
            "analysis" => TaskType::Analysis,
            "review" => TaskType::Review,
            "testing" => TaskType::Testing,
            "documentation" => TaskType::Documentation,
            "research" => TaskType::Research,
            _ => TaskType::Implementation,
        };

        let mut mgr = self.0.lock().await;
        let tasks = mgr.decompose_task(task_id, name, description, task_type);
        let count = tasks.len();
        mgr.add_tasks(tasks);

        // Auto-assign
        let assignments = mgr.auto_assign();

        let mut lines = vec![format!("Decomposed into {} sub-tasks:", count)];
        for (tid, agent_id) in &assignments {
            lines.push(format!("  {} -> agent {}", tid, agent_id));
        }

        if assignments.is_empty() {
            lines.push("  No idle agents available for assignment.".to_string());
        }

        Ok(lines.join("\n"))
    }
}

/// Show collaboration task status.
pub struct CollabStatusTool(pub Arc<Mutex<CollaborationManager>>);

#[async_trait]
impl Tool for CollabStatusTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "collab_status".into(),
            description: "Show collaboration status: tasks, agents, and progress.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mgr = self.0.lock().await;
        let mut result = mgr.task_summary();
        result.push_str("\n\n");
        result.push_str(&mgr.agent_summary());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_list() {
        let mgr = CollaborationManager::new();
        assert_eq!(mgr.list_agents().len(), 4);
        assert!(mgr.get_agent("coder").is_some());
        assert!(mgr.get_agent("reviewer").is_some());
    }

    #[test]
    fn test_decompose_implementation() {
        let mgr = CollaborationManager::new();
        let tasks = mgr.decompose_task("t1", "Add Auth", "Implement user authentication", TaskType::Implementation);
        assert_eq!(tasks.len(), 4); // design, implement, test, review
        assert_eq!(tasks[0].task_type, TaskType::Analysis);
        assert_eq!(tasks[1].task_type, TaskType::Implementation);
        assert_eq!(tasks[2].task_type, TaskType::Testing);
        assert_eq!(tasks[3].task_type, TaskType::Review);
    }

    #[test]
    fn test_decompose_simple() {
        let mgr = CollaborationManager::new();
        let tasks = mgr.decompose_task("t1", "Research", "Look up API docs", TaskType::Research);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_auto_assign() {
        let mut mgr = CollaborationManager::new();
        let tasks = mgr.decompose_task("t1", "Add Auth", "Implement auth", TaskType::Analysis);
        mgr.add_tasks(tasks);

        let assignments = mgr.auto_assign();
        // First task (design/analysis) should be assigned
        assert!(!assignments.is_empty());
    }

    #[test]
    fn test_complete_task() {
        let mut mgr = CollaborationManager::new();
        let tasks = mgr.decompose_task("t1", "Task", "desc", TaskType::Analysis);
        mgr.add_tasks(tasks);
        mgr.auto_assign();

        mgr.complete_task("t1");
        assert_eq!(mgr.tasks[0].status, TaskStatus::Completed);
        // Agent should be freed
        let agent = mgr.agents.values().find(|a| a.current_task.is_none());
        assert!(agent.is_some());
    }

    #[test]
    fn test_message_bus() {
        let mut mgr = CollaborationManager::new();
        mgr.send_message("coder", "reviewer", "Please review auth.rs", "task_request");

        let msgs = mgr.receive_messages("reviewer");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "Please review auth.rs");

        // Should be empty after receive
        let msgs = mgr.receive_messages("reviewer");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_task_summary() {
        let mut mgr = CollaborationManager::new();
        let tasks = mgr.decompose_task("t1", "Test", "desc", TaskType::Implementation);
        mgr.add_tasks(tasks);

        let summary = mgr.task_summary();
        assert!(summary.contains("Tasks: 4 total"));
        assert!(summary.contains("Pending: 4"));
    }
}
