//! Agent monitoring — event bus, snapshots, metrics (Phase 3).

pub mod audit;
pub mod event_bus;
pub mod perf;

use chrono::{DateTime, Utc};
use maix_core::AgentState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Event Bus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    StateChanged {
        agent_id: String,
        from: AgentState,
        to: AgentState,
    },
    TaskStarted {
        agent_id: String,
        task_id: String,
    },
    TaskDone {
        agent_id: String,
        task_id: String,
        result: String,
    },
    TaskFailed {
        agent_id: String,
        task_id: String,
        error: String,
    },
    ToolCall {
        agent_id: String,
        tool: String,
        args: serde_json::Value,
        dur_ms: u64,
    },
    ToolResult {
        agent_id: String,
        tool: String,
        result_preview: String,
    },
    TokenUsed {
        agent_id: String,
        prompt_tokens: u64,
        completion_tokens: u64,
        cost_estimate: f64,
    },
    QueueChanged {
        depth: usize,
        action: String,
    },
    MemorySaved {
        key: String,
        size_bytes: usize,
    },
    OrchestratorTick {
        active_agents: usize,
        queue_depth: usize,
        total_tokens: u64,
        total_cost: f64,
    },
}

/// Global event bus — broadcast to multiple subscribers.
pub struct EventBus {
    tx: broadcast::Sender<AgentEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn sender(&self) -> broadcast::Sender<AgentEvent> {
        self.tx.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }

    pub fn emit(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub agent_id: String,
    pub role: String,
    pub state: AgentState,
    pub current_task: Option<String>,
    pub stats: SessionStats,
    pub last_active: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub total_tokens: u64,
    pub total_cost: f64,
    pub total_rounds: u64,
    pub tool_calls: u64,
    pub tool_success: u64,
    pub avg_latency_ms: f64,
    pub llm_calls: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorMetrics {
    pub total_agents: usize,
    pub active_agents: usize,
    pub idle_agents: usize,
    pub queue_depth: usize,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// Central monitor that collects agent state and publishes metrics.
pub struct Monitor {
    #[allow(dead_code)]
    bus: Arc<EventBus>,
    agents: HashMap<String, AgentSnapshot>,
    metrics: OrchestratorMetrics,
    start_time: std::time::Instant,
}

impl Monitor {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            agents: HashMap::new(),
            metrics: OrchestratorMetrics::default(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Update internal state from an event.
    pub fn handle_event(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::StateChanged { agent_id, to, .. } => {
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.state = *to;
                    agent.last_active = Utc::now();
                }
            }
            AgentEvent::TaskDone { agent_id, .. } => {
                self.metrics.tasks_completed += 1;
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.last_active = Utc::now();
                }
            }
            AgentEvent::TaskFailed { agent_id, .. } => {
                self.metrics.tasks_failed += 1;
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.last_active = Utc::now();
                }
            }
            AgentEvent::TokenUsed { agent_id, prompt_tokens, completion_tokens, cost_estimate } => {
                self.metrics.total_tokens += prompt_tokens + completion_tokens;
                self.metrics.total_cost += cost_estimate;
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.stats.total_tokens += prompt_tokens + completion_tokens;
                    agent.stats.total_cost += cost_estimate;
                    agent.stats.llm_calls += 1;
                }
            }
            AgentEvent::QueueChanged { depth, .. } => {
                self.metrics.queue_depth = *depth;
            }
            _ => {}
        }
    }

    pub fn register_agent(&mut self, agent_id: &str, role: &str) {
        self.agents.insert(agent_id.into(), AgentSnapshot {
            agent_id: agent_id.into(),
            role: role.into(),
            state: AgentState::Idle,
            current_task: None,
            stats: SessionStats::default(),
            last_active: Utc::now(),
        });
        self.metrics.total_agents = self.agents.len();
    }

    pub fn deregister_agent(&mut self, agent_id: &str) {
        self.agents.remove(agent_id);
        self.metrics.total_agents = self.agents.len();
    }

    pub fn snapshot(&self) -> (Vec<AgentSnapshot>, OrchestratorMetrics) {
        let mut metrics = self.metrics.clone();
        metrics.uptime_secs = self.start_time.elapsed().as_secs();
        metrics.idle_agents = self.agents.values().filter(|a| a.state == AgentState::Idle).count();
        metrics.active_agents = self.agents.len() - metrics.idle_agents;
        let snapshots: Vec<AgentSnapshot> = self.agents.values().cloned().collect();
        (snapshots, metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_subscribe() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        bus.emit(AgentEvent::QueueChanged { depth: 5, action: "enqueue".into() });

        let event = rx.try_recv().unwrap();
        match event {
            AgentEvent::QueueChanged { depth, .. } => assert_eq!(depth, 5),
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_monitor_tracks_agents() {
        let bus = Arc::new(EventBus::new(64));
        let mut monitor = Monitor::new(bus.clone());

        monitor.register_agent("agent-1", "coder");
        assert_eq!(monitor.snapshot().0.len(), 1);

        let event = AgentEvent::TokenUsed {
            agent_id: "agent-1".into(),
            prompt_tokens: 100,
            completion_tokens: 200,
            cost_estimate: 0.001,
        };
        monitor.handle_event(&event);

        let (snaps, _) = monitor.snapshot();
        assert_eq!(snaps[0].stats.total_tokens, 300);
    }
}
