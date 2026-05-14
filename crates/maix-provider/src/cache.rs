//! Prompt cache management — tracking, optimization, and statistics.
//!
//! Monitors cache utilization across API calls to reduce costs.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cache prefix strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixStrategy {
    /// Cache only the system prompt.
    SystemPrompt,
    /// Cache system prompt + tool definitions.
    SystemAndTools,
    /// Cache full context (if provider supports it).
    FullContext,
}

impl PrefixStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SystemPrompt => "system_prompt",
            Self::SystemAndTools => "system_and_tools",
            Self::FullContext => "full_context",
        }
    }
}

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub enabled: bool,
    pub prefix_strategy: PrefixStrategy,
    pub max_prefix_tokens: u64,
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix_strategy: PrefixStrategy::SystemAndTools,
            max_prefix_tokens: 100_000,
            ttl: Duration::from_secs(300),
        }
    }
}

/// A cached prefix with metadata.
#[derive(Debug, Clone)]
pub struct CachedPrefix {
    pub content_hash: u64,
    pub tokens: u64,
    pub last_used: Instant,
    pub hit_count: u64,
    pub cost_saved: f64,
}

/// Cache statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub tokens_saved: u64,
    pub cost_saved: f64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f32 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.cache_hits as f32 / self.total_requests as f32
    }

    pub fn format_stats(&self) -> String {
        format!(
            "Cache: {:.1}% hit | {} tokens saved | ${:.4} saved",
            self.hit_rate() * 100.0,
            self.tokens_saved,
            self.cost_saved
        )
    }

    pub fn format_bar(&self, width: usize) -> String {
        if self.total_requests == 0 {
            return "○".repeat(width);
        }
        let recent_hits = self.cache_hits.min(width as u64);
        let recent_misses = self.cache_misses.min(width as u64 - recent_hits);
        let total = recent_hits + recent_misses;
        if total == 0 {
            return "○".repeat(width);
        }
        let hit_chars = (recent_hits as f64 / total as f64 * width as f64) as usize;
        format!(
            "{}{}",
            "●".repeat(hit_chars),
            "○".repeat(width - hit_chars)
        )
    }
}

/// Record of a single API request for cache analysis.
#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub timestamp: Instant,
    pub system_prompt_hash: u64,
    pub tools_hash: u64,
    pub cache_hit: bool,
    pub cache_tokens: u64,
    pub total_tokens: u64,
}

/// Efficiency report from cache analysis.
#[derive(Debug, Clone)]
pub struct CacheEfficiencyReport {
    pub hit_rate: f32,
    pub prefix_stability: f32,
    pub estimated_savings: f64,
    pub recommendations: Vec<String>,
}

/// Cache optimizer — analyzes patterns and generates recommendations.
pub struct CacheOptimizer {
    history: Vec<RequestRecord>,
    max_history: usize,
}

impl CacheOptimizer {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            max_history,
        }
    }

    pub fn record(&mut self, record: RequestRecord) {
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(record);
    }

    pub fn analyze_efficiency(&self) -> CacheEfficiencyReport {
        let total = self.history.len();
        if total == 0 {
            return CacheEfficiencyReport {
                hit_rate: 0.0,
                prefix_stability: 1.0,
                estimated_savings: 0.0,
                recommendations: vec!["No request history yet.".to_string()],
            };
        }

        let hits = self.history.iter().filter(|r| r.cache_hit).count();
        let hit_rate = hits as f32 / total as f32;

        // Prefix stability: how often the system prompt hash stays the same
        let prefix_changes = self.count_prefix_changes();
        let prefix_stability = 1.0 - (prefix_changes as f32 / total as f32);

        let estimated_savings = self.calculate_savings();
        let recommendations = self.generate_recommendations(hit_rate, prefix_stability);

        CacheEfficiencyReport {
            hit_rate,
            prefix_stability,
            estimated_savings,
            recommendations,
        }
    }

    fn count_prefix_changes(&self) -> usize {
        let mut changes = 0;
        for i in 1..self.history.len() {
            if self.history[i].system_prompt_hash != self.history[i - 1].system_prompt_hash {
                changes += 1;
            }
        }
        changes
    }

    fn calculate_savings(&self) -> f64 {
        // Estimate: cached tokens cost ~50% less (read vs write)
        let cached_tokens: u64 = self.history.iter().filter(|r| r.cache_hit).map(|r| r.cache_tokens).sum();
        cached_tokens as f64 * 0.003 / 1000.0 * 0.5
    }

    fn generate_recommendations(&self, hit_rate: f32, prefix_stability: f32) -> Vec<String> {
        let mut recs = Vec::new();

        if hit_rate < 0.5 {
            recs.push("Cache hit rate is low. Keep system prompt stable to improve caching.".to_string());
        }

        if prefix_stability < 0.5 {
            recs.push("System prompt changes frequently. Move dynamic content to user messages.".to_string());
        }

        if self.history.len() > 10 && hit_rate > 0.8 {
            recs.push("Excellent cache performance. Current strategy is working well.".to_string());
        }

        if recs.is_empty() {
            recs.push("Cache performance is normal.".to_string());
        }

        recs
    }

    pub fn history(&self) -> &[RequestRecord] {
        &self.history
    }
}

