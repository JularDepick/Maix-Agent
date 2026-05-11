//! Anthropic protocol provider — Claude API native protocol support (stub).

use async_trait::async_trait;
use maix_core::traits::{ChatOutput, ChatStreamTrait, LLMProviderTrait};
use maix_core::{MaixError, MaixResult, Message, ToolDef};

pub struct AnthropicProvider {
    #[allow(dead_code)]
    api_key: String,
    model: String,
    #[allow(dead_code)]
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: "https://api.anthropic.com/v1/messages".into(),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl LLMProviderTrait for AnthropicProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    fn context_window(&self) -> usize {
        200_000
    }

    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDef]>,
    ) -> MaixResult<ChatOutput> {
        Err(MaixError::Provider(
            "Anthropic provider is not yet implemented".into(),
        ))
    }

    async fn chat_stream(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDef]>,
    ) -> MaixResult<Box<dyn ChatStreamTrait>> {
        Err(MaixError::Provider(
            "Anthropic streaming is not yet implemented".into(),
        ))
    }
}
