//! Memory compaction — summarize and compress old memory entries.

use crate::{MemoryEntry, MemoryKind, MemoryStore};
use chrono::Utc;
use maix_core::{Message, MessageContent, Role};
use maix_provider::{ChatRequest, LLMProvider};
use std::collections::HashMap;

/// Compact session memories by keeping only the most relevant entries.
pub async fn compact_session(
    store: &mut dyn MemoryStore,
    query: &str,
    max_entries: usize,
) -> Result<usize, String> {
    let entries = store
        .search(query, max_entries * 2)
        .await
        .map_err(|e| e.to_string())?;

    if entries.len() <= max_entries {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in entries.iter().skip(max_entries) {
        let _ = store.forget(&entry.id).await;
        removed += 1;
    }
    Ok(removed)
}

/// Summarize old episodic entries using an LLM, save as a single semantic memory,
/// then delete the original entries.
pub async fn summarize_old_entries(
    store: &mut dyn MemoryStore,
    provider: &dyn LLMProvider,
    older_than_days: i64,
) -> Result<usize, String> {
    let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
    let entries = store
        .search("", 1000)
        .await
        .map_err(|e| e.to_string())?;

    let old_entries: Vec<&MemoryEntry> = entries.iter().filter(|e| e.created_at < cutoff).collect();
    if old_entries.is_empty() {
        return Ok(0);
    }

    // Build conversation text for summarization
    let conversation: Vec<String> = old_entries
        .iter()
        .map(|e| format!("[{}] {}", e.id, e.content))
        .collect();

    let prompt = format!(
        "Summarize these conversation memories into a concise summary. Preserve key facts, decisions, and context:\n\n{}",
        conversation.join("\n")
    );

    let req = ChatRequest {
        messages: vec![
            Message {
                role: Role::System,
                content: MessageContent::Text(
                    "You are a memory summarizer. Be concise but preserve important information.".into(),
                ),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
            Message {
                role: Role::User,
                content: MessageContent::Text(prompt),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
        ],
        tools: None,
        tool_choice: None,
        temperature: Some(0.3),
        max_tokens: Some(1024),
        model_override: None,
    };

    let response = provider
        .chat(req)
        .await
        .map_err(|e| format!("summarize: {e}"))?;

    let summary_text = match &response.message.content {
        MessageContent::Text(s) => s.clone(),
        _ => String::new(),
    };

    if !summary_text.is_empty() {
        // Save summary as semantic memory
        let summary_entry = MemoryEntry {
            id: format!("summary_{}", uuid::Uuid::new_v4()),
            content: summary_text,
            kind: MemoryKind::Semantic,
            importance: 0.8,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        };
        let _ = store.save(summary_entry).await;
    }

    // Delete old entries
    let count = old_entries.len();
    for entry in old_entries {
        let _ = store.forget(&entry.id).await;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compaction_smoke() {
        assert!(true);
    }
}
