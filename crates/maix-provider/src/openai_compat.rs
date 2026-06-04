use super::{ChatRequest, ChatResponse, ChatStream, LLMProvider, ProviderCapabilities};
use super::rate_limiter::{RateLimiter, RetryConfig};
use async_trait::async_trait;
use maix_core::MaixResult;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Global shared HTTP client with connection pooling.
pub fn global_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("failed to build global reqwest client")
    })
}

/// A provider that speaks the OpenAI `/v1/chat/completions` protocol.
/// Works with any compatible API.
pub struct OpenAICompatProvider {
    client: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
    context_window: usize,
    extra_headers: Vec<(String, String)>,
    /// Extra fields merged into the JSON body of every request.
    extra_body: serde_json::Map<String, serde_json::Value>,
    capabilities: ProviderCapabilities,
    /// Rate limiter (optional).
    rate_limiter: Option<Arc<RateLimiter>>,
    /// Retry configuration.
    retry_config: RetryConfig,
}

impl OpenAICompatProvider {
    pub fn new(
        api_base: String,
        api_key: String,
        model: String,
    ) -> Self {
        Self {
            client: global_http_client().clone(),
            api_base,
            api_key,
            model,
            context_window: 128_000,
            extra_headers: Vec::new(),
            extra_body: Default::default(),
            capabilities: ProviderCapabilities::default(),
            rate_limiter: None,
            retry_config: RetryConfig::default(),
        }
    }

    /// Enable rate limiting.
    pub fn with_rate_limit(mut self, requests_per_minute: u32) -> Self {
        self.rate_limiter = Some(Arc::new(RateLimiter::new(requests_per_minute)));
        self
    }

    /// Set retry configuration.
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Set the context window size.
    pub fn with_context_window(mut self, tokens: usize) -> Self {
        self.context_window = tokens;
        self.capabilities.max_context = tokens;
        self
    }

    /// Enable reasoning support.
    pub fn with_reasoning(mut self) -> Self {
        self.capabilities.supports_reasoning = true;
        self
    }

    /// Set provider capabilities.
    pub fn with_capabilities(mut self, caps: ProviderCapabilities) -> Self {
        self.context_window = caps.max_context;
        self.capabilities = caps;
        self
    }

    /// Add an extra HTTP header to every request.
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }

    /// Add an extra JSON field to every request body.
    pub fn with_body_field(mut self, key: &str, value: serde_json::Value) -> Self {
        self.extra_body.insert(key.into(), value);
        self
    }

    /// Build the full chat completions URL.
    fn chat_url(&self) -> String {
        let base = self.api_base.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        }
    }

    /// Inject extra fields into the serialized request body.
    fn build_body(&self, req: &ChatRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::to_value(req).unwrap_or_default();
        if let Some(obj) = body.as_object_mut() {
            // Use model override from request if set, otherwise use provider default
            let model = req.model_override.as_deref().unwrap_or(&self.model);
            obj.insert("model".into(), model.into());
            obj.insert("stream".into(), stream.into());
            for (k, v) in &self.extra_body {
                obj.insert(k.clone(), v.clone());
            }
        }
        body
    }
}

