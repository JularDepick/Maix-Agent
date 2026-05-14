//! Multi-model routing based on task category.
//!
//! Supports two modes:
//! - **Static routing**: map TaskCategory → ModelRoute
//! - **Auto routing**: per-turn decision between cheap and capable models
//!   based on task complexity (thinking level)

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

/// Thinking level for auto-mode routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ThinkingLevel {
    /// No thinking needed — use cheap/fast model.
    #[default]
    Off,
    /// Standard thinking — use capable model.
    High,
    /// Maximum thinking — use capable model with extended thinking.
    Max,
}


/// A routing decision for a single turn.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Which model to use.
    pub route: ModelRoute,
    /// Thinking level for this turn.
    pub thinking_level: ThinkingLevel,
    /// Reason for this routing choice (for debugging/logging).
    pub reason: String,
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

/// Configuration for auto-mode routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct AutoModeConfig {
    /// Enable auto-mode routing.
    #[serde(default)]
    pub enabled: bool,
    /// Cheap/fast model for simple tasks.
    #[serde(default)]
    pub cheap_model: String,
    /// Provider for cheap model (falls back to default provider).
    #[serde(default)]
    pub cheap_provider: String,
    /// Capable model for complex tasks.
    #[serde(default)]
    pub capable_model: String,
    /// Provider for capable model (falls back to default provider).
    #[serde(default)]
    pub capable_provider: String,
}


#[derive(Debug, Clone)]
pub struct ModelRouter {
    routes: HashMap<TaskCategory, ModelRoute>,
    default_route: ModelRoute,
    auto_mode: AutoModeConfig,
}

