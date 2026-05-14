//! Cache statistics — track prompt cache hit rates and cost savings.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// A single cache event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,
}

/// Cache statistics tracker.
pub struct CacheStats {
    events: VecDeque<CacheEvent>,
    max_events: usize,
    /// Cost per million tokens for cache reads (typically 90% cheaper).
    cache_read_cost_per_million: f64,
    /// Cost per million tokens for regular input.
    input_cost_per_million: f64,
    /// Cost per million tokens for output.
    output_cost_per_million: f64,
}

impl CacheStats {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::new(),
            max_events,
            cache_read_cost_per_million: 0.03,  // DeepSeek cache read pricing
            input_cost_per_million: 0.27,        // DeepSeek input pricing
            output_cost_per_million: 1.10,       // DeepSeek output pricing
        }
    }

    /// Record a cache event.
    pub fn record(
        &mut self,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
        model: String,
    ) {
        self.events.push_back(CacheEvent {
            timestamp: chrono::Utc::now(),
            cache_read_tokens,
            cache_write_tokens,
            input_tokens,
            output_tokens,
            model,
        });

        while self.events.len() > self.max_events {
            self.events.pop_front();
        }
    }

    /// Get total cache read tokens.
    pub fn total_cache_read_tokens(&self) -> u64 {
        self.events.iter().map(|e| e.cache_read_tokens).sum()
    }

    /// Get total input tokens (excluding cache reads).
    pub fn total_input_tokens(&self) -> u64 {
        self.events.iter().map(|e| e.input_tokens).sum()
    }

    /// Get total output tokens.
    pub fn total_output_tokens(&self) -> u64 {
        self.events.iter().map(|e| e.output_tokens).sum()
    }

    /// Calculate cache hit rate.
    pub fn cache_hit_rate(&self) -> f64 {
        let total_input = self.total_input_tokens();
        let cache_reads = self.total_cache_read_tokens();
        if total_input + cache_reads == 0 {
            return 0.0;
        }
        cache_reads as f64 / (total_input + cache_reads) as f64
    }

    /// Calculate cost savings from cache.
    pub fn cost_savings(&self) -> f64 {
        let cache_reads = self.total_cache_read_tokens();
        // Without cache, these would be regular input tokens
        let full_cost = cache_reads as f64 * self.input_cost_per_million / 1_000_000.0;
        let actual_cost = cache_reads as f64 * self.cache_read_cost_per_million / 1_000_000.0;
        full_cost - actual_cost
    }

    /// Calculate total cost.
    pub fn total_cost(&self) -> f64 {
        let cache_reads = self.total_cache_read_tokens() as f64;
        let input = self.total_input_tokens() as f64;
        let output = self.total_output_tokens() as f64;

        (cache_reads * self.cache_read_cost_per_million
            + input * self.input_cost_per_million
            + output * self.output_cost_per_million)
            / 1_000_000.0
    }

    /// Get recent cache events.
    pub fn recent_events(&self, n: usize) -> Vec<&CacheEvent> {
        self.events.iter().rev().take(n).collect()
    }

    /// Format statistics for display.
    pub fn format_stats(&self) -> String {
        let total_events = self.events.len();
        let cache_reads = self.total_cache_read_tokens();
        let input = self.total_input_tokens();
        let output = self.total_output_tokens();
        let hit_rate = self.cache_hit_rate();
        let savings = self.cost_savings();
        let total = self.total_cost();

        let mut lines = vec![
            format!("Cache Statistics ({} requests):", total_events),
            "".to_string(),
            format!("  Cache read tokens:  {:>10}", Self::format_tokens(cache_reads)),
            format!("  Input tokens:       {:>10}", Self::format_tokens(input)),
            format!("  Output tokens:      {:>10}", Self::format_tokens(output)),
            "".to_string(),
            format!("  Cache hit rate:     {:>9.1}%", hit_rate * 100.0),
            format!("  Cost savings:       ${:.6}", savings),
            format!("  Total cost:         ${:.6}", total),
        ];

        // Show recent history
        if !self.events.is_empty() {
            lines.push("".to_string());
            lines.push("Recent requests:".to_string());
            let recent = self.recent_events(10);
            let mut history = String::from("  ");
            for event in recent.iter().rev() {
                let hit = if event.cache_read_tokens > 0 { "+" } else { "o" };
                history.push_str(hit);
            }
            lines.push(format!("  {} (+ = cache hit, o = miss)", history.trim()));
        }

        lines.join("\n")
    }

    fn format_tokens(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            format!("{}", n)
        }
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Show cache hit rate statistics and cost savings.
pub struct CacheStatsTool(pub Arc<Mutex<CacheStats>>);

#[async_trait]
impl Tool for CacheStatsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cache_stats".into(),
            description: "Show prompt cache hit rate, token savings, and cost reduction from caching.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let stats = self.0.lock().await;
        Ok(stats.format_stats())
    }
}

/// Record a cache event (for testing or manual entry).
pub struct CacheRecordTool(pub Arc<Mutex<CacheStats>>);

#[async_trait]
impl Tool for CacheRecordTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cache_record".into(),
            description: "Record a cache event with token counts. Used for tracking cache performance.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cache_read_tokens": { "type": "integer", "description": "Tokens read from cache" },
                    "input_tokens": { "type": "integer", "description": "Regular input tokens" },
                    "output_tokens": { "type": "integer", "description": "Output tokens" }
                },
                "required": ["input_tokens", "output_tokens"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let cache_read = args["cache_read_tokens"].as_u64().unwrap_or(0);
        let input = args["input_tokens"]
            .as_u64()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'input_tokens'".into()))?;
        let output = args["output_tokens"]
            .as_u64()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'output_tokens'".into()))?;

        let mut stats = self.0.lock().await;
        stats.record(cache_read, 0, input, output, "manual".into());

        Ok(format!(
            "Recorded: cache_read={}, input={}, output={}",
            cache_read, input, output
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_rate() {
        let mut stats = CacheStats::new(100);
        stats.record(1000, 0, 500, 200, "test".into());
        stats.record(2000, 0, 300, 100, "test".into());

        let hit_rate = stats.cache_hit_rate();
        assert!(hit_rate > 0.7); // 3000 cache / (3000 + 800 total) ≈ 79%
    }

    #[test]
    fn test_cost_savings() {
        let mut stats = CacheStats::new(100);
        stats.record(10000, 0, 1000, 500, "test".into());

        let savings = stats.cost_savings();
        assert!(savings > 0.0); // Cache reads are cheaper
    }

    #[test]
    fn test_max_events() {
        let mut stats = CacheStats::new(3);
        for i in 0..5 {
            stats.record(i * 100, 0, 100, 50, "test".into());
        }
        assert_eq!(stats.events.len(), 3);
    }

    #[test]
    fn test_format_stats() {
        let mut stats = CacheStats::new(100);
        stats.record(1000, 0, 500, 200, "test".into());
        let formatted = stats.format_stats();
        assert!(formatted.contains("Cache Statistics"));
        assert!(formatted.contains("hit rate"));
    }
}
