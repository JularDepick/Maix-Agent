//! Context-aware tool selection — recommends tools based on conversation context.

/// A tool's metadata for selection scoring.
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub prerequisites: Vec<String>,
}

/// Context events that influence tool selection.
#[derive(Debug, Clone)]
pub enum ContextEvent {
    FileOpened(String),
    ErrorOccurred(String),
    ToolUsed(String),
    GitStatusChanged,
    SearchQuery(String),
}

/// Scores and recommends tools based on context.
pub struct ToolSelector {
    tools: Vec<ToolInfo>,
    history: Vec<ContextEvent>,
}

impl ToolSelector {
    pub fn new(tools: Vec<ToolInfo>) -> Self {
        Self {
            tools,
            history: Vec::new(),
        }
    }

    pub fn record_event(&mut self, event: ContextEvent) {
        self.history.push(event);
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn history(&self) -> &[ContextEvent] {
        &self.history
    }

    /// Suggest tools based on user input and context.
    pub fn suggest_tools(&self, user_input: &str) -> Vec<(&str, f32)> {
        let input_lower = user_input.to_lowercase();
        let mut scores: Vec<(&str, f32)> = self
            .tools
            .iter()
            .map(|tool| {
                let mut score = 0.0f32;

                // Trigger keyword matching
                for trigger in &tool.triggers {
                    if input_lower.contains(&trigger.to_lowercase()) {
                        score += 2.0;
                    }
                }

                // Context-based scoring
                for event in &self.history {
                    match event {
                        ContextEvent::ErrorOccurred(err) => {
                            if matches!(tool.name.as_str(), "grep" | "search" | "fs_read") {
                                score += 1.5;
                            }
                            if err.contains("permission") && tool.name == "shell_exec" {
                                score -= 1.0;
                            }
                        }
                        ContextEvent::FileOpened(f) => {
                            if tool.name == "fs_edit" || tool.name == "fs_read" {
                                score += 1.0;
                            }
                            if f.ends_with(".rs") && tool.name == "ast_definitions" {
                                score += 0.5;
                            }
                        }
                        ContextEvent::ToolUsed(prev_tool) => {
                            // Chain bonuses
                            if prev_tool == "grep" && tool.name == "fs_read" {
                                score += 0.8;
                            }
                            if prev_tool == "fs_read" && tool.name == "fs_edit" {
                                score += 0.8;
                            }
                        }
                        ContextEvent::SearchQuery(_) => {
                            if matches!(tool.name.as_str(), "grep" | "glob" | "search") {
                                score += 1.0;
                            }
                        }
                        _ => {}
                    }
                }

                (tool.name.as_str(), score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    pub fn top_suggestions(&self, user_input: &str, limit: usize) -> Vec<(&str, f32)> {
        self.suggest_tools(user_input)
            .into_iter()
            .filter(|(_, s)| *s > 0.5)
            .take(limit)
            .collect()
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tools() -> Vec<ToolInfo> {
        vec![
            ToolInfo {
                name: "fs_read".into(),
                description: "Read file".into(),
                triggers: vec!["read".into(), "show".into(), "view".into()],
                prerequisites: vec![],
            },
            ToolInfo {
                name: "fs_edit".into(),
                description: "Edit file".into(),
                triggers: vec!["edit".into(), "change".into(), "fix".into()],
                prerequisites: vec![],
            },
            ToolInfo {
                name: "grep".into(),
                description: "Search content".into(),
                triggers: vec!["search".into(), "find".into(), "grep".into()],
                prerequisites: vec![],
            },
            ToolInfo {
                name: "shell_exec".into(),
                description: "Run command".into(),
                triggers: vec!["run".into(), "execute".into(), "shell".into()],
                prerequisites: vec![],
            },
        ]
    }

    #[test]
    fn test_selector_new() {
        let selector = ToolSelector::new(sample_tools());
        assert_eq!(selector.tool_count(), 4);
    }

    #[test]
    fn test_trigger_match() {
        let selector = ToolSelector::new(sample_tools());
        let suggestions = selector.suggest_tools("please read the file");
        assert!(suggestions.iter().any(|(name, _)| *name == "fs_read"));
    }

    #[test]
    fn test_context_error_boosts_search() {
        let mut selector = ToolSelector::new(sample_tools());
        selector.record_event(ContextEvent::ErrorOccurred("file not found".into()));
        let suggestions = selector.suggest_tools("check the code");
        let grep_score = suggestions
            .iter()
            .find(|(n, _)| *n == "grep")
            .map(|(_, s)| *s)
            .unwrap_or(0.0);
        assert!(grep_score > 0.0);
    }

    #[test]
    fn test_context_file_opened_boosts_edit() {
        let mut selector = ToolSelector::new(sample_tools());
        selector.record_event(ContextEvent::FileOpened("main.rs".into()));
        let suggestions = selector.suggest_tools("update the code");
        let edit_score = suggestions
            .iter()
            .find(|(n, _)| *n == "fs_edit")
            .map(|(_, s)| *s)
            .unwrap_or(0.0);
        assert!(edit_score > 0.0);
    }

    #[test]
    fn test_tool_chain_bonus() {
        let mut selector = ToolSelector::new(sample_tools());
        selector.record_event(ContextEvent::ToolUsed("grep".into()));
        let suggestions = selector.suggest_tools("look at results");
        let read_score = suggestions
            .iter()
            .find(|(n, _)| *n == "fs_read")
            .map(|(_, s)| *s)
            .unwrap_or(0.0);
        assert!(read_score > 0.0);
    }

    #[test]
    fn test_top_suggestions_limit() {
        let mut selector = ToolSelector::new(sample_tools());
        selector.record_event(ContextEvent::FileOpened("main.rs".into()));
        let suggestions = selector.top_suggestions("edit the file", 2);
        assert!(suggestions.len() <= 2);
    }

    #[test]
    fn test_clear_history() {
        let mut selector = ToolSelector::new(sample_tools());
        selector.record_event(ContextEvent::ErrorOccurred("err".into()));
        assert_eq!(selector.history().len(), 1);
        selector.clear_history();
        assert!(selector.history().is_empty());
    }

    #[test]
    fn test_no_match_low_score() {
        let selector = ToolSelector::new(sample_tools());
        let suggestions = selector.suggest_tools("xyzzy foobar");
        assert!(suggestions.iter().all(|(_, s)| *s < 0.5));
    }
}