impl ModelRouter {
    pub fn new(default_provider: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            routes: HashMap::new(),
            default_route: ModelRoute::new(default_provider, default_model),
            auto_mode: AutoModeConfig::default(),
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

    pub fn with_auto_mode(mut self, config: AutoModeConfig) -> Self {
        self.auto_mode = config;
        self
    }

    /// Get a static route by category (legacy behavior).
    pub fn route(&self, category: Option<TaskCategory>) -> &ModelRoute {
        category
            .and_then(|c| self.routes.get(&c))
            .unwrap_or(&self.default_route)
    }

    pub fn default_route(&self) -> &ModelRoute {
        &self.default_route
    }

    /// Make a per-turn routing decision using auto-mode logic.
    /// Falls back to static routing if auto-mode is disabled.
    pub fn decide(&self, user_input: &str, context_tokens: u64, max_context: u64) -> RoutingDecision {
        if !self.auto_mode.enabled || (self.auto_mode.cheap_model.is_empty() && self.auto_mode.capable_model.is_empty()) {
            // Auto-mode disabled — use static routing
            let category = detect_category(user_input, None);
            let route = self.route(category).clone();
            return RoutingDecision {
                route,
                thinking_level: ThinkingLevel::Off,
                reason: "auto-mode disabled, using static route".into(),
            };
        }

        let complexity = estimate_complexity(user_input, context_tokens, max_context);

        match complexity {
            ThinkingLevel::Off => {
                let provider = if self.auto_mode.cheap_provider.is_empty() {
                    &self.default_route.provider
                } else {
                    &self.auto_mode.cheap_provider
                };
                let model = if self.auto_mode.cheap_model.is_empty() {
                    &self.default_route.model
                } else {
                    &self.auto_mode.cheap_model
                };
                RoutingDecision {
                    route: ModelRoute::new(provider, model),
                    thinking_level: ThinkingLevel::Off,
                    reason: "simple task (complexity=off)".to_string(),
                }
            }
            ThinkingLevel::High => {
                let provider = if self.auto_mode.capable_provider.is_empty() {
                    &self.default_route.provider
                } else {
                    &self.auto_mode.capable_provider
                };
                let model = if self.auto_mode.capable_model.is_empty() {
                    &self.default_route.model
                } else {
                    &self.auto_mode.capable_model
                };
                RoutingDecision {
                    route: ModelRoute::new(provider, model),
                    thinking_level: ThinkingLevel::High,
                    reason: "complex task (complexity=high)".to_string(),
                }
            }
            ThinkingLevel::Max => {
                let provider = if self.auto_mode.capable_provider.is_empty() {
                    &self.default_route.provider
                } else {
                    &self.auto_mode.capable_provider
                };
                let model = if self.auto_mode.capable_model.is_empty() {
                    &self.default_route.model
                } else {
                    &self.auto_mode.capable_model
                };
                RoutingDecision {
                    route: ModelRoute::new(provider, model),
                    thinking_level: ThinkingLevel::Max,
                    reason: "deep reasoning required (complexity=max)".to_string(),
                }
            }
        }
    }
}

/// Estimate thinking level needed for a turn based on input and context.
fn estimate_complexity(input: &str, context_tokens: u64, max_context: u64) -> ThinkingLevel {
    let lower = input.to_lowercase();
    let word_count = input.split_whitespace().count();

    // Explicit deep-reasoning signals → Max
    let max_signals = [
        "think step by step",
        "deeply analyze",
        "carefully reason",
        "thoroughly explain",
        "design a system",
        "architect",
        "trade-off",
        "compare and contrast",
        "prove that",
        "derive",
    ];
    if max_signals.iter().any(|s| lower.contains(s)) {
        return ThinkingLevel::Max;
    }

    // Complex task signals → High
    let high_signals = [
        "refactor", "implement", "debug", "fix this bug",
        "write a function", "write a class", "create a module",
        "explain how", "why does", "what is the difference",
        "optimize", "migrate", "review this code", "test",
        "error", "exception", "stack trace", "panic",
        "complex", "multiple files", "entire codebase",
    ];
    let high_score = high_signals.iter().filter(|s| lower.contains(*s)).count();
    if high_score >= 2 {
        return ThinkingLevel::High;
    }

    // Context pressure → High (model needs to be more careful)
    if max_context > 0 {
        let usage = context_tokens as f64 / max_context as f64;
        if usage > 0.7 {
            return ThinkingLevel::High;
        }
    }

    // Short simple input → Off
    if word_count <= 10 && high_score == 0 {
        return ThinkingLevel::Off;
    }

    // Medium complexity → High
    if word_count > 30 || high_score > 0 {
        return ThinkingLevel::High;
    }

    ThinkingLevel::Off
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
        if words.iter().any(|w| fast_keywords.contains(w)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_routing() {
        let router = ModelRouter::new("openai", "gpt-4o")
            .with_route(TaskCategory::Coding, "anthropic", "claude-sonnet-4-6")
            .with_route(TaskCategory::FastReply, "openai", "gpt-4o-mini");

        let route = router.route(Some(TaskCategory::Coding));
        assert_eq!(route.model, "claude-sonnet-4-6");

        let route = router.route(Some(TaskCategory::FastReply));
        assert_eq!(route.model, "gpt-4o-mini");

        // Unknown category falls back to default
        let route = router.route(Some(TaskCategory::Chat));
        assert_eq!(route.model, "gpt-4o");
    }

    #[test]
    fn test_auto_mode_disabled() {
        let router = ModelRouter::new("openai", "gpt-4o");
        let decision = router.decide("hello", 0, 100000);
        assert_eq!(decision.thinking_level, ThinkingLevel::Off);
        assert_eq!(decision.route.model, "gpt-4o");
    }

    #[test]
    fn test_auto_mode_simple_task() {
        let router = ModelRouter::new("openai", "gpt-4o").with_auto_mode(AutoModeConfig {
            enabled: true,
            cheap_model: "gpt-4o-mini".into(),
            cheap_provider: String::new(),
            capable_model: "claude-sonnet-4-6".into(),
            capable_provider: "anthropic".into(),
        });

        let decision = router.decide("hi", 0, 100000);
        assert_eq!(decision.thinking_level, ThinkingLevel::Off);
        assert_eq!(decision.route.model, "gpt-4o-mini");
    }

    #[test]
    fn test_auto_mode_complex_task() {
        let router = ModelRouter::new("openai", "gpt-4o").with_auto_mode(AutoModeConfig {
            enabled: true,
            cheap_model: "gpt-4o-mini".into(),
            cheap_provider: String::new(),
            capable_model: "claude-sonnet-4-6".into(),
            capable_provider: "anthropic".into(),
        });

        let decision = router.decide("refactor this bug in the error handler", 0, 100000);
        assert_eq!(decision.thinking_level, ThinkingLevel::High);
        assert_eq!(decision.route.model, "claude-sonnet-4-6");
    }

    #[test]
    fn test_auto_mode_deep_reasoning() {
        let router = ModelRouter::new("openai", "gpt-4o").with_auto_mode(AutoModeConfig {
            enabled: true,
            cheap_model: "gpt-4o-mini".into(),
            cheap_provider: String::new(),
            capable_model: "claude-opus-4-7".into(),
            capable_provider: "anthropic".into(),
        });

        let decision = router.decide("think step by step about this algorithm", 0, 100000);
        assert_eq!(decision.thinking_level, ThinkingLevel::Max);
        assert_eq!(decision.route.model, "claude-opus-4-7");
    }

    #[test]
    fn test_auto_mode_context_pressure() {
        let router = ModelRouter::new("openai", "gpt-4o").with_auto_mode(AutoModeConfig {
            enabled: true,
            cheap_model: "gpt-4o-mini".into(),
            cheap_provider: String::new(),
            capable_model: "claude-sonnet-4-6".into(),
            capable_provider: "anthropic".into(),
        });

        // 80% context usage → High
        let decision = router.decide("ok", 80000, 100000);
        assert_eq!(decision.thinking_level, ThinkingLevel::High);
    }

    #[test]
    fn test_estimate_complexity() {
        assert_eq!(estimate_complexity("hi", 0, 100000), ThinkingLevel::Off);
        assert_eq!(estimate_complexity("hello world", 0, 100000), ThinkingLevel::Off);
        assert_eq!(
            estimate_complexity("fix this bug in the error handler", 0, 100000),
            ThinkingLevel::High
        );
        assert_eq!(
            estimate_complexity("think step by step about this", 0, 100000),
            ThinkingLevel::Max
        );
    }

    #[test]
    fn test_detect_category() {
        assert_eq!(detect_category("fix this bug", None), Some(TaskCategory::Coding));
        assert_eq!(detect_category("explain how this works", None), Some(TaskCategory::Reasoning));
        assert_eq!(detect_category("search for latest news", None), Some(TaskCategory::Research));
        assert_eq!(detect_category("hi", None), Some(TaskCategory::FastReply));
        assert_eq!(detect_category("tell me a joke", None), None);
    }
}
