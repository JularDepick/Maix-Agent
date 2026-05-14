//! Planning mode — multi-step plan generation and execution tracking.

use serde::{Deserialize, Serialize};

/// Step in a plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub tool_hint: Option<String>,
    pub status: StepStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    Draft,
    Approved,
    Executing,
    Completed,
    Aborted,
}

/// A multi-step plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
}

impl Plan {
    pub fn new(goal: &str) -> Self {
        Self {
            goal: goal.to_string(),
            steps: Vec::new(),
            status: PlanStatus::Draft,
        }
    }

    pub fn add_step(&mut self, description: &str, tool_hint: Option<&str>) {
        let id = self.steps.len() + 1;
        self.steps.push(PlanStep {
            id,
            description: description.to_string(),
            tool_hint: tool_hint.map(|s| s.to_string()),
            status: StepStatus::Pending,
            result: None,
        });
    }

    pub fn approve(&mut self) {
        if self.status == PlanStatus::Draft {
            self.status = PlanStatus::Approved;
        }
    }

    pub fn start_execution(&mut self) {
        if self.status == PlanStatus::Approved {
            self.status = PlanStatus::Executing;
        }
    }

    pub fn abort(&mut self) {
        self.status = PlanStatus::Aborted;
    }

    pub fn pending_steps(&self) -> Vec<&PlanStep> {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Pending)
            .collect()
    }

    pub fn next_pending(&mut self) -> Option<&mut PlanStep> {
        self.steps
            .iter_mut()
            .find(|s| s.status == StepStatus::Pending)
    }

    pub fn complete_step(&mut self, id: usize, result: &str) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == id) {
            step.status = StepStatus::Done;
            step.result = Some(result.to_string());
        }
        self.check_completion();
    }

    pub fn fail_step(&mut self, id: usize, error: &str) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == id) {
            step.status = StepStatus::Failed;
            step.result = Some(error.to_string());
        }
    }

    pub fn skip_step(&mut self, id: usize) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == id) {
            step.status = StepStatus::Skipped;
        }
        self.check_completion();
    }

    fn check_completion(&mut self) {
        let all_done = self
            .steps
            .iter()
            .all(|s| matches!(s.status, StepStatus::Done | StepStatus::Skipped));
        if all_done {
            self.status = PlanStatus::Completed;
        }
    }

    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Done | StepStatus::Skipped))
            .count();
        (done, self.steps.len())
    }

    pub fn format_display(&self) -> String {
        let mut output = format!("Goal: {}\nStatus: {:?}\n\n", self.goal, self.status);
        for step in &self.steps {
            let icon = match step.status {
                StepStatus::Pending => "[ ]",
                StepStatus::InProgress => "[~]",
                StepStatus::Done => "[x]",
                StepStatus::Failed => "[!]",
                StepStatus::Skipped => "[-]",
            };
            let tool = step
                .tool_hint
                .as_deref()
                .map(|t| format!(" ({})", t))
                .unwrap_or_default();
            output.push_str(&format!("{} {}{}\n", icon, step.description, tool));
        }
        output
    }
}

/// Planner for generating plans from task descriptions.
pub struct Planner {
    max_steps: usize,
}

impl Planner {
    pub fn new(max_steps: usize) -> Self {
        Self { max_steps }
    }

    pub fn create_plan(&self, goal: &str, steps: Vec<(&str, Option<&str>)>) -> Plan {
        let mut plan = Plan::new(goal);
        for (desc, tool) in steps.into_iter().take(self.max_steps) {
            plan.add_step(desc, tool);
        }
        plan
    }

    pub fn max_steps(&self) -> usize {
        self.max_steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_new() {
        let plan = Plan::new("Fix bug");
        assert_eq!(plan.goal, "Fix bug");
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn test_plan_add_step() {
        let mut plan = Plan::new("task");
        plan.add_step("Step 1", Some("grep"));
        plan.add_step("Step 2", None);
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].id, 1);
        assert_eq!(plan.steps[1].id, 2);
    }

