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
}

impl BatchEmbedder {
    pub fn new(provider: Arc<dyn EmbeddingProvider>, batch_size: usize) -> Self {
        Self { provider, buffer: Mutex::new(Vec::new()), batch_size }
    }

    /// Enqueue a single text for embedding. Returns when the batch is flushed.
    pub async fn embed_one(&self, text: &str) -> Option<Embedding> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut buf = self.buffer.lock().await;
            buf.push((text.to_string(), tx));
            if buf.len() >= self.batch_size {
                let batch = std::mem::take(&mut *buf);
                drop(buf); // release lock before async work
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

