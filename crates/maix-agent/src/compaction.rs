//! Smart context compaction — strategy-based compression with quality tracking.
//!
//! Applies multiple compaction strategies in priority order:
//! 1. ToolResultSummarizer — truncate long tool outputs
//! 2. DuplicateRemover — deduplicate repeated tool calls
//! 3. ContextPruner — remove low-importance messages
//! 4. SemanticCompressor — LLM-based summarization (last resort)

use maix_core::{Message, MessageContent, Role};
use std::collections::HashMap;

/// Result of a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactedResult {
    pub messages: Vec<Message>,
    pub original_tokens: u64,
    pub compacted_tokens: u64,
    pub quality_score: f32,
    pub strategy_used: String,
}

impl CompactedResult {
    pub fn tokens_saved(&self) -> u64 {
        self.original_tokens.saturating_sub(self.compacted_tokens)
    }

    pub fn compression_ratio(&self) -> f32 {
        if self.original_tokens == 0 {
            return 1.0;
        }
        self.compacted_tokens as f32 / self.original_tokens as f32
    }
}

/// Statistics across all compaction operations.
#[derive(Debug, Clone, Default)]
pub struct CompactionStats {
    pub total_compactions: u32,
    pub tokens_saved: u64,
    pub average_quality: f32,
    pub strategy_usage: HashMap<String, u32>,
}

impl CompactionStats {
    pub fn record(&mut self, result: &CompactedResult) {
        self.total_compactions += 1;
        self.tokens_saved += result.tokens_saved();

        // Running average quality
        let n = self.total_compactions as f32;
        self.average_quality = self.average_quality * (n - 1.0) / n + result.quality_score / n;

        *self.strategy_usage.entry(result.strategy_used.clone()).or_insert(0) += 1;
    }

    pub fn format_stats(&self) -> String {
        if self.total_compactions == 0 {
            return "No compactions yet.".to_string();
        }
        format!(
            "Compacted {} times | Saved ~{} tokens | Avg quality: {:.0}%",
            self.total_compactions,
            self.tokens_saved,
            self.average_quality * 100.0
        )
    }
}

/// A compaction strategy that can be applied to messages.
pub trait CompactionStrategy: Send + Sync {
    fn name(&self) -> &str;
    fn can_apply(&self, messages: &[Message]) -> bool;
    fn compact(&self, messages: &[Message]) -> CompactedResult;
}

// ---------------------------------------------------------------------------
// Strategy 1: ToolResultSummarizer
// ---------------------------------------------------------------------------

/// Truncates tool outputs longer than a threshold, keeping first and last N lines.
pub struct ToolResultSummarizer {
    /// Maximum characters before truncation.
    pub max_chars: usize,
    /// Lines to keep at start and end.
    pub keep_lines: usize,
}

impl Default for ToolResultSummarizer {
    fn default() -> Self {
        Self {
            max_chars: 500,
            keep_lines: 5,
        }
    }
}

impl CompactionStrategy for ToolResultSummarizer {
    fn name(&self) -> &str {
        "tool_result_summarizer"
    }

    fn can_apply(&self, messages: &[Message]) -> bool {
        messages.iter().any(|m| {
            m.role == Role::Tool
                && m.content.text().map(|t| t.len() > self.max_chars).unwrap_or(false)
        })
    }