    #[test]
    fn test_plan_approve() {
        let mut plan = Plan::new("task");
        plan.approve();
        assert_eq!(plan.status, PlanStatus::Approved);
    }

    #[test]
    fn test_plan_start_execution() {
        let mut plan = Plan::new("task");
        plan.approve();
        plan.start_execution();
        assert_eq!(plan.status, PlanStatus::Executing);
    }

    #[test]
    fn test_plan_complete_step() {
        let mut plan = Plan::new("task");
        plan.add_step("Step 1", None);
        plan.add_step("Step 2", None);
        plan.complete_step(1, "done");
        assert_eq!(plan.steps[0].status, StepStatus::Done);
        assert_eq!(plan.steps[0].result.as_deref(), Some("done"));
    }

    #[test]
    fn test_plan_completion() {
        let mut plan = Plan::new("task");
        plan.add_step("Step 1", None);
        plan.add_step("Step 2", None);
        plan.complete_step(1, "ok");
        assert_ne!(plan.status, PlanStatus::Completed);
        plan.complete_step(2, "ok");
        assert_eq!(plan.status, PlanStatus::Completed);
    }

    #[test]
    fn test_plan_skip_step() {
        let mut plan = Plan::new("task");
        plan.add_step("Step 1", None);
        plan.add_step("Step 2", None);
        plan.complete_step(1, "ok");
        plan.skip_step(2);
        assert_eq!(plan.status, PlanStatus::Completed);
    }

    #[test]
    fn test_plan_fail_step() {
        let mut plan = Plan::new("task");
        plan.add_step("Step 1", None);
        plan.fail_step(1, "error");
        assert_eq!(plan.steps[0].status, StepStatus::Failed);
    }

    #[test]
    fn test_plan_abort() {
        let mut plan = Plan::new("task");
        plan.approve();
        plan.start_execution();
        plan.abort();
        assert_eq!(plan.status, PlanStatus::Aborted);
    }

    #[test]
    fn test_plan_progress() {
        let mut plan = Plan::new("task");
        plan.add_step("a", None);
        plan.add_step("b", None);
        plan.add_step("c", None);
        plan.complete_step(1, "ok");
        assert_eq!(plan.progress(), (1, 3));
        plan.skip_step(2);
        assert_eq!(plan.progress(), (2, 3));
    }

    #[test]
    fn test_plan_pending_steps() {
        let mut plan = Plan::new("task");
        plan.add_step("a", None);
        plan.add_step("b", None);
        plan.complete_step(1, "ok");
        assert_eq!(plan.pending_steps().len(), 1);
    }

    #[test]
    fn test_plan_next_pending() {
        let mut plan = Plan::new("task");
        plan.add_step("a", None);
        plan.add_step("b", None);
        let step = plan.next_pending().unwrap();
        assert_eq!(step.description, "a");
    }

    #[test]
    fn test_plan_format_display() {
        let mut plan = Plan::new("Fix bug");
        plan.add_step("Read code", Some("fs_read"));
        plan.add_step("Fix it", Some("fs_edit"));
        plan.complete_step(1, "ok");
        let display = plan.format_display();
        assert!(display.contains("Fix bug"));
        assert!(display.contains("[x]"));
        assert!(display.contains("[ ]"));
    }

    #[test]
    fn test_planner_create_plan() {
        let planner = Planner::new(10);
        let plan = planner.create_plan(
            "Build feature",
            vec![("Design", None), ("Implement", Some("fs_edit"))],
        );
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.goal, "Build feature");
    }

    #[test]
    fn test_planner_max_steps() {
        let planner = Planner::new(5);
        let plan = planner.create_plan(
            "task",
            vec![
                ("a", None),
                ("b", None),
                ("c", None),
                ("d", None),
                ("e", None),
                ("f", None), // truncated
            ],
        );
        assert_eq!(plan.steps.len(), 5);
    }
}
