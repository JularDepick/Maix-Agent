use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// Content can be plain text or multimodal parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
    ImageBase64 { source: ImageSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A tool call made by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// OpenAI-compatible tool definition sent to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDef {
    pub fn new(name: &str, description: &str, parameters: Value) -> Self {
        Self {
            tool_type: "function".into(),
            function: FunctionDef {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Token usage info.
/// Supports aliases for different provider formats:
/// - DeepSeek: prompt_cache_hit_tokens / prompt_cache_miss_tokens
/// - OpenAI: prompt_tokens_details.cached_tokens (mapped externally)
/// - Anthropic: cache_creation_input_tokens / cache_read_input_tokens
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, alias = "prompt_cache_hit_tokens", alias = "cache_read_input_tokens")]
    pub cache_read_tokens: u64,
    #[serde(default, alias = "prompt_cache_miss_tokens", alias = "cache_creation_input_tokens")]
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    /// Merge another usage into this one (accumulate).
    pub fn merge(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }

    /// Calculate cost based on pricing.
    pub fn cost(&self, pricing: &Pricing) -> f64 {
        let non_cached_input = self.prompt_tokens.saturating_sub(self.cache_read_tokens);
        non_cached_input as f64 * pricing.input_per_million / 1_000_000.0
            + self.output_tokens() as f64 * pricing.output_per_million / 1_000_000.0
            + self.cache_read_tokens as f64 * pricing.cache_read_per_million / 1_000_000.0
            + self.cache_write_tokens as f64 * pricing.cache_write_per_million / 1_000_000.0
    }

    /// Output tokens (alias for completion_tokens).
    pub fn output_tokens(&self) -> u64 {
        self.completion_tokens
    }

    /// Cache hit rate: cache_read / (prompt_tokens).
    pub fn cache_hit_rate(&self) -> f64 {
        if self.prompt_tokens == 0 {
            0.0
        } else {
            self.cache_read_tokens as f64 / self.prompt_tokens as f64 * 100.0
        }
    }

    /// Savings from cache (what it would have cost at full input price).
    pub fn cache_savings(&self, pricing: &Pricing) -> f64 {
        self.cache_read_tokens as f64 * (pricing.input_per_million - pricing.cache_read_per_million) / 1_000_000.0
    }
}

/// Per-token pricing (per million tokens, in CNY).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
}

impl Default for Pricing {
    fn default() -> Self {
        // DeepSeek pricing as default
        Self {
            input_per_million: 0.5,
            output_per_million: 2.0,
            cache_read_per_million: 0.05,
            cache_write_per_million: 0.5,
        }
    }
}

impl Pricing {
    /// Anthropic Claude pricing.
    pub fn anthropic() -> Self {
        Self {
            input_per_million: 3.0,
            output_per_million: 15.0,
            cache_read_per_million: 0.3,
            cache_write_per_million: 3.75,
        }
    }

    /// OpenAI GPT-4o pricing.
    pub fn openai_gpt4o() -> Self {
        Self {
            input_per_million: 2.5,
            output_per_million: 10.0,
            cache_read_per_million: 1.25,
            cache_write_per_million: 2.5,
        }
    }
}

/// Per-turn usage record for cost tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnUsage {
    pub turn: usize,
    pub usage: TokenUsage,
    pub cost: f64,
    pub model: String,
}

/// Session cost tracker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostTracker {
    pub turns: Vec<TurnUsage>,
    pub pricing: Pricing,
    /// Maximum allowed cost for this session (0 = unlimited).
    #[serde(default)]
    pub budget: f64,
}

impl CostTracker {
    pub fn new(pricing: Pricing) -> Self {
        Self {
            turns: Vec::new(),
            pricing,
            budget: 0.0,
        }
    }

    /// Record a turn's usage.
    pub fn record_turn(&mut self, turn: usize, usage: TokenUsage, model: String) {
        let cost = usage.cost(&self.pricing);
        self.turns.push(TurnUsage { turn, usage, cost, model });
    }

    /// Total cost across all turns.
    pub fn total_cost(&self) -> f64 {
        self.turns.iter().map(|t| t.cost).sum()
    }

    /// Total usage across all turns.
    pub fn total_usage(&self) -> TokenUsage {
        let mut total = TokenUsage::default();
        for t in &self.turns {
            total.merge(&t.usage);
        }
        total
    }

    /// Total cache savings.
    pub fn total_cache_savings(&self) -> f64 {
        self.turns.iter().map(|t| t.usage.cache_savings(&self.pricing)).sum()
    }

    /// Set the session budget (0 = unlimited).
    pub fn set_budget(&mut self, budget: f64) {
        self.budget = budget;
    }

    /// Check if the budget is exceeded. Returns (exceeded, remaining).
    pub fn budget_status(&self) -> (bool, f64) {
        if self.budget <= 0.0 {
            return (false, f64::INFINITY);
        }
        let spent = self.total_cost();
        let remaining = self.budget - spent;
        (remaining <= 0.0, remaining)
    }

    /// Check if adding `estimated_cost` would exceed the budget.
    pub fn would_exceed_budget(&self, estimated_cost: f64) -> bool {
        if self.budget <= 0.0 {
            return false;
        }
        self.total_cost() + estimated_cost > self.budget
    }

    /// Budget usage as a percentage (0.0-1.0+). Returns 0 if no budget set.
    pub fn budget_usage_pct(&self) -> f64 {
        if self.budget <= 0.0 {
            return 0.0;
        }
        self.total_cost() / self.budget
    }
}