/// Prompt cache manager — central cache tracking and optimization.
pub struct PromptCacheManager {
    config: CacheConfig,
    stats: CacheStats,
    optimizer: CacheOptimizer,
    prefix_cache: HashMap<u64, CachedPrefix>,
}

impl PromptCacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            stats: CacheStats::default(),
            optimizer: CacheOptimizer::new(200),
            prefix_cache: HashMap::new(),
        }
    }

    /// Record a completed API request's cache performance.
    pub fn record_request(
        &mut self,
        system_prompt_hash: u64,
        tools_hash: u64,
        cache_hit: bool,
        cache_tokens: u64,
        total_tokens: u64,
    ) {
        self.stats.total_requests += 1;

        if cache_hit && cache_tokens > 0 {
            self.stats.cache_hits += 1;
            self.stats.tokens_saved += cache_tokens;
            // Cost saved: cached tokens are ~50% cheaper
            self.stats.cost_saved += cache_tokens as f64 * 0.003 / 1000.0 * 0.5;

            // Update prefix cache
            let entry = self.prefix_cache.entry(system_prompt_hash).or_insert_with(|| CachedPrefix {
                content_hash: system_prompt_hash,
                tokens: cache_tokens,
                last_used: Instant::now(),
                hit_count: 0,
                cost_saved: 0.0,
            });
            entry.hit_count += 1;
            entry.last_used = Instant::now();
            entry.cost_saved += cache_tokens as f64 * 0.003 / 1000.0 * 0.5;
        } else {
            self.stats.cache_misses += 1;
        }

        self.optimizer.record(RequestRecord {
            timestamp: Instant::now(),
            system_prompt_hash,
            tools_hash,
            cache_hit,
            cache_tokens,
            total_tokens,
        });
    }

    /// Get cache statistics.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get the cache configuration.
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Get the optimizer.
    pub fn optimizer(&self) -> &CacheOptimizer {
        &self.optimizer
    }

    /// Get prefix strategy.
    pub fn strategy(&self) -> PrefixStrategy {
        self.config.prefix_strategy
    }

    /// Format a compact status line for the TUI status bar.
    pub fn format_status(&self) -> String {
        if !self.config.enabled {
            return "cache: off".to_string();
        }
        format!(
            "cache: {:.0}% | saved: {} tok | ${:.3}",
            self.stats.hit_rate() * 100.0,
            self.stats.tokens_saved,
            self.stats.cost_saved
        )
    }

    /// Format a detailed view.
    pub fn format_detailed(&self) -> String {
        let mut lines = vec![
            format!("Strategy: {}", self.config.prefix_strategy.as_str()),
            self.stats.format_stats(),
            String::new(),
            format!("Recent: {}", self.stats.format_bar(20)),
        ];

        let report = self.optimizer.analyze_efficiency();
        if !report.recommendations.is_empty() {
            lines.push(String::new());
            lines.push("Recommendations:".to_string());
            for rec in &report.recommendations {
                lines.push(format!("  - {}", rec));
            }
        }

        lines.join("\n")
    }
}

