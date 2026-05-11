//! Hybrid retrieval: vector similarity + BM25 keyword + time decay + importance.

use super::embedding::cosine_similarity;
use super::MemoryEntry;
use crate::embedding::Embedding;
use chrono::Utc;

/// A scored memory entry from retrieval.
#[derive(Debug, Clone)]
pub struct ScoredEntry {
    pub entry: MemoryEntry,
    pub score: f32,
}

/// Hybrid retriever that combines multiple scoring signals.
pub struct HybridRetriever;

impl HybridRetriever {
    /// Retrieve top-K entries given a query embedding, text query, and entry list.
    pub fn retrieve(
        entries: &[MemoryEntry],
        embeddings: &[Embedding],
        query_embedding: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> Vec<ScoredEntry> {
        let mut scored: Vec<ScoredEntry> = Vec::new();
        let now = Utc::now();

        for (i, entry) in entries.iter().enumerate() {
            let cos_sim = if i < embeddings.len() {
                cosine_similarity(query_embedding, &embeddings[i])
            } else {
                0.0
            };

            let bm25 = Self::bm25_score(entry, query_text);

            // Time decay: newer = higher. Half-life of 30 days.
            let days_old = (now - entry.created_at).num_hours() as f32 / 24.0;
            let time_score = 0.5_f32.powf(days_old / 30.0);

            let importance = entry.importance;

            // Weighted combination
            let score = 0.40 * cos_sim + 0.30 * bm25 + 0.15 * time_score + 0.15 * importance;

            if score > 0.05 {
                scored.push(ScoredEntry {
                    entry: entry.clone(),
                    score,
                });
            }
        }

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    /// Simple BM25-like keyword score.
    fn bm25_score(entry: &MemoryEntry, query: &str) -> f32 {
        let content_lower = entry.content.to_lowercase();
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
        let content_len = content_lower.len() as f32;
        if content_len < 1.0 {
            return 0.0;
        }

        let mut score = 0.0;
        let avg_dl = 200.0; // average document length estimate
        let k1 = 1.2;
        let b = 0.75;

        for term in &query_terms {
            let tf = content_lower.matches(term).count() as f32;
            if tf > 0.0 {
                let numerator = tf * (k1 + 1.0);
                let denominator = tf + k1 * (1.0 - b + b * content_len / avg_dl);
                score += numerator / denominator;
            }
        }

        // Normalize to [0, 1] range approximately
        (score / (score + 1.0)).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::Embedding;
    use crate::{MemoryEntry, MemoryKind};
    use std::collections::HashMap;

    fn make_entry(id: &str, content: &str, importance: f32, days_ago: i64) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            content: content.into(),
            kind: MemoryKind::Semantic,
            importance,
            created_at: Utc::now() - chrono::Duration::days(days_ago),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_hybrid_retrieval_ranking() {
        let entries = vec![
            make_entry("e1", "Rust is a systems programming language", 1.0, 1),
            make_entry("e2", "Python is great for data science", 0.5, 30),
            make_entry("e3", "Rust memory safety and concurrency", 0.9, 5),
        ];

        // Simple random-ish embeddings (not real, just for testing)
        let embeddings: Vec<Embedding> = entries.iter().map(|_| vec![0.1; 128]).collect();
        let query_emb = vec![0.2; 128];

        let results = HybridRetriever::retrieve(
            &entries, &embeddings, &query_emb, "Rust systems", 3,
        );

        assert!(!results.is_empty());
        // e1 should rank high (recent + keyword match + high importance)
        assert_eq!(results[0].entry.id, "e1");
    }
}
