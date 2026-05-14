//! Token counting and cost estimation for LLM requests.
//!
//! Provides approximate token counting without external dependencies,
//! using character-based heuristics calibrated per model family.

use std::collections::HashMap;

/// Model family for token estimation calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFamily {
    /// GPT-4, GPT-3.5, and compatible models (~4 chars/token).
    OpenAI,
    /// Claude models (~3.5 chars/token).
    Anthropic,
    /// Generic fallback (~4 chars/token).
    Generic,
}

impl ModelFamily {
    /// Detect model family from model ID string.
    pub fn from_model(model: &str) -> Self {
        let lower = model.to_lowercase();
        if lower.starts_with("gpt") || lower.starts_with("o1") || lower.starts_with("o3") {
            Self::OpenAI
        } else if lower.starts_with("claude") {
            Self::Anthropic
        } else {
            Self::Generic
        }
    }

    /// Average characters per token for this family.
    fn chars_per_token(&self) -> f32 {
        match self {
            Self::OpenAI => 4.0,
            Self::Anthropic => 3.5,
            Self::Generic => 4.0,
        }
    }
}

/// Pricing profile for a model.
#[derive(Debug, Clone)]
pub struct PricingProfile {
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
    pub context_window: u64,
}

impl PricingProfile {
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = input_tokens as f64 / 1000.0 * self.input_cost_per_1k;
        let output_cost = output_tokens as f64 / 1000.0 * self.output_cost_per_1k;
        input_cost + output_cost
    }
}

/// Token count breakdown.
#[derive(Debug, Clone, Default)]
pub struct TokenCount {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Cumulative token statistics.
#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub total_input: u64,
    pub total_output: u64,
    pub request_count: u64,
    pub total_cost: f64,
}

impl TokenStats {
    pub fn record(&mut self, count: &TokenCount, cost: f64) {
        self.total_input += count.input_tokens;
        self.total_output += count.output_tokens;
        self.request_count += 1;
        self.total_cost += cost;
    }

    pub fn format_stats(&self) -> String {
        format!(
            "Requests: {} | Input: {} | Output: {} | Cost: ${:.4}",
            self.request_count, self.total_input, self.total_output, self.total_cost
        )
    }
}

/// Token counter with model-aware estimation.
pub struct TokenCounter {
    family: ModelFamily,
    pricing: HashMap<String, PricingProfile>,
    stats: TokenStats,
}

impl TokenCounter {
    pub fn new(family: ModelFamily) -> Self {
        let mut pricing = HashMap::new();

        // OpenAI models
        pricing.insert("gpt-4o".to_string(), PricingProfile {
            input_cost_per_1k: 0.0025,
            output_cost_per_1k: 0.01,
            context_window: 128_000,
        });
        pricing.insert("gpt-4o-mini".to_string(), PricingProfile {
            input_cost_per_1k: 0.00015,
            output_cost_per_1k: 0.0006,
            context_window: 128_000,
        });
        pricing.insert("gpt-4-turbo".to_string(), PricingProfile {
            input_cost_per_1k: 0.01,
            output_cost_per_1k: 0.03,
            context_window: 128_000,
        });

        // Anthropic models
        pricing.insert("claude-sonnet-4-20250514".to_string(), PricingProfile {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            context_window: 200_000,
        });
        pricing.insert("claude-haiku-35".to_string(), PricingProfile {
            input_cost_per_1k: 0.0008,
            output_cost_per_1k: 0.004,
            context_window: 200_000,
        });

        Self {
            family,
            pricing,
            stats: TokenStats::default(),
        }
    }

    /// Count tokens in a text string.
    pub fn count_text(&self, text: &str) -> u64 {
        let cpt = self.family.chars_per_token();
        // Account for special tokens, newlines, etc.
        let base = text.len() as f32 / cpt;
        // Add overhead for special tokens (~4 per message boundary)
        (base + 4.0).ceil() as u64
    }

    /// Count tokens in a list of messages (text parts only).
    pub fn count_messages(&self, texts: &[&str]) -> u64 {
        // Each message has ~4 tokens overhead (role, separators)
        let content_tokens: u64 = texts.iter().map(|t| self.count_text(t)).sum();
        content_tokens + (texts.len() as u64 * 4)
    }

    /// Estimate tokens for a request with input and expected output.
    pub fn estimate_request(&self, input_text: &str, expected_output_len: usize) -> TokenCount {
        let input_tokens = self.count_text(input_text);
        let output_tokens = self.count_text(&"x".repeat(expected_output_len));
        TokenCount {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        }
    }

    /// Calculate cost for a token count using a named model's pricing.
    pub fn calculate_cost(&self, model: &str, count: &TokenCount) -> f64 {
        if let Some(profile) = self.pricing.get(model) {
            profile.calculate_cost(count.input_tokens, count.output_tokens)
        } else {
            // Default estimate: $0.01 per 1K input, $0.03 per 1K output
            count.input_tokens as f64 / 1000.0 * 0.01 + count.output_tokens as f64 / 1000.0 * 0.03
        }
    }

    /// Get pricing profile for a model.
    pub fn pricing(&self, model: &str) -> Option<&PricingProfile> {
        self.pricing.get(model)
    }

    /// Get cumulative stats.
    pub fn stats(&self) -> &TokenStats {
        &self.stats
    }