/// Simple hash for content comparison.
pub fn content_hash(text: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in text.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats_hit_rate() {
        let stats = CacheStats {
            total_requests: 10,
            cache_hits: 7,
            cache_misses: 3,
            tokens_saved: 1000,
            cost_saved: 0.01,
        };
        assert!((stats.hit_rate() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_zero_requests() {
        let stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_format_stats() {
        let stats = CacheStats {
            total_requests: 10,
            cache_hits: 7,
            cache_misses: 3,
            tokens_saved: 1000,
            cost_saved: 0.05,
        };
        let s = stats.format_stats();
        assert!(s.contains("70.0%"));
        assert!(s.contains("1000"));
    }

    #[test]
    fn test_format_bar() {
        let stats = CacheStats {
            total_requests: 10,
            cache_hits: 7,
            cache_misses: 3,
            tokens_saved: 1000,
            cost_saved: 0.05,
        };
        let bar = stats.format_bar(10);
        assert!(bar.contains('●'));
        assert!(bar.contains('○'));
    }

    #[test]
    fn test_cache_optimizer_empty() {
        let optimizer = CacheOptimizer::new(100);
        let report = optimizer.analyze_efficiency();
        assert_eq!(report.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_optimizer_with_history() {
        let mut optimizer = CacheOptimizer::new(100);
        for i in 0..10 {
            optimizer.record(RequestRecord {
                timestamp: Instant::now(),
                system_prompt_hash: 12345,
                tools_hash: 67890,
                cache_hit: i % 3 != 0,
                cache_tokens: 500,
                total_tokens: 2000,
            });
        }
        let report = optimizer.analyze_efficiency();
        assert!(report.hit_rate > 0.5);
        assert!(report.prefix_stability > 0.9);
    }

    #[test]
    fn test_prompt_cache_manager() {
        let mut mgr = PromptCacheManager::new(CacheConfig::default());
        mgr.record_request(123, 456, true, 1000, 3000);
        mgr.record_request(123, 456, false, 0, 3000);
        mgr.record_request(123, 456, true, 800, 3000);

        assert_eq!(mgr.stats().total_requests, 3);
        assert_eq!(mgr.stats().cache_hits, 2);
        assert_eq!(mgr.stats().cache_misses, 1);
        assert!(mgr.stats().tokens_saved > 0);
    }

    #[test]
    fn test_format_status() {
        let mut mgr = PromptCacheManager::new(CacheConfig::default());
        mgr.record_request(123, 456, true, 1000, 3000);
        let status = mgr.format_status();
        assert!(status.contains("cache:"));
        assert!(status.contains("saved:"));
    }

    #[test]
    fn test_format_status_disabled() {
        let config = CacheConfig {
            enabled: false,
            ..Default::default()
        };
        let mgr = PromptCacheManager::new(config);
        assert_eq!(mgr.format_status(), "cache: off");
    }

    #[test]
    fn test_format_detailed() {
        let mut mgr = PromptCacheManager::new(CacheConfig::default());
        mgr.record_request(123, 456, true, 1000, 3000);
        let detail = mgr.format_detailed();
        assert!(detail.contains("Strategy:"));
        assert!(detail.contains("Recent:"));
    }

    #[test]
    fn test_content_hash() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        let h3 = content_hash("hello worle");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_prefix_strategy_as_str() {
        assert_eq!(PrefixStrategy::SystemPrompt.as_str(), "system_prompt");
        assert_eq!(PrefixStrategy::SystemAndTools.as_str(), "system_and_tools");
        assert_eq!(PrefixStrategy::FullContext.as_str(), "full_context");
    }

    #[test]
    fn test_optimizer_recommendations() {
        let mut optimizer = CacheOptimizer::new(100);
        // Low hit rate
        for i in 0..10 {
            optimizer.record(RequestRecord {
                timestamp: Instant::now(),
                system_prompt_hash: i,
                tools_hash: 0,
                cache_hit: false,
                cache_tokens: 0,
                total_tokens: 1000,
            });
        }
        let report = optimizer.analyze_efficiency();
        assert!(report.recommendations.iter().any(|r| r.contains("low")));
    }
}
