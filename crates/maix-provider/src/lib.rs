//! LLM Provider abstraction layer.
//!
//! Supports any OpenAI-compatible API (DeepSeek, MiniMax, OpenAI, etc.).
//! Key types: [`LLMProvider`] trait, [`OpenAICompatProvider`], [`ProviderRegistry`].

pub mod anthropic;
mod openai_compat;
mod registry;
pub mod stream;
mod traits;

pub use anthropic::AnthropicProvider;
pub use openai_compat::OpenAICompatProvider;
pub use registry::ProviderRegistry;
pub use stream::ChatStream;
pub use traits::{LLMProvider, ProviderCapabilities};

use maix_core::{MaixError, Message, ToolDef, TokenUsage};
use serde::{Deserialize, Serialize};

/// Convert a reqwest error into MaixError::Http.
pub(crate) fn http_err(e: impl std::fmt::Display) -> MaixError {
    MaixError::Http(e.to_string())
}

/// Non-streaming chat request.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    None,
    Auto,
    Required,
    #[serde(untagged)]
    Specific { function: SpecificFunction },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificFunction {
    pub name: String,
}

/// Non-streaming chat response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: TokenUsage,
}

/// SSE streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub choices: Vec<ChoiceDelta>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChoiceDelta {
    #[serde(default)]
    pub index: u32,
    pub delta: Option<DeltaContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaContent {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaToolCall {
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
