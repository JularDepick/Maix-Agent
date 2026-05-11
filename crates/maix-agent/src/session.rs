//! Session state — conversation history.

use maix_core::{Message, MessageContent, Role, ToolCall};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub total_tokens: u64,
    pub turn_count: u64,
}

impl Session {
    pub fn new(id: String) -> Self {
        Self {
            id,
            messages: Vec::new(),
            total_tokens: 0,
            turn_count: 0,
        }
    }

    pub fn add_message(&mut self, role: Role, content: &str, tokens: u64) {
        self.add_message_with_reasoning(role, content, None, tokens);
    }

    pub fn add_message_with_reasoning(
        &mut self,
        role: Role,
        content: &str,
        reasoning: Option<String>,
        tokens: u64,
    ) {
        self.total_tokens += tokens;
        self.messages.push(Message {
            role,
            content: MessageContent::Text(content.to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: reasoning,
        });
    }

    pub fn add_assistant_tool_calls(&mut self, tool_calls: &[ToolCall], reasoning: Option<String>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Text(String::new()),
            name: None,
            tool_call_id: None,
            tool_calls: Some(tool_calls.to_vec()),
            reasoning_content: reasoning,
        });
    }

    pub fn add_tool_result(&mut self, tool_call_id: &str, result: &str) {
        self.messages.push(Message {
            role: Role::Tool,
            content: MessageContent::Text(result.to_string()),
            name: None,
            tool_call_id: Some(tool_call_id.to_string()),
            tool_calls: None,
            reasoning_content: None,
        });
    }
}