    fn compact(&self, messages: &[Message]) -> CompactedResult {
        let original_tokens = crate::context::estimate_tokens(messages);

        let compacted: Vec<Message> = messages
            .iter()
            .map(|m| {
                if m.role != Role::Tool {
                    return m.clone();
                }
                let text = match m.content.text() {
                    Some(t) => t,
                    None => return m.clone(),
                };
                if text.len() <= self.max_chars {
                    return m.clone();
                }

                // Truncate: keep first N and last N lines
                let lines: Vec<&str> = text.lines().collect();
                let truncated = if lines.len() <= self.keep_lines * 2 {
                    text.to_string()
                } else {
                    let first: Vec<&str> = lines[..self.keep_lines].to_vec();
                    let last: Vec<&str> = lines[lines.len() - self.keep_lines..].to_vec();
                    let skipped = lines.len() - self.keep_lines * 2;
                    format!(
                        "{}\n... ({} lines omitted) ...\n{}",
                        first.join("\n"),
                        skipped,
                        last.join("\n")
                    )
                };

                Message {
                    role: m.role.clone(),
                    content: MessageContent::Text(truncated),
                    name: m.name.clone(),
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls: m.tool_calls.clone(),
                    reasoning_content: m.reasoning_content.clone(),
                }
            })
            .collect();

        let compacted_tokens = crate::context::estimate_tokens(&compacted);
        CompactedResult {
            messages: compacted,
            original_tokens,
            compacted_tokens,
            quality_score: 0.9,
            strategy_used: self.name().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Strategy 2: DuplicateRemover
// ---------------------------------------------------------------------------

/// Removes duplicate tool call/result pairs (same tool, same args, keeps last).
pub struct DuplicateRemover;

impl CompactionStrategy for DuplicateRemover {
    fn name(&self) -> &str {
        "duplicate_remover"
    }

    fn can_apply(&self, messages: &[Message]) -> bool {
        // Check if there are repeated tool names
        let tool_names: Vec<&str> = messages
            .iter()
            .filter_map(|m| m.tool_calls.as_ref())
            .flat_map(|tcs| tcs.iter().map(|tc| tc.function.name.as_str()))
            .collect();
        if tool_names.len() < 2 {
            return false;
        }
        let mut seen = std::collections::HashSet::new();
        for name in &tool_names {
            if !seen.insert(name) {
                return true;
            }
        }
        false
    }

    fn compact(&self, messages: &[Message]) -> CompactedResult {
        let original_tokens = crate::context::estimate_tokens(messages);

        // Track seen (tool_name, args_hash) pairs; keep only the last occurrence
        let mut seen_calls: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, m) in messages.iter().enumerate() {
            if let Some(ref tcs) = m.tool_calls {
                for tc in tcs {
                    let key = format!("{}:{}", tc.function.name, tc.function.arguments);
                    seen_calls.entry(key).or_default().push(i);
                }
            }
        }

        // Mark indices to remove (all but last of each duplicate)
        let mut remove_indices = std::collections::HashSet::new();
        for indices in seen_calls.values() {
            if indices.len() > 1 {
                // Keep last, remove rest
                for &idx in &indices[..indices.len() - 1] {
                    remove_indices.insert(idx);
                }
                // Also remove corresponding tool results
                for &idx in &indices[..indices.len() - 1] {
                    // Find tool result messages that follow this assistant message
                    for (j, m) in messages.iter().enumerate() {
                        if j > idx && m.role == Role::Tool {
                            // Check if this tool result corresponds to the removed call
                            if let Some(ref tcs) = messages[idx].tool_calls {
                                for tc in tcs {
                                    if m.tool_call_id.as_deref() == Some(&tc.id) {
                                        remove_indices.insert(j);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let compacted: Vec<Message> = messages
            .iter()
            .enumerate()
            .filter(|(i, _)| !remove_indices.contains(i))
            .map(|(_, m)| m.clone())
            .collect();

        let compacted_tokens = crate::context::estimate_tokens(&compacted);
        CompactedResult {
            messages: compacted,
            original_tokens,
            compacted_tokens,
            quality_score: 0.95,
            strategy_used: self.name().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Strategy 3: ContextPruner
// ---------------------------------------------------------------------------

/// Removes low-importance messages based on scoring, preserving system and recent messages.
pub struct ContextPruner {
    /// Maximum tokens to target after pruning.
    pub target_tokens: usize,
}

impl ContextPruner {
    pub fn new(target_tokens: usize) -> Self {
        Self { target_tokens }
    }
}

impl CompactionStrategy for ContextPruner {
    fn name(&self) -> &str {
        "context_pruner"
    }

    fn can_apply(&self, messages: &[Message]) -> bool {
        let current = crate::context::estimate_tokens(messages) as usize;
        current > self.target_tokens
    }

    fn compact(&self, messages: &[Message]) -> CompactedResult {
        let original_tokens = crate::context::estimate_tokens(messages);
        let weights = crate::context::ImportanceWeights::default();
        let compacted = crate::context::smart_truncate(messages, self.target_tokens, &weights);
        let compacted_tokens = crate::context::estimate_tokens(&compacted);

        CompactedResult {
            messages: compacted,
            original_tokens,
            compacted_tokens,
            quality_score: 0.8,
            strategy_used: self.name().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// SmartCompactor — orchestrates strategies
// ---------------------------------------------------------------------------

/// Smart compactor that applies multiple strategies in priority order.
pub struct SmartCompactor {
    strategies: Vec<Box<dyn CompactionStrategy>>,
    stats: CompactionStats,
    /// Context window size in tokens.
    context_window: u64,
}

impl SmartCompactor {
    pub fn new(context_window: u64) -> Self {
        let target = (context_window as f64 * 0.6) as usize;
        Self {
            strategies: vec![
                Box::new(ToolResultSummarizer::default()),
                Box::new(DuplicateRemover),
                Box::new(ContextPruner::new(target)),
            ],
            stats: CompactionStats::default(),
            context_window,
        }
    }

    /// Check if compaction should be triggered.
    pub fn should_compact(&self, messages: &[Message]) -> bool {
        let current_tokens = crate::context::estimate_tokens(messages);
        let usage = current_tokens as f64 / self.context_window as f64;

        // 85% threshold
        if usage > 0.85 {
            return true;
        }

        // 200+ messages
        if messages.len() > 200 {
            return true;
        }

        // Tool output accumulation > 25% of context
        let tool_tokens: u64 = messages
            .iter()
            .filter(|m| m.role == Role::Tool)
            .map(|m| {
                let text_len = m.content.text().unwrap_or("").len() as u64;
                text_len / 4
            })
            .sum();
        if tool_tokens > self.context_window / 4 {
            return true;
        }

        false
    }

    /// Run compaction using the first applicable strategy.
    pub fn compact(&mut self, messages: &[Message]) -> Option<CompactedResult> {
        for strategy in &self.strategies {
            if strategy.can_apply(messages) {
                let mut result = strategy.compact(messages);

                // Chain: try additional strategies on the result
                for next_strategy in &self.strategies {
                    if next_strategy.name() != strategy.name()
                        && next_strategy.can_apply(&result.messages)
                    {
                        let next_result = next_strategy.compact(&result.messages);
                        if next_result.compacted_tokens < result.compacted_tokens {
                            result = CompactedResult {
                                messages: next_result.messages,
                                original_tokens: result.original_tokens,
                                compacted_tokens: next_result.compacted_tokens,
                                quality_score: (result.quality_score + next_result.quality_score) / 2.0,
                                strategy_used: format!("{}+{}", strategy.name(), next_strategy.name()),
                            };
                        }
                    }
                }

                self.stats.record(&result);
                return Some(result);
            }
        }
        None
    }

    /// Get compaction statistics.
    pub fn stats(&self) -> &CompactionStats {
        &self.stats
    }

    /// Get current context utilization percentage.
    pub fn utilization(&self, messages: &[Message]) -> f32 {
        let tokens = crate::context::estimate_tokens(messages);
        crate::context::context_utilization(tokens, self.context_window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maix_core::{FunctionCall, MessageContent, ToolCall};

    fn make_msg(role: Role, text: &str) -> Message {
        Message {
            role,
            content: MessageContent::Text(text.into()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }

    fn make_tool_call_msg(tool_name: &str, args: &str, call_id: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: MessageContent::Text("".into()),
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: call_id.to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name.to_string(),
                    arguments: args.to_string(),
                },
            }]),
            reasoning_content: None,
        }
    }

    fn make_tool_result(call_id: &str, text: &str) -> Message {
        Message {
            role: Role::Tool,
            content: MessageContent::Text(text.into()),
            name: None,
            tool_call_id: Some(call_id.to_string()),
            tool_calls: None,
            reasoning_content: None,
        }
    }

    #[test]
    fn test_tool_result_summarizer_truncates() {
        let summarizer = ToolResultSummarizer {
            max_chars: 50,
            keep_lines: 2,
        };
        let long_text = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10";
        let messages = vec![make_msg(Role::Tool, long_text)];
        assert!(summarizer.can_apply(&messages));

        let result = summarizer.compact(&messages);
        let text = result.messages[0].content.text().unwrap();
        assert!(text.contains("omitted"));
        assert!(result.compacted_tokens < result.original_tokens);
    }

    #[test]
    fn test_tool_result_summarizer_skips_short() {
        let summarizer = ToolResultSummarizer::default();
        let messages = vec![make_msg(Role::Tool, "short output")];
        assert!(!summarizer.can_apply(&messages));
    }

    #[test]
    fn test_duplicate_remover() {
        let remover = DuplicateRemover;
        let messages = vec![
            make_tool_call_msg("fs_read", r#"{"path":"a.rs"}"#, "call-1"),
            make_tool_result("call-1", "content v1"),
            make_tool_call_msg("fs_read", r#"{"path":"a.rs"}"#, "call-2"),
            make_tool_result("call-2", "content v2"),
        ];
        assert!(remover.can_apply(&messages));

        let result = remover.compact(&messages);
        assert!(result.messages.len() < messages.len());
    }

    #[test]
    fn test_context_pruner() {
        let messages: Vec<Message> = (0..100)
            .map(|i| make_msg(Role::Assistant, &format!("message {}", i)))
            .collect();
        let pruner = ContextPruner::new(50);
        assert!(pruner.can_apply(&messages));

        let result = pruner.compact(&messages);
        assert!(result.messages.len() < messages.len());
    }

    #[test]
    fn test_smart_compactor_should_compact() {
        let compactor = SmartCompactor::new(1000);
        // Under threshold
        let few: Vec<Message> = (0..5).map(|i| make_msg(Role::User, &format!("msg {}", i))).collect();
        assert!(!compactor.should_compact(&few));

        // Over 200 messages
        let many: Vec<Message> = (0..201).map(|i| make_msg(Role::User, &format!("msg {}", i))).collect();
        assert!(compactor.should_compact(&many));
    }

    #[test]
    fn test_smart_compactor_runs_strategy() {
        let mut compactor = SmartCompactor::new(1000);
        // Create messages with long multi-line tool output
        let long_output: String = (0..100).map(|i| format!("line {} of output with some content here\n", i)).collect();
        let messages = vec![
            make_msg(Role::System, "You are helpful"),
            make_msg(Role::User, "Read this file"),
            make_tool_call_msg("fs_read", r#"{"path":"big.txt"}"#, "c1"),
            make_tool_result("c1", &long_output),
        ];

        let result = compactor.compact(&messages);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.compacted_tokens < result.original_tokens);
        assert!(result.quality_score > 0.0);
        assert!(compactor.stats().total_compactions == 1);
    }

    #[test]
    fn test_compaction_stats() {
        let mut stats = CompactionStats::default();
        let result = CompactedResult {
            messages: vec![],
            original_tokens: 1000,
            compacted_tokens: 500,
            quality_score: 0.9,
            strategy_used: "test".to_string(),
        };
        stats.record(&result);
        assert_eq!(stats.total_compactions, 1);
        assert_eq!(stats.tokens_saved, 500);
        assert!(stats.average_quality > 0.8);
    }

    #[test]
    fn test_compacted_result_ratio() {
        let result = CompactedResult {
            messages: vec![],
            original_tokens: 1000,
            compacted_tokens: 600,
            quality_score: 0.9,
            strategy_used: "test".to_string(),
        };
        assert_eq!(result.tokens_saved(), 400);
        assert!((result.compression_ratio() - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_format_stats_empty() {
        let stats = CompactionStats::default();
        assert!(stats.format_stats().contains("No compactions"));
    }

    #[test]
    fn test_format_stats_with_data() {
        let mut stats = CompactionStats::default();
        let result = CompactedResult {
            messages: vec![],
            original_tokens: 1000,
            compacted_tokens: 500,
            quality_score: 0.9,
            strategy_used: "test".to_string(),
        };
        stats.record(&result);
        let s = stats.format_stats();
        assert!(s.contains("1 times"));
        assert!(s.contains("500 tokens"));
    }

    #[test]
    fn test_utilization() {
        let compactor = SmartCompactor::new(10000);
        let messages = vec![make_msg(Role::User, "hello")];
        let util = compactor.utilization(&messages);
        assert!(util < 1.0);
    }
}