    /// Get mutable stats for recording.
    pub fn stats_mut(&mut self) -> &mut TokenStats {
        &mut self.stats
    }

    /// Check if a request fits within a model's context window.
    pub fn fits_context(&self, model: &str, input_tokens: u64, max_output: u64) -> bool {
        if let Some(profile) = self.pricing.get(model) {
            input_tokens + max_output <= profile.context_window
        } else {
            true
        }
    }
}

/// Live token tracker — accumulates across a session.
pub struct LiveTokenTracker {
    counter: TokenCounter,
    session_input: u64,
    session_output: u64,
    session_cost: f64,
}

impl LiveTokenTracker {
    pub fn new(family: ModelFamily) -> Self {
        Self {
            counter: TokenCounter::new(family),
            session_input: 0,
            session_output: 0,
            session_cost: 0.0,
        }
    }

    /// Record a completed request.
    pub fn record(&mut self, model: &str, input_tokens: u64, output_tokens: u64) {
        let count = TokenCount {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        };
        let cost = self.counter.calculate_cost(model, &count);
        self.counter.stats_mut().record(&count, cost);
        self.session_input += input_tokens;
        self.session_output += output_tokens;
        self.session_cost += cost;
    }

    pub fn session_tokens(&self) -> u64 {
        self.session_input + self.session_output
    }

    pub fn session_cost(&self) -> f64 {
        self.session_cost
    }

    pub fn format_session(&self) -> String {
        format!(
            "Session: {} in/{} out | ${:.4}",
            self.session_input, self.session_output, self.session_cost
        )
    }

    pub fn counter(&self) -> &TokenCounter {
        &self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_family_detection() {
        assert_eq!(ModelFamily::from_model("gpt-4o"), ModelFamily::OpenAI);
        assert_eq!(ModelFamily::from_model("gpt-3.5-turbo"), ModelFamily::OpenAI);
        assert_eq!(ModelFamily::from_model("o3-mini"), ModelFamily::OpenAI);
        assert_eq!(ModelFamily::from_model("claude-sonnet-4-20250514"), ModelFamily::Anthropic);
        assert_eq!(ModelFamily::from_model("llama-3"), ModelFamily::Generic);
    }

    #[test]
    fn test_count_text() {
        let counter = TokenCounter::new(ModelFamily::OpenAI);
        let tokens = counter.count_text("Hello, world!");
        assert!(tokens > 0);
        assert!(tokens < 10);
    }

    #[test]
    fn test_count_messages() {
        let counter = TokenCounter::new(ModelFamily::OpenAI);
        let tokens = counter.count_messages(&["Hello", "World", "How are you?"]);
        assert!(tokens > 10);
    }

    #[test]
    fn test_estimate_request() {
        let counter = TokenCounter::new(ModelFamily::OpenAI);
        let count = counter.estimate_request("What is Rust?", 200);
        assert!(count.input_tokens > 0);
        assert!(count.output_tokens > 0);
        assert_eq!(count.total_tokens, count.input_tokens + count.output_tokens);
    }

    #[test]
    fn test_calculate_cost() {
        let counter = TokenCounter::new(ModelFamily::OpenAI);
        let count = TokenCount {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
        };
        let cost = counter.calculate_cost("gpt-4o", &count);
        assert!(cost > 0.0);
        // 1000/1000 * 0.0025 + 500/1000 * 0.01 = 0.0025 + 0.005 = 0.0075
        assert!((cost - 0.0075).abs() < 0.001);
    }

    #[test]
    fn test_fits_context() {
        let counter = TokenCounter::new(ModelFamily::OpenAI);
        assert!(counter.fits_context("gpt-4o", 1000, 4000));
        assert!(!counter.fits_context("gpt-4o", 127_000, 2000));
    }

    #[test]
    fn test_token_stats() {
        let mut stats = TokenStats::default();
        let count = TokenCount {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        };
        stats.record(&count, 0.01);
        assert_eq!(stats.request_count, 1);
        assert_eq!(stats.total_input, 100);
        assert!(stats.total_cost > 0.0);
    }

    #[test]
    fn test_live_tracker() {
        let mut tracker = LiveTokenTracker::new(ModelFamily::OpenAI);
        tracker.record("gpt-4o", 1000, 500);
        assert_eq!(tracker.session_tokens(), 1500);
        assert!(tracker.session_cost() > 0.0);
    }

    #[test]
    fn test_format_stats() {
        let stats = TokenStats {
            total_input: 1000,
            total_output: 500,
            request_count: 5,
            total_cost: 0.05,
        };
        let s = stats.format_stats();
        assert!(s.contains("5"));
        assert!(s.contains("1000"));
    }

    #[test]
    fn test_pricing_profile() {
        let profile = PricingProfile {
            input_cost_per_1k: 0.01,
            output_cost_per_1k: 0.03,
            context_window: 128_000,
        };
        let cost = profile.calculate_cost(2000, 1000);
        // 2000/1000 * 0.01 + 1000/1000 * 0.03 = 0.02 + 0.03 = 0.05
        assert!((cost - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_format_session() {
        let mut tracker = LiveTokenTracker::new(ModelFamily::OpenAI);
        tracker.record("gpt-4o", 500, 200);
        let s = tracker.format_session();
        assert!(s.contains("500 in"));
        assert!(s.contains("200 out"));
    }
}
