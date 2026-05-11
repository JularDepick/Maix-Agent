//! Multi-model routing based on task category (Phase 2.2).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskCategory {
    Chat,
    Coding,
    Reasoning,
    Research,
    FastReply,
}

impl TaskCategory {
    pub fn default_model(&self) -> &'static str {
        match self {
            TaskCategory::Reasoning | TaskCategory::Research => "deepseek-v4-pro",
            TaskCategory::Coding => "deepseek-v4-flash",
            TaskCategory::Chat | TaskCategory::FastReply => "deepseek-chat",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelRoute {
    pub provider: String,
    pub model: String,
}

impl ModelRoute {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelRouter {
    routes: HashMap<TaskCategory, ModelRoute>,
    default_route: ModelRoute,
}

impl ModelRouter {
    pub fn new(default_provider: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            routes: HashMap::new(),
            default_route: ModelRoute::new(default_provider, default_model),
        }
    }

    pub fn with_route(
        mut self,
        category: TaskCategory,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        self.routes
            .insert(category, ModelRoute::new(provider, model));
        self
    }

    pub fn route(&self, category: Option<TaskCategory>) -> &ModelRoute {
        category
            .and_then(|c| self.routes.get(&c))
            .unwrap_or(&self.default_route)
    }

    pub fn default_route(&self) -> &ModelRoute {
        &self.default_route
    }
}

/// Detect task category from user input.
pub fn detect_category(input: &str, explicit_tag: Option<&str>) -> Option<TaskCategory> {
    if let Some(tag) = explicit_tag {
        return match tag.to_lowercase().as_str() {
            "code" | "coding" => Some(TaskCategory::Coding),
            "reason" | "reasoning" | "think" => Some(TaskCategory::Reasoning),
            "research" | "search" | "investigate" => Some(TaskCategory::Research),
            "fast" | "quick" => Some(TaskCategory::FastReply),
            _ => None,
        };
    }

    let lower = input.to_lowercase();

    // Coding signals
    let code_keywords = [
        "code", "function", "bug", "error", "compile", "rust", "python",
        "impl ", "trait", "struct", "fn ", "let ", "import ", "def ",
        "fix ", "debug", "refactor", "api", "endpoint", "route",
    ];
    let code_score = code_keywords.iter().filter(|k| lower.contains(*k)).count();
    if code_score >= 3 {
        return Some(TaskCategory::Coding);
    }

    // Reasoning signals
    let reason_keywords = [
        "explain", "why", "how does", "analyze", "compare", "evaluate",
        "design", "architecture", "trade-off", "best practice", "recommend",
    ];
    let reason_score = reason_keywords.iter().filter(|k| lower.contains(*k)).count();
    if reason_score >= 3 {
        return Some(TaskCategory::Reasoning);
    }

    // Research signals
    let research_keywords = [
        "search", "find", "research", "investigate", "look up", "latest",
        "news", "history of", "what is", "who is", "when did",
    ];
    let research_score = research_keywords
        .iter()
        .filter(|k| lower.contains(*k))
        .count();
    if research_score >= 2 {
        return Some(TaskCategory::Research);
    }

    // Fast reply signals (very short, simple questions)
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() <= 5 {
        let fast_keywords = ["hi", "hello", "thanks", "ok", "yes", "no", "bye"];
        if fast_keywords.iter().any(|k| lower.contains(*k)) {
            return Some(TaskCategory::FastReply);
        }
    }

    // Default: no specific category (router will use default)
    if code_score > 0 {
        Some(TaskCategory::Coding)
    } else if reason_score > 0 {
        Some(TaskCategory::Reasoning)
    } else {
        None // Use default
    }
}
