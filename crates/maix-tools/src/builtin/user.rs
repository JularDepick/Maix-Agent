//! User interaction tool: ask_user.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};

// ---------------------------------------------------------------------------
// ask_user
// ---------------------------------------------------------------------------

pub struct AskUserTool;

impl Default for AskUserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl AskUserTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ask_user".into(),
            description: "Ask the user a question and wait for their response. Use for clarifications, preferences, or choices.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask the user" },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of choices for the user to pick from"
                    }
                },
                "required": ["question"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let question = args["question"].as_str().unwrap_or_default();
        let options: Vec<String> = args["options"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let display = if options.is_empty() {
            question.to_string()
        } else {
            let opts: String = options.iter().enumerate()
                .map(|(i, o)| format!("  {}. {}", i + 1, o))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}\n\n{}", question, opts)
        };

        if let Some(tx) = &ctx.ask_user_tx {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            tx.send((display, resp_tx)).map_err(|_| {
                maix_core::MaixError::Tool("failed to send question to user".into())
            })?;
            let response = resp_rx.await.map_err(|_| {
                maix_core::MaixError::Tool("user response channel closed".into())
            })?;
            Ok(response)
        } else {
            // No interactive channel — return the question as-is for non-interactive mode
            Ok(format!("[ask_user] {}", display))
        }
    }
}
