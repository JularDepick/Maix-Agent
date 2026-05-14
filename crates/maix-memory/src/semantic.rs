//! Semantic memory — cosine similarity search with embeddings.

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Simple TF-IDF-like embedding for local use.
pub fn simple_embed(text: &str, dimension: usize) -> Vec<f32> {
    let mut embedding = vec![0.0f32; dimension];
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return embedding;
    }

    for word in &words {
        let hash = simple_hash(word) % dimension;
        embedding[hash] += 1.0;
    }

    // Normalize
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut embedding {
            *v /= norm;
        }
    }

    embedding
}

fn simple_hash(s: &str) -> usize {
    let mut hash: usize = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as usize);
    }
    hash
}

/// Search entries by semantic similarity.
pub fn semantic_search(
    query_embedding: &[f32],
    entries: &[(usize, &[f32])],
    top_k: usize,
) -> Vec<(usize, f32)> {
    let mut scores: Vec<(usize, f32)> = entries
        .iter()
        .map(|(idx, emb)| (*idx, cosine_similarity(query_embedding, emb)))
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.into_iter().take(top_k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn test_simple_embed() {
        let emb = simple_embed("hello world", 128);
        assert_eq!(emb.len(), 128);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01); // normalized
    }

    #[test]
    fn test_simple_embed_empty() {
        let emb = simple_embed("", 64);
        assert!(emb.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_simple_embed_similar_texts() {
        let a = simple_embed("rust programming language", 256);
        let b = simple_embed("rust coding language", 256);
        let c = simple_embed("python web framework", 256);
        let sim_ab = cosine_similarity(&a, &b);
        let sim_ac = cosine_similarity(&a, &c);
        assert!(sim_ab > sim_ac); // a is more similar to b than to c
    }

    #[test]
    fn test_semantic_search() {
        let emb1 = vec![1.0, 0.0];
        let emb2 = vec![0.0, 1.0];
        let emb3 = vec![0.7, 0.7];
        let entries = vec![(0, emb1.as_slice()), (1, emb2.as_slice()), (2, emb3.as_slice())];
        let query = vec![1.0, 0.0];
        let results = semantic_search(&query, &entries, 2);
        assert_eq!(results[0].0, 0); // most similar
    }

    #[test]
    fn test_simple_hash_deterministic() {
        assert_eq!(simple_hash("test"), simple_hash("test"));
        assert_ne!(simple_hash("abc"), simple_hash("xyz"));
    }
}
