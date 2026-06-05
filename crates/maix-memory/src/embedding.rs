//! Embedding generation for vector memory (Phase 2).

use async_trait::async_trait;
use maix_core::MaixResult;

/// An embedding vector — typically 1536 (OpenAI) or 1024 dims.
pub type Embedding = Vec<f32>;

/// Embedding provider trait.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> MaixResult<Vec<Embedding>>;
    fn dims(&self) -> usize;
}

/// Embedding provider using an OpenAI-compatible API.
pub struct APIEmbeddingProvider {
    client: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
    dims: usize,
}

impl APIEmbeddingProvider {
    pub fn new(api_base: String, api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base,
            api_key,
            model,
            dims: 1536,
        }
    }

    pub fn with_dims(mut self, dims: usize) -> Self {
        self.dims = dims;
        self
    }
}

#[async_trait]
impl EmbeddingProvider for APIEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> MaixResult<Vec<Embedding>> {
        let url = format!("{}/v1/embeddings", self.api_base.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(maix_core::MaixError::Provider(format!(
                "embedding API error: {text}"
            )));
        }

        let data: serde_json::Value = resp.json().await.map_err(|e| {
            maix_core::MaixError::Http(e.to_string())
        })?;
        let embeddings: Vec<Embedding> = data["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|item| {
                item["embedding"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect()
            })
            .collect();

        Ok(embeddings)
    }

    fn dims(&self) -> usize {
        self.dims
    }
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-9 || norm_b < 1e-9 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ---------------------------------------------------------------------------
// Batch embedder — buffers embeddings to reduce API calls
// ---------------------------------------------------------------------------

use std::sync::Arc;
use tokio::sync::Mutex;

/// Wraps an EmbeddingProvider and accumululates texts for batch processing.
pub struct BatchEmbedder {
    provider: Arc<dyn EmbeddingProvider>,
    buffer: Mutex<Vec<(String, tokio::sync::oneshot::Sender<Option<Embedding>>)>>,
    batch_size: usize,
    flush_interval: std::time::Duration,
    last_flush: Mutex<std::time::Instant>,
}

impl BatchEmbedder {
    pub fn new(provider: Arc<dyn EmbeddingProvider>, batch_size: usize) -> Self {
        Self {
            provider,
            buffer: Mutex::new(Vec::new()),
            batch_size,
            flush_interval: std::time::Duration::from_secs(5),
            last_flush: Mutex::new(std::time::Instant::now()),
        }
    }

    /// Set the flush interval for time-based flushing.
    pub fn with_flush_interval(mut self, interval: std::time::Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Enqueue a single text for embedding. Returns when the batch is flushed.
    pub async fn embed_one(&self, text: &str) -> Option<Embedding> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut buf = self.buffer.lock().await;
            buf.push((text.to_string(), tx));

            let should_flush = buf.len() >= self.batch_size
                || self.last_flush.lock().await.elapsed() >= self.flush_interval;

            if should_flush {
                let batch = std::mem::take(&mut *buf);
                *self.last_flush.lock().await = std::time::Instant::now();
                drop(buf);
                self.flush_batch(batch).await;
            }
        }
        rx.await.ok().flatten()
    }

    /// Force flush remaining items. Call on shutdown.
    pub async fn flush_all(&self) {
        let batch = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };
        if !batch.is_empty() {
            self.flush_batch(batch).await;
        }
    }

    async fn flush_batch(
        &self,
        batch: Vec<(String, tokio::sync::oneshot::Sender<Option<Embedding>>)>,
    ) {
        let texts: Vec<String> = batch.iter().map(|(t, _)| t.clone()).collect();
        let embeddings = self.provider.embed(&texts).await.ok().unwrap_or_default();
        let embedding_iter = embeddings.into_iter().chain(std::iter::repeat_with(Vec::new));
        for ((_, tx), emb) in batch.into_iter().zip(embedding_iter) {
            let _ = tx.send(if emb.is_empty() { None } else { Some(emb) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_symmetric() {
        let a = vec![0.5, 0.3, 0.8];
        let b = vec![0.2, 0.9, 0.1];
        assert!((cosine_similarity(&a, &b) - cosine_similarity(&b, &a)).abs() < 1e-6);
    }

    #[test]
    fn test_api_embedding_provider_dims() {
        let provider = APIEmbeddingProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        );
        assert_eq!(provider.dims(), 1536);

        let provider = provider.with_dims(768);
        assert_eq!(provider.dims(), 768);
    }

    #[test]
    fn test_batch_embedder_flush_interval_config() {
        let provider = Arc::new(APIEmbeddingProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ));
        let embedder = BatchEmbedder::new(provider, 10)
            .with_flush_interval(std::time::Duration::from_secs(10));
        assert_eq!(embedder.flush_interval, std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_batch_embedder_default_interval() {
        let provider = Arc::new(APIEmbeddingProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ));
        let embedder = BatchEmbedder::new(provider, 10);
        assert_eq!(embedder.flush_interval, std::time::Duration::from_secs(5));
    }
}

