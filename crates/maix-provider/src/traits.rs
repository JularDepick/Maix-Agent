use super::{ChatRequest, ChatResponse, ChatStream};
use async_trait::async_trait;
use maix_core::MaixResult;

/// What this provider/model supports.
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub max_context: usize,
    pub supports_reasoning: bool,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub max_tool_calls_per_turn: u8,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            max_context: 128_000,
            supports_reasoning: false,
            supports_tool_use: true,
            supports_vision: false,
            supports_streaming: true,
            max_tool_calls_per_turn: 1,
        }
    }
}

/// The core LLM provider abstraction.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Send a chat completion request, wait for full response.
    async fn chat(&self, req: ChatRequest) -> MaixResult<ChatResponse>;

    /// Send a chat completion request, stream chunks.
    async fn chat_stream(&self, req: ChatRequest) -> MaixResult<ChatStream>;

    /// Max context window size in tokens.
    fn context_window(&self) -> usize;

    /// Model name.
    fn model_name(&self) -> &str;

    /// Provider capabilities.
    fn capabilities(&self) -> ProviderCapabilities;
}
