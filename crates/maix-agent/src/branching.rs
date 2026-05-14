//! Conversation branching — fork conversations from any point.

use std::collections::HashMap;

/// A node in the conversation tree.
#[derive(Debug, Clone)]
pub struct MessageNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub content: String,
    pub role: String,
}

/// A named branch pointing to a tip node.
#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub tip_node_id: String,
    pub created_from: Option<String>,
}

/// Tree-structured conversation with branching support.
pub struct ConversationTree {
    nodes: HashMap<String, MessageNode>,
    branches: HashMap<String, Branch>,
    active_branch: String,
}

impl ConversationTree {
    pub fn new() -> Self {
        let root_id = "root".to_string();
        let mut nodes = HashMap::new();
        nodes.insert(
            root_id.clone(),
            MessageNode {
                id: root_id.clone(),
                parent_id: None,
                children_ids: Vec::new(),
                content: String::new(),
                role: "system".into(),
            },
        );

        let mut branches = HashMap::new();
        branches.insert(
            "main".to_string(),
            Branch {
                name: "main".into(),
                tip_node_id: root_id,
                created_from: None,
            },
        );

        Self {
            nodes,
            branches,
            active_branch: "main".to_string(),
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) -> String {
        let id = format!("msg_{}", uuid::Uuid::new_v4());
        let parent_id = self
            .branches
            .get(&self.active_branch)
            .map(|b| b.tip_node_id.clone());

        let node = MessageNode {
            id: id.clone(),
            parent_id: parent_id.clone(),
            children_ids: Vec::new(),
            content: content.to_string(),
            role: role.to_string(),
        };

        if let Some(pid) = &parent_id {
            if let Some(parent) = self.nodes.get_mut(pid) {
                parent.children_ids.push(id.clone());
            }
        }

        self.nodes.insert(id.clone(), node);
        if let Some(branch) = self.branches.get_mut(&self.active_branch) {
            branch.tip_node_id = id.clone();
        }
        id
    }

    pub fn fork(&mut self, branch_name: &str) -> String {
        let tip = self
            .branches
            .get(&self.active_branch)
            .map(|b| b.tip_node_id.clone())
            .unwrap_or_else(|| "root".to_string());

        let branch = Branch {
            name: branch_name.to_string(),
            tip_node_id: tip,
            created_from: Some(self.active_branch.clone()),
        };

        self.branches.insert(branch_name.to_string(), branch);
        branch_name.to_string()
    }

    pub fn switch_branch(&mut self, branch_name: &str) -> bool {
        if self.branches.contains_key(branch_name) {
            self.active_branch = branch_name.to_string();
            true
        } else {
            false
        }
    }

    pub fn active_branch(&self) -> &str {
        &self.active_branch
    }

    pub fn branches(&self) -> Vec<&str> {
        self.branches.keys().map(|s| s.as_str()).collect()
    }

    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get_branch_history(&self, branch_name: &str) -> Vec<&MessageNode> {
        let branch = match self.branches.get(branch_name) {
            Some(b) => b,
            None => return Vec::new(),
        };

        let mut history = Vec::new();
        let mut current = Some(&branch.tip_node_id);

        while let Some(node_id) = current {
            if let Some(node) = self.nodes.get(node_id) {
                // Skip the root node
                if node.id != "root" {
                    history.push(node);
                }
                current = node.parent_id.as_ref();
            } else {
                break;
            }
        }

        history.reverse();
        history
    }

    pub fn get_active_history(&self) -> Vec<&MessageNode> {
        let branch = self.active_branch.clone();
        self.get_branch_history(&branch)
    }

    pub fn delete_branch(&mut self, branch_name: &str) -> bool {
        if branch_name == "main" || branch_name == self.active_branch {
            return false;
        }
        self.branches.remove(branch_name).is_some()
    }

    pub fn merge_branch(&mut self, source: &str) -> bool {
        if source == self.active_branch {
            return false;
        }

        let source_tip = match self.branches.get(source) {
            Some(b) => b.tip_node_id.clone(),
            None => return false,
        };

        // Add source tip as child of current active tip
        let active_tip = self
            .branches
            .get(&self.active_branch)
            .map(|b| b.tip_node_id.clone());

        if let Some(pid) = active_tip {
            // Remove source_tip from its old parent's children_ids
            if let Some(old_parent_id) = self.nodes.get(&source_tip).and_then(|n| n.parent_id.clone()) {
                if let Some(old_parent) = self.nodes.get_mut(&old_parent_id) {
                    old_parent.children_ids.retain(|id| id != &source_tip);
                }
            }
            if let Some(parent) = self.nodes.get_mut(&pid) {
                parent.children_ids.push(source_tip.clone());
            }
            if let Some(node) = self.nodes.get_mut(&source_tip) {
                node.parent_id = Some(pid);
            }
            if let Some(branch) = self.branches.get_mut(&self.active_branch) {
                branch.tip_node_id = source_tip;
            }
        }

        self.branches.remove(source);
        true
    }
}

impl Default for ConversationTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_new() {
        let tree = ConversationTree::new();
        assert_eq!(tree.branch_count(), 1);
        assert_eq!(tree.active_branch(), "main");
    }

    #[test]
    fn test_add_message() {
        let mut tree = ConversationTree::new();
        let id = tree.add_message("user", "hello");
        assert!(!id.is_empty());
        assert_eq!(tree.node_count(), 2); // root + message
    }

    #[test]
    fn test_fork() {
        let mut tree = ConversationTree::new();
        tree.add_message("user", "hello");
        tree.fork("experiment");
        assert_eq!(tree.branch_count(), 2);
        assert!(tree.branches().contains(&"experiment"));
    }

    #[test]
    fn test_switch_branch() {
        let mut tree = ConversationTree::new();
        tree.fork("dev");
        assert!(tree.switch_branch("dev"));
        assert_eq!(tree.active_branch(), "dev");
        assert!(!tree.switch_branch("nonexistent"));
    }

    #[test]
    fn test_branch_history() {
        let mut tree = ConversationTree::new();
        tree.add_message("user", "hello");
        tree.add_message("assistant", "hi");
        let history = tree.get_active_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "hello");
        assert_eq!(history[1].content, "hi");
    }

    #[test]
    fn test_fork_isolation() {
        let mut tree = ConversationTree::new();
        tree.add_message("user", "hello");
        tree.fork("branch-a");
        tree.switch_branch("branch-a");
        tree.add_message("user", "branch msg");

        tree.switch_branch("main");
        let main_history = tree.get_active_history();
        assert_eq!(main_history.len(), 1); // only "hello" on main

        tree.switch_branch("branch-a");
        let branch_history = tree.get_active_history();
        assert_eq!(branch_history.len(), 2); // "hello" + "branch msg"
    }

    #[test]
    fn test_delete_branch() {
        let mut tree = ConversationTree::new();
        tree.fork("temp");
        assert!(tree.delete_branch("temp"));
        assert_eq!(tree.branch_count(), 1);
    }

    #[test]
    fn test_cannot_delete_main() {
        let mut tree = ConversationTree::new();
        assert!(!tree.delete_branch("main"));
    }

    #[test]
    fn test_merge_branch() {
        let mut tree = ConversationTree::new();
        tree.add_message("user", "main msg");
        tree.fork("feature");
        tree.switch_branch("feature");
        tree.add_message("user", "feature done");

        tree.switch_branch("main");
        assert!(tree.merge_branch("feature"));
        assert_eq!(tree.branch_count(), 1); // feature removed

        let history = tree.get_active_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_cannot_merge_self() {
        let mut tree = ConversationTree::new();
        assert!(!tree.merge_branch("main"));
    }
}