/// Agent lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Thinking,
    ExecutingTool,
    WaitingApproval,
    Responding,
    UpdatingMemory,
    Errored,
    Paused,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::Text("hello".into());
        assert_eq!(content.text(), Some("hello"));
    }

    #[test]
    fn test_message_content_parts() {
        let content = MessageContent::Parts(vec![ContentPart::Text { text: "hi".into() }]);
        assert_eq!(content.text(), None);
    }

    #[test]
    fn test_token_usage_merge() {
        let mut a = TokenUsage { prompt_tokens: 100, completion_tokens: 50, total_tokens: 150, cache_read_tokens: 10, cache_write_tokens: 5 };
        let b = TokenUsage { prompt_tokens: 200, completion_tokens: 80, total_tokens: 280, cache_read_tokens: 20, cache_write_tokens: 10 };
        a.merge(&b);
        assert_eq!(a.prompt_tokens, 300);
        assert_eq!(a.completion_tokens, 130);
        assert_eq!(a.total_tokens, 430);
        assert_eq!(a.cache_read_tokens, 30);
        assert_eq!(a.cache_write_tokens, 15);
    }

    #[test]
    fn test_token_usage_output_tokens() {
        let u = TokenUsage { prompt_tokens: 100, completion_tokens: 50, total_tokens: 150, cache_read_tokens: 0, cache_write_tokens: 0 };
        assert_eq!(u.output_tokens(), 50);
    }

    #[test]
    fn test_token_usage_cache_hit_rate() {
        let u = TokenUsage { prompt_tokens: 100, completion_tokens: 50, total_tokens: 150, cache_read_tokens: 30, cache_write_tokens: 0 };
        let rate = u.cache_hit_rate();
        assert!((rate - 30.0).abs() < 0.01);

        let zero = TokenUsage::default();
        assert_eq!(zero.cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_token_usage_cost() {
        let pricing = Pricing::default();
        let u = TokenUsage { prompt_tokens: 1_000_000, completion_tokens: 500_000, total_tokens: 1_500_000, cache_read_tokens: 0, cache_write_tokens: 0 };
        let cost = u.cost(&pricing);
        // input: 1M * 0.5/1M = 0.5, output: 500K * 2.0/1M = 1.0
        assert!((cost - 1.5).abs() < 0.01);
    }

    #[test]
    fn test_token_usage_cost_with_cache() {
        let pricing = Pricing::default();
        let u = TokenUsage { prompt_tokens: 1_000_000, completion_tokens: 0, total_tokens: 1_000_000, cache_read_tokens: 500_000, cache_write_tokens: 0 };
        let cost = u.cost(&pricing);
        // non-cached input: 500K * 0.5/1M = 0.25, cache read: 500K * 0.05/1M = 0.025
        assert!((cost - 0.275).abs() < 0.001);
    }

    #[test]
    fn test_cache_savings() {
        let pricing = Pricing::default();
        let u = TokenUsage { prompt_tokens: 1_000_000, completion_tokens: 0, total_tokens: 1_000_000, cache_read_tokens: 500_000, cache_write_tokens: 0 };
        let savings = u.cache_savings(&pricing);
        // 500K * (0.5 - 0.05) / 1M = 0.225
        assert!((savings - 0.225).abs() < 0.001);
    }

    #[test]
    fn test_cost_tracker_record_and_total() {
        let mut tracker = CostTracker::new(Pricing::default());
        let usage = TokenUsage { prompt_tokens: 1000, completion_tokens: 500, total_tokens: 1500, cache_read_tokens: 0, cache_write_tokens: 0 };
        tracker.record_turn(1, usage.clone(), "test".into());
        tracker.record_turn(2, usage, "test".into());
        assert!(tracker.total_cost() > 0.0);
        assert_eq!(tracker.total_usage().prompt_tokens, 2000);
        assert_eq!(tracker.turns.len(), 2);
    }

    #[test]
    fn test_cost_tracker_budget() {
        let mut tracker = CostTracker::new(Pricing::default());
        tracker.set_budget(1.0);
        let (exceeded, _) = tracker.budget_status();
        assert!(!exceeded);

        // Add a large usage
        let usage = TokenUsage { prompt_tokens: 10_000_000, completion_tokens: 10_000_000, total_tokens: 20_000_000, cache_read_tokens: 0, cache_write_tokens: 0 };
        tracker.record_turn(1, usage, "test".into());
        let (exceeded, remaining) = tracker.budget_status();
        assert!(exceeded);
        assert!(remaining < 0.0);
    }

    #[test]
    fn test_cost_tracker_would_exceed() {
        let mut tracker = CostTracker::new(Pricing::default());
        tracker.set_budget(1.0);
        assert!(!tracker.would_exceed_budget(0.5));
        assert!(tracker.would_exceed_budget(2.0));
    }

    #[test]
    fn test_cost_tracker_no_budget() {
        let tracker = CostTracker::new(Pricing::default());
        let (exceeded, remaining) = tracker.budget_status();
        assert!(!exceeded);
        assert_eq!(remaining, f64::INFINITY);
        assert!(!tracker.would_exceed_budget(999.0));
        assert_eq!(tracker.budget_usage_pct(), 0.0);
    }

    #[test]
    fn test_pricing_presets() {
        let anthropic = Pricing::anthropic();
        assert!(anthropic.input_per_million > 0.0);
        assert!(anthropic.output_per_million > anthropic.input_per_million);

        let openai = Pricing::openai_gpt4o();
        assert!(openai.input_per_million > 0.0);

        let default = Pricing::default();
        assert!(default.input_per_million > 0.0);
    }
}
