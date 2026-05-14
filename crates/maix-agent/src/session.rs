//! Session state — conversation history with branching support.

use maix_core::{ContentPart, ImageUrl, Message, MessageContent, Role, ToolCall};
use serde::{Deserialize, Serialize};

/// A conversation branch forked from a specific message index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub name: String,
    pub fork_point: usize,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub description: Option<String>,
}

/// Merge strategy for combining branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Append all branch messages to the main conversation.
    Append,
    /// Compress the branch into a single summary message.
    Squash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub total_tokens: u64,
    pub turn_count: u64,
    #[serde(default)]
    pub branches: Vec<Branch>,
    #[serde(default)]
    pub active_branch: Option<String>,
}

impl Session {
    pub fn new(id: String) -> Self {
        Self {
            id,
            messages: Vec::new(),
            total_tokens: 0,
            turn_count: 0,
            branches: Vec::new(),
            active_branch: None,
        }
    }

    /// Fork the session at the current message count.
    pub fn fork(&mut self, name: &str, description: Option<&str>) -> String {
        self.fork_at(self.messages.len(), name, description)
    }

    /// Fork the session at a specific message index.
    pub fn fork_at(&mut self, index: usize, name: &str, description: Option<&str>) -> String {
        let branch_id = format!("{}_{}", name, chrono::Utc::now().timestamp_millis());
        let branch = Branch {
            id: branch_id.clone(),
            name: name.to_string(),
            fork_point: index.min(self.messages.len()),
            messages: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            description: description.map(|s| s.to_string()),
        };
        self.branches.push(branch);
        branch_id
    }

    /// Switch to a branch, making it the active branch.
    pub fn switch_branch(&mut self, branch_id: &str) -> Result<(), String> {
        if self.branches.iter().any(|b| b.id == branch_id) {
            self.active_branch = Some(branch_id.to_string());
            Ok(())
        } else {
            Err(format!("branch not found: {branch_id}"))
        }
    }

    /// Switch back to the main trunk (no active branch).
    pub fn switch_to_main(&mut self) {
        self.active_branch = None;
    }

    /// List all branches.
    pub fn list_branches(&self) -> &[Branch] {
        &self.branches
    }

    /// Get the active branch, if any.
    pub fn active_branch(&self) -> Option<&Branch> {
        self.active_branch
            .as_ref()
            .and_then(|id| self.branches.iter().find(|b| b.id == *id))
    }

    /// Get a mutable reference to the active branch.
    fn active_branch_mut(&mut self) -> Option<&mut Branch> {
        self.active_branch
            .clone()
            .and_then(move |id| self.branches.iter_mut().find(|b| b.id == id))
    }

    /// Add a message to the active branch (or main trunk if no active branch).
    pub fn add_message_to_active(&mut self, role: Role, content: &str, tokens: u64) {
        let msg = Message {
            role,
            content: MessageContent::Text(content.to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };

        if let Some(branch) = self.active_branch_mut() {
            branch.messages.push(msg);
        } else {
            self.total_tokens += tokens;
            self.messages.push(msg);
        }
    }

    /// Merge a branch into the main trunk.
    pub fn merge_branch(&mut self, branch_id: &str, strategy: MergeStrategy) -> Result<usize, String> {
        let branch_idx = self.branches.iter().position(|b| b.id == branch_id)
            .ok_or_else(|| format!("branch not found: {branch_id}"))?;

        let branch = self.branches.remove(branch_idx);
        let count = branch.messages.len();

        match strategy {
            MergeStrategy::Append => {
                // Insert branch messages at the fork point
                let insert_at = branch.fork_point.min(self.messages.len());
                for (i, msg) in branch.messages.into_iter().enumerate() {
                    self.messages.insert(insert_at + i, msg);
                }
            }
            MergeStrategy::Squash => {
                // Create a single summary message
                let summary: String = branch.messages.iter()
                    .filter_map(|m| m.content.text())
                    .collect::<Vec<_>>()
                    .join("\n---\n");
                let insert_at = branch.fork_point.min(self.messages.len());
                self.messages.insert(insert_at, Message {
                    role: Role::System,
                    content: MessageContent::Text(format!("[Branch '{}' merged]\n{}", branch.name, summary)),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                });
            }
        }

        // Clear active branch if it was the merged one
        if self.active_branch.as_deref() == Some(branch_id) {
            self.active_branch = None;
        }

        Ok(count)
    }

    /// Delete a branch without merging.
    pub fn delete_branch(&mut self, branch_id: &str) -> Result<(), String> {
        let idx = self.branches.iter().position(|b| b.id == branch_id)
            .ok_or_else(|| format!("branch not found: {branch_id}"))?;
        self.branches.remove(idx);
        if self.active_branch.as_deref() == Some(branch_id) {
            self.active_branch = None;
        }
        Ok(())
    }

    /// Get a combined view of messages: main trunk + active branch messages.
    pub fn effective_messages(&self) -> Vec<&Message> {
        let mut msgs: Vec<&Message> = self.messages.iter().collect();
        if let Some(branch) = self.active_branch() {
            msgs.extend(branch.messages.iter());
        }
        msgs
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
        // Check if result is an image (JSON with type "image")
        let content = if let Ok(json) = serde_json::from_str::<serde_json::Value>(result) {
            if json.get("type").and_then(|t| t.as_str()) == Some("image") {
                if let Some(source) = json.get("source") {
                    let media_type = source.get("media_type").and_then(|t| t.as_str()).unwrap_or("image/png");
                    let data = source.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    let url = format!("data:{};base64,{}", media_type, data);
                    MessageContent::Parts(vec![ContentPart::ImageUrl {
                        image_url: ImageUrl { url, detail: None },
                    }])
                } else {
                    MessageContent::Text(result.to_string())
                }
            } else {
                MessageContent::Text(result.to_string())
            }
        } else {
            MessageContent::Text(result.to_string())
        };

        self.messages.push(Message {
            role: Role::Tool,
            content,
            name: None,
            tool_call_id: Some(tool_call_id.to_string()),
            tool_calls: None,
            reasoning_content: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_and_list_branches() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "hello", 5);
        session.add_message(Role::Assistant, "hi", 3);

        let branch_id = session.fork("test-branch", Some("testing"));
        assert_eq!(session.list_branches().len(), 1);
        assert_eq!(session.list_branches()[0].name, "test-branch");
        assert_eq!(session.list_branches()[0].fork_point, 2);
        let _ = branch_id;
    }

    #[test]
    fn test_fork_at_index() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "msg1", 5);
        session.add_message(Role::Assistant, "msg2", 3);
        session.add_message(Role::User, "msg3", 5);

        let branch_id = session.fork_at(1, "mid-fork", None);
        assert_eq!(session.list_branches()[0].fork_point, 1);
        let _ = branch_id;
    }

