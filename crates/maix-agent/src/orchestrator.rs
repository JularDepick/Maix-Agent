//! Multi-agent orchestration — roles, pool, orchestrator.

use crate::{Agent, AgentConfig, AgentMode};
use maix_memory::MemoryStore;
use maix_provider::LLMProvider;
use maix_task_queue::{AgentId, InsertAt, Task, TaskId, TaskQueue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Agent Role
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRole {
    pub name: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub max_iter: usize,
    pub auto_approve: bool,
}

impl AgentRole {
    pub fn architect() -> Self {
        Self {
            name: "architect".into(),
            system_prompt: "You are a software architect. Analyze requirements, design systems, and decompose complex tasks into subtasks.".into(),
            tools: vec!["fs_read".into(), "web_fetch".into()],
            model: None,
            max_iter: 8,
            auto_approve: true,
        }
    }

    pub fn coder() -> Self {
        Self {
            name: "coder".into(),
            system_prompt: "You are a software engineer. Write, modify, and debug code. Execute shell commands to test your changes.".into(),
            tools: vec!["fs_read".into(), "fs_write".into(), "shell_exec".into()],
            model: None,
            max_iter: 16,
            auto_approve: false,
        }
    }

    pub fn reviewer() -> Self {
        Self {
            name: "reviewer".into(),
            system_prompt: "You are a code reviewer. Review code for correctness, security issues, and adherence to best practices.".into(),
            tools: vec!["fs_read".into(), "web_fetch".into()],
            model: None,
            max_iter: 6,
            auto_approve: true,
        }
    }

    pub fn researcher() -> Self {
        Self {
            name: "researcher".into(),
            system_prompt: "You are a researcher. Search for information, analyze findings, and produce concise summaries.".into(),
            tools: vec!["web_fetch".into(), "fs_read".into()],
            model: None,
            max_iter: 10,
            auto_approve: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent Pool
// ---------------------------------------------------------------------------

pub struct AgentHandle {
    pub agent: Agent,
    pub role: AgentRole,
    pub agent_id: String,
    pub is_busy: bool,
}

pub struct AgentPool {
    agents: HashMap<AgentId, AgentHandle>,
}

impl Default for AgentPool {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPool {
    pub fn new() -> Self {
        Self { agents: HashMap::new() }
    }

    pub fn add(&mut self, handle: AgentHandle) {
        self.agents.insert(handle.agent_id.clone(), handle);
    }

    pub fn remove(&mut self, id: &str) -> Option<AgentHandle> {
        self.agents.remove(id)
    }

    pub fn find_idle(&self, role: Option<&str>) -> Option<&AgentHandle> {
        self.agents.values().find(|h| {
            !h.is_busy && role.is_none_or(|r| h.role.name == r)
        })
    }

    pub fn find_idle_mut(&mut self, role: Option<&str>) -> Option<&mut AgentHandle> {
        self.agents.values_mut().find(|h| {
            !h.is_busy && role.is_none_or(|r| h.role.name == r)
        })
    }

    pub fn list_ids(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    pub fn count(&self) -> usize {
        self.agents.len()
    }
}

// ---------------------------------------------------------------------------
// Orchestration Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationMode {
    Hierarchical,
    Collaborative,
    Debate,
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

pub struct Orchestrator {
    pub pool: AgentPool,
    pub queue: Arc<RwLock<TaskQueue>>,
    pub mode: OrchestrationMode,
    pub memory: Option<Box<dyn MemoryStore>>,
}

impl Orchestrator {
    pub fn new(mode: OrchestrationMode) -> Self {
        Self {
            pool: AgentPool::new(),
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            mode,
            memory: None,
        }
    }

    pub fn with_memory(mut self, memory: Box<dyn MemoryStore>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn add_agent(
        &mut self,
        provider: Arc<dyn LLMProvider>,
        role: AgentRole,
        working_dir: std::path::PathBuf,
    ) -> AgentId {
        let id = uuid::Uuid::new_v4().to_string();
        let mode = if role.auto_approve { AgentMode::Yolo } else { AgentMode::Agent };
        let agent = Agent::new(
            AgentConfig { mode, ..Default::default() },
            provider,
            Arc::new(maix_tools::ToolRegistry::with_builtins()),
            Box::new(
                maix_memory::FileMemoryStore::new(
                    std::env::temp_dir().join("maix-multi-agent-memory"),
                )
                .unwrap(),
            ),
            id.clone(),
            working_dir,
        );
        self.pool.add(AgentHandle { agent, role, agent_id: id.clone(), is_busy: false });
        id
    }

    pub async fn submit(&mut self, description: &str, input: &str, priority: u8, _role: Option<&str>) -> TaskId {
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            input: input.into(),
            priority,
            depends_on: vec![],
            deadline: None,
            retry: maix_task_queue::RetryPolicy::new(3),
            created_at: std::time::Instant::now(),
        };
        let id = task.id.clone();
        {
            let mut q = self.queue.write().await;
            q.enqueue(task);
        }
        id
    }

    pub async fn submit_at(
        &mut self,
        description: &str,
        input: &str,
        priority: u8,
        at: InsertAt,
    ) -> Result<TaskId, String> {
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            input: input.into(),
            priority,
            depends_on: vec![],
            deadline: None,
            retry: maix_task_queue::RetryPolicy::new(3),
            created_at: std::time::Instant::now(),
        };
        let id = task.id.clone();
        {
            let mut q = self.queue.write().await;
            q.insert(task, at)?;
        }
        Ok(id)
    }

    pub async fn tick(&mut self) -> Vec<TaskResult> {
        let mut results = Vec::new();

        loop {
            let task = {
                let mut q = self.queue.write().await;
                q.pop_next()
            };

            let task = match task {
                Some(t) => t,
                None => break,
            };

            let agent = self.pool.find_idle_mut(None);
            match agent {
                Some(handle) => {
                    handle.is_busy = true;
                    let agent_id = handle.agent_id.clone();
                    let task_id = task.task.id.clone();

                    match handle.agent.run(&task.task.input, None, None).await {
                        Ok(output) => {
                            results.push(TaskResult {
                                task_id: task_id.clone(),
                                agent_id: agent_id.clone(),
                                output,
                                success: true,
                            });
                            {
                                let mut q = self.queue.write().await;
                                let _ = q.complete(&task_id, true);
                            }
                        }
                        Err(e) => {
                            results.push(TaskResult {
                                task_id: task_id.clone(),
                                agent_id: agent_id.clone(),
                                output: e.to_string(),
                                success: false,
                            });
                            {
                                let mut q = self.queue.write().await;
                                let _ = q.complete(&task_id, false);
                            }
                        }
                    }

                    handle.is_busy = false;
                }
                None => {
                    {
                        let mut q = self.queue.write().await;
                        let _ = q.insert(task.task, InsertAt::Head);
                    }
                    break;
                }
            }
        }

        results
    }

    pub async fn queue_depth(&self) -> usize {
        self.queue.read().await.len()
    }

    pub fn agent_list(&self) -> Vec<&AgentHandle> {
        self.pool.agents.values().collect()
    }
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub agent_id: String,
    pub output: String,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_roles_have_tools() {
        let arch = AgentRole::architect();
        assert!(arch.tools.contains(&"fs_read".to_string()));

        let coder = AgentRole::coder();
        assert!(coder.tools.contains(&"shell_exec".to_string()));

        let reviewer = AgentRole::reviewer();
        assert_eq!(reviewer.max_iter, 6);
    }

    #[tokio::test]
    async fn test_orchestrator_submit() {
        let mut orch = Orchestrator::new(OrchestrationMode::Hierarchical);
        let id = orch.submit("test task", "hello", 5, None).await;
        assert!(!id.is_empty());
    }
}
