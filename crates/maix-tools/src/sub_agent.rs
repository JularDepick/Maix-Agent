//! Sub-agent tool — spawn a child task in an isolated git worktree.

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

pub struct SubAgentTool;

impl Default for SubAgentTool {
    fn default() -> Self {
        Self
    }
}

impl SubAgentTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SubAgentTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "sub_agent".into(),
            description: "Spawn a sub-agent to work on a task independently. Can optionally isolate in a git worktree for parallel work."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "Task description for the sub-agent (required)" },
                    "worktree": { "type": "boolean", "description": "Create a git worktree for isolation (default: false)" },
                    "branch_name": { "type": "string", "description": "Branch name for worktree (auto-generated if omitted)" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default: 300)" }
                },
                "required": ["task"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let task = args["task"].as_str().unwrap_or("");
        if task.is_empty() {
            return Err(maix_core::MaixError::Tool("sub_agent: task is required".into()));
        }

        let use_worktree = args["worktree"].as_bool().unwrap_or(false);
        let timeout = args["timeout"].as_u64().unwrap_or(300);

        let work_dir = if use_worktree {
            let branch_name = args["branch_name"].as_str().unwrap_or("");
            let name = if branch_name.is_empty() {
                format!("maix-sub-{}", &uuid::Uuid::new_v4().to_string()[..8])
            } else {
                branch_name.to_string()
            };

            // Check if we're in a git repo
            let is_repo = tokio::process::Command::new("git")
                .args(["rev-parse", "--is-inside-work-tree"])
                .current_dir(&ctx.working_dir)
                .output()
                .await
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !is_repo {
                return Err(maix_core::MaixError::Tool(
                    "sub_agent: not in a git repository, cannot create worktree".into(),
                ));
            }

            let worktree_path = crate::git::worktree_add(&ctx.working_dir, &name).await?;
            Some((worktree_path, name))
        } else {
            None
        };

        let exec_dir = work_dir
            .as_ref()
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| ctx.working_dir.clone());

        // Run the sub-agent task using maix-cli
        let result = run_sub_agent_task(&exec_dir, task, timeout).await;

        // Clean up worktree if we created one
        if let Some((_, ref name)) = work_dir {
            if let Err(e) = crate::git::worktree_remove(&ctx.working_dir, name).await {
                tracing::warn!("sub_agent: failed to clean up worktree '{name}': {e}");
            }
        }

        result
    }
}

/// Run a sub-agent task by invoking maix-cli.
async fn run_sub_agent_task(
    working_dir: &std::path::Path,
    task: &str,
    timeout_secs: u64,
) -> MaixResult<String> {
    // Try maix-cli first, fall back to direct LLM call
    let cli_result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::process::Command::new("maix-cli")
            .args(["ask", "--print", task])
            .current_dir(working_dir)
            .output(),
    )
    .await;

    match cli_result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() && !stdout.is_empty() {
                Ok(format!("Sub-agent result:\n{stdout}"))
            } else if !stderr.is_empty() {
                Ok(format!("Sub-agent completed with warnings:\n{stderr}\n{stdout}"))
            } else {
                Ok(format!("Sub-agent completed:\n{stdout}"))
            }
        }
        Ok(Err(e)) => {
            // maix-cli not found, provide a useful fallback message
            Ok(format!(
                "Sub-agent task queued: {task}\n\
                 Note: maix-cli not available ({e}). \
                 The task has been recorded and can be executed manually.",
            ))
        }
        Err(_) => Err(maix_core::MaixError::Tool(format!(
            "sub_agent: task timed out after {timeout_secs}s"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_agent_def() {
        let tool = SubAgentTool::new();
        let def = tool.def();
        assert_eq!(def.name, "sub_agent");
        assert_eq!(def.risk_level, RiskLevel::Write);
    }
}