    #[test]
    fn test_switch_branch() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "hello", 5);
        let branch_id = session.fork("b1", None);

        assert!(session.switch_branch(&branch_id).is_ok());
        assert_eq!(session.active_branch().unwrap().name, "b1");

        assert!(session.switch_branch("nonexistent").is_err());
    }

    #[test]
    fn test_switch_to_main() {
        let mut session = Session::new("s1".into());
        let branch_id = session.fork("b1", None);
        session.switch_branch(&branch_id).unwrap();
        session.switch_to_main();
        assert!(session.active_branch().is_none());
    }

    #[test]
    fn test_add_message_to_branch() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "hello", 5);
        let branch_id = session.fork("b1", None);
        session.switch_branch(&branch_id).unwrap();

        session.add_message_to_active(Role::User, "branch msg", 5);

        // Main trunk still has 1 message
        assert_eq!(session.messages.len(), 1);
        // Branch has 1 message
        assert_eq!(session.active_branch().unwrap().messages.len(), 1);
    }

    #[test]
    fn test_effective_messages() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "main msg", 5);
        let branch_id = session.fork("b1", None);
        session.switch_branch(&branch_id).unwrap();
        session.add_message_to_active(Role::User, "branch msg", 5);

        let effective = session.effective_messages();
        assert_eq!(effective.len(), 2);
    }

    #[test]
    fn test_merge_append() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "msg1", 5);
        let branch_id = session.fork("b1", None);
        session.switch_branch(&branch_id).unwrap();
        session.add_message_to_active(Role::User, "branch msg", 5);

        session.switch_to_main();
        let count = session.merge_branch(&branch_id, MergeStrategy::Append).unwrap();
        assert_eq!(count, 1);
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_merge_squash() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "msg1", 5);
        let branch_id = session.fork("b1", None);
        session.switch_branch(&branch_id).unwrap();
        session.add_message_to_active(Role::User, "branch msg 1", 5);
        session.add_message_to_active(Role::User, "branch msg 2", 5);

        session.switch_to_main();
        let count = session.merge_branch(&branch_id, MergeStrategy::Squash).unwrap();
        assert_eq!(count, 2);
        // Original 1 message + 1 squashed summary
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_delete_branch() {
        let mut session = Session::new("s1".into());
        let branch_id = session.fork("b1", None);
        assert_eq!(session.list_branches().len(), 1);

        session.delete_branch(&branch_id).unwrap();
        assert_eq!(session.list_branches().len(), 0);
    }

    #[test]
    fn test_multiple_branches() {
        let mut session = Session::new("s1".into());
        session.add_message(Role::User, "hello", 5);

        let b1 = session.fork("branch-1", None);
        let b2 = session.fork("branch-2", None);
        assert_eq!(session.list_branches().len(), 2);

        session.switch_branch(&b1).unwrap();
        assert_eq!(session.active_branch().unwrap().name, "branch-1");

        session.switch_branch(&b2).unwrap();
        assert_eq!(session.active_branch().unwrap().name, "branch-2");
    }
}
