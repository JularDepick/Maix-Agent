//! Context window optimization — smart truncation with importance scoring.

use maix_core::{Message, Role};

/// Importance weights for scoring messages.
#[derive(Debug, Clone)]
pub struct ImportanceWeights {
    /// Recency: newer messages score higher.
    pub recency: f32,
    /// Role-based importance (system > user > assistant > tool).
    pub role: f32,
    /// Tool results that contain errors are more important.
    pub error_boost: f32,
    /// Messages with tool calls are more important.
    pub tool_call_boost: f32,
}

impl Default for ImportanceWeights {
    fn default() -> Self {
        Self {
            recency: 0.3,
            role: 0.3,
            error_boost: 0.2,
            tool_call_boost: 0.2,
        }
    }
}

/// Score a message's importance (0.0 - 1.0).
pub fn score_message(msg: &Message, index: usize, total: usize, weights: &ImportanceWeights) -> f32 {
    let mut score = 0.0;

    // Recency: exponential decay from newest to oldest
    let recency = if total > 0 {
        index as f32 / total as f32
    } else {
        1.0
    };
    score += recency * weights.recency;

    // Role importance
    let role_score = match msg.role {
        Role::System => 1.0,
        Role::User => 0.8,
        Role::Assistant => 0.6,
        Role::Tool => 0.4,
    };
    score += role_score * weights.role;

    // Error boost: tool results containing errors are more important
    if msg.role == Role::Tool {
        if let Some(text) = msg.content.text() {
            let lower = text.to_lowercase();
            if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
                score += weights.error_boost;
            }
        }
    }

    // Tool call boost: assistant messages with tool calls
    if msg.role == Role::Assistant {
        if let Some(ref tool_calls) = msg.tool_calls {
            if !tool_calls.is_empty() {
                score += weights.tool_call_boost;
            }
        }
    }

    score.min(1.0)
}

/// Smart truncation: remove lowest-importance messages to fit within token budget.
/// Preserves system messages and recent messages.
pub fn smart_truncate(
    messages: &[Message],
    max_tokens: usize,
    weights: &ImportanceWeights,
) -> Vec<Message> {
    let total = messages.len();
    if total == 0 {
        return messages.to_vec();
    }

    // Calculate current token estimate
    let current_tokens: usize = messages.iter()
        .map(|m| m.content.text().unwrap_or("").len() / 4)
        .sum();

    if current_tokens <= max_tokens {
        return messages.to_vec();
    }

    // Score all messages
    let mut scored: Vec<(usize, f32, &Message)> = messages.iter()
        .enumerate()
        .map(|(i, m)| (i, score_message(m, i, total, weights), m))
        .collect();

    // Sort by score (lowest first for removal)
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Remove lowest-scored messages until under budget
    let mut removed = std::collections::HashSet::new();
    let mut tokens_to_remove = current_tokens.saturating_sub(max_tokens);

    for (idx, _score, msg) in &scored {
        if tokens_to_remove == 0 {
            break;
        }

        // Never remove system messages
        if msg.role == Role::System {
            continue;
        }

        // Never remove the last user message
        if msg.role == Role::User && *idx == total - 1 {
            continue;
        }

        let msg_tokens = msg.content.text().unwrap_or("").len() / 4;
        removed.insert(*idx);
        tokens_to_remove = tokens_to_remove.saturating_sub(msg_tokens);
    }

    // Rebuild message list without removed messages
    messages.iter()
        .enumerate()
        .filter(|(i, _)| !removed.contains(i))
        .map(|(_, m)| m.clone())
        .collect()
}

/// Estimate token count for a set of messages.
pub fn estimate_tokens(messages: &[Message]) -> u64 {
    messages.iter()
        .map(|m| {
            let text_len = m.content.text().unwrap_or("").len() as u64;
            let tool_len = m.tool_calls.as_ref()
                .map(|tcs| tcs.iter().map(|tc| tc.function.arguments.len() as u64 + tc.function.name.len() as u64).sum::<u64>())
                .unwrap_or(0);
            (text_len + tool_len) / 4
        })
        .sum()
}

/// Calculate context utilization percentage.
pub fn context_utilization(current_tokens: u64, max_context: u64) -> f32 {
    if max_context == 0 {
        return 0.0;
    }
    (current_tokens as f64 / max_context as f64 * 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use maix_core::MessageContent;

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

    #[test]
    fn test_score_system_highest() {
        let weights = ImportanceWeights::default();
        let sys = make_msg(Role::System, "You are helpful");
        let user = make_msg(Role::User, "Hello");
        let assistant = make_msg(Role::Assistant, "Hi");

        // System at index 0 has low recency but highest role score
        // User at index 1 has higher recency
        // When placed at the same recency position, system should score highest
        let s_sys = score_message(&sys, 2, 3, &weights);
        let s_user = score_message(&user, 2, 3, &weights);
        let s_assistant = score_message(&assistant, 2, 3, &weights);

        assert!(s_sys > s_user);
        assert!(s_user > s_assistant);
    }

    #[test]
    fn test_score_error_boost() {
        let weights = ImportanceWeights::default();
        let normal = make_msg(Role::Tool, "success");
        let error = make_msg(Role::Tool, "Error: file not found");

        let s_normal = score_message(&normal, 0, 2, &weights);
        let s_error = score_message(&error, 1, 2, &weights);

        assert!(s_error > s_normal);
    }

    #[test]
    fn test_smart_truncate_preserves_system() {
        let messages = vec![
            make_msg(Role::System, "System prompt"),
            make_msg(Role::User, "Hello"),
            make_msg(Role::Assistant, "Hi there, how can I help you today? I'm ready to assist with whatever you need."),
            make_msg(Role::User, "Tell me about Rust programming language and its advantages"),
            make_msg(Role::Assistant, "Rust is a systems programming language focused on safety, speed, and concurrency."),
        ];

        let weights = ImportanceWeights::default();
        let truncated = smart_truncate(&messages, 20, &weights);

        // System message should be preserved
        assert!(truncated.iter().any(|m| m.role == Role::System));
    }

    #[test]
    fn test_estimate_tokens() {
        let messages = vec![
            make_msg(Role::User, "hello world"),
        ];
        let tokens = estimate_tokens(&messages);
        assert!(tokens > 0);
        assert!(tokens < 10);
    }

    #[test]
    fn test_context_utilization() {
        assert_eq!(context_utilization(50000, 100000), 50.0);
        assert_eq!(context_utilization(0, 100000), 0.0);
        assert_eq!(context_utilization(100000, 0), 0.0);
    }
}
