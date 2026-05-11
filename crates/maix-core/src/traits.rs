//! Common trait abstractions — shared across domain crates (Phase 1.2).
//!
//! These traits define the contracts that domain-layer crates fulfill,
//! enabling the core engine to remain loosely coupled to implementations.

use crate::{MaixResult, Message, ToolDef, TokenUsage};
use async_trait::async_trait;
use serde_json::Value;

/// Tool execution abstraction.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn def(&self) -> ToolDef;
    async fn execute(&self, args: Value, working_dir: &std::path::Path) -> MaixResult<String>;
}

/// Skill loading abstraction.
#[async_trait]
pub trait SkillProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn system_prompt(&self) -> Option<&str>;
    async fn run(&self, input: &str, working_dir: &std::path::Path) -> MaixResult<String>;
}

/// Memory storage abstraction.
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    async fn save(
        &mut self,
        id: &str,
        content: &str,
        kind: &str,
        importance: f32,
        session_id: Option<&str>,
    ) -> MaixResult<()>;
    async fn search(&self, query: &str, limit: usize) -> MaixResult<Vec<(String, String, f32)>>;
    async fn forget(&mut self, id: &str) -> MaixResult<()>;
    async fn context_for_session(&self, session_id: &str, max_tokens: usize) -> MaixResult<String>;
}

/// LLM model provider abstraction.
#[async_trait]
pub trait LLMProviderTrait: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDef]>) -> MaixResult<ChatOutput>;
    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDef]>,
    ) -> MaixResult<Box<dyn ChatStreamTrait>>;
    fn model_name(&self) -> &str;
    fn context_window(&self) -> usize;
}

/// A completed chat output (non-streaming).
#[derive(Debug, Clone)]
pub struct ChatOutput {
    pub message: Message,
    pub usage: TokenUsage,
}

/// Streaming chat output trait.
#[async_trait]
pub trait ChatStreamTrait: Send + Unpin {
    async fn next_chunk(&mut self) -> MaixResult<Option<ChatChunkData>>;
}

/// A single chunk from a streaming response.
#[derive(Debug, Clone)]
pub struct ChatChunkData {
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_call_name: Option<String>,
    pub tool_call_args: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}