#[async_trait]
impl LLMProvider for OpenAICompatProvider {
    async fn chat(&self, req: ChatRequest) -> MaixResult<ChatResponse> {
        let body = self.build_body(&req, false);
        tracing::debug!(body = %body, "sending chat request");

        let client = &self.client;
        let url = self.chat_url();
        let api_key = &self.api_key;
        let extra_headers = &self.extra_headers;

        super::rate_limiter::with_retry(
            &self.retry_config,
            self.rate_limiter.as_deref(),
            || async {
                let mut http_req = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&body);

                for (k, v) in extra_headers {
                    http_req = http_req.header(k.as_str(), v.as_str());
                }

                let resp = http_req.send().await.map_err(super::http_err)?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(maix_core::MaixError::Provider(format!(
                        "HTTP {status}: {text}"
                    )));
                }

                resp.json().await.map_err(|e| {
                    maix_core::MaixError::Provider(format!("failed to parse response: {e}"))
                })
            },
        )
        .await
    }

    async fn chat_stream(&self, req: ChatRequest) -> MaixResult<ChatStream> {
        let body = self.build_body(&req, true);
        tracing::debug!(body = %body, "starting chat stream");

        let client = &self.client;
        let url = self.chat_url();
        let api_key = &self.api_key;
        let extra_headers = &self.extra_headers;

        // Rate limit before opening the stream, retry on connection errors
        let resp = super::rate_limiter::with_retry(
            &self.retry_config,
            self.rate_limiter.as_deref(),
            || async {
                let mut http_req = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .header("Accept", "text/event-stream")
                    .json(&body);

                for (k, v) in extra_headers {
                    http_req = http_req.header(k.as_str(), v.as_str());
                }

                let resp = http_req.send().await.map_err(super::http_err)?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(maix_core::MaixError::Provider(format!(
                        "HTTP {status}: {text}"
                    )));
                }

                Ok(resp)
            },
        )
        .await?;

        Ok(ChatStream::new(resp))
    }

    fn context_window(&self) -> usize {
        self.context_window
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_url_with_v1_suffix() {
        let provider = OpenAICompatProvider::new(
            "https://api.example.com/v1".into(), "key".into(), "model".into()
        );
        assert_eq!(provider.chat_url(), "https://api.example.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_url_without_v1_suffix() {
        let provider = OpenAICompatProvider::new(
            "https://api.example.com".into(), "key".into(), "model".into()
        );
        assert_eq!(provider.chat_url(), "https://api.example.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_url_trailing_slash() {
        let provider = OpenAICompatProvider::new(
            "https://api.example.com/v1/".into(), "key".into(), "model".into()
        );
        assert_eq!(provider.chat_url(), "https://api.example.com/v1/chat/completions");
    }

    #[test]
    fn test_builder_context_window() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_context_window(64_000);
        assert_eq!(provider.context_window(), 64_000);
    }

    #[test]
    fn test_builder_reasoning() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_reasoning();
        assert!(provider.capabilities().supports_reasoning);
    }

    #[test]
    fn test_builder_header() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_header("X-Custom", "value");
        assert_eq!(provider.extra_headers.len(), 1);
        assert_eq!(provider.extra_headers[0].0, "X-Custom");
    }

    #[test]
    fn test_build_body_basic() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "gpt-4o".into()
        );
        let req = ChatRequest {
            messages: vec![],
            tools: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            model_override: None,
        };
        let body = provider.build_body(&req, false);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn test_build_body_model_override() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "gpt-4o".into()
        );
        let req = ChatRequest {
            messages: vec![],
            tools: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            model_override: Some("gpt-4o-mini".into()),
        };
        let body = provider.build_body(&req, false);
        assert_eq!(body["model"], "gpt-4o-mini"); // overridden
    }

    #[test]
    fn test_build_body_extra_fields() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_body_field("top_k", serde_json::json!(10));
        let req = ChatRequest {
            messages: vec![],
            tools: None,
            tool_choice: None,
            temperature: None,
            max_tokens: None,
            model_override: None,
        };
        let body = provider.build_body(&req, true);
        assert_eq!(body["top_k"], 10);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_with_rate_limit() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_rate_limit(60);
        assert!(provider.rate_limiter.is_some());
    }

    #[test]
    fn test_with_retry_config() {
        use super::super::rate_limiter::RetryConfig;
        let config = RetryConfig { max_retries: 5, ..Default::default() };
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_retry_config(config);
        assert_eq!(provider.retry_config.max_retries, 5);
    }

    #[test]
    fn test_with_body_field() {
        let provider = OpenAICompatProvider::new(
            "http://localhost".into(), "key".into(), "model".into()
        ).with_body_field("custom", serde_json::json!("value"));
        assert_eq!(provider.extra_body.len(), 1);
        assert_eq!(provider.extra_body["custom"], "value");
    }
}
