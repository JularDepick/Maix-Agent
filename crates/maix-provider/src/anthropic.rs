//! Anthropic Messages API provider — Claude native protocol.

use super::{ChatRequest, ChatResponse, ChatStream, LLMProvider, ProviderCapabilities};
use async_trait::async_trait;
use maix_core::{MaixError, MaixResult, Message, Role, TokenUsage, ToolCall, FunctionCall};
use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

fn anthropic_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .build()
            .expect("failed to build anthropic client")
    })
}

/// Anthropic Messages API provider.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
    capabilities: ProviderCapabilities,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: anthropic_client().clone(),
            api_key,
            model,
            base_url: "https://api.anthropic.com".into(),
            capabilities: ProviderCapabilities {
                max_context: 200_000,
                supports_reasoning: true,
                supports_tool_use: true,
                supports_vision: true,
                supports_streaming: true,
                max_tool_calls_per_turn: 5,
            },
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').into();
        self
    }

    pub fn with_context_window(mut self, tokens: usize) -> Self {
        self.capabilities.max_context = tokens;
        self
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    /// Convert OpenAI-format messages to Anthropic format.
    /// Returns (system_prompt, anthropic_messages).
    fn convert_messages(&self, messages: &[Message]) -> (String, Vec<serde_json::Value>) {
        let mut system = String::new();
        let mut anth_messages: Vec<serde_json::Value> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if !system.is_empty() {
                        system.push('\n');
                    }
                    if let Some(text) = msg.content.text() {
                        system.push_str(text);
                    }
                }
                Role::User => {
                    let content = match &msg.content {
                        maix_core::MessageContent::Text(text) => {
                            serde_json::json!([{"type": "text", "text": text}])
                        }
                        maix_core::MessageContent::Parts(parts) => {
                            let blocks: Vec<serde_json::Value> = parts
                                .iter()
                                .map(|p| match p {
                                    maix_core::ContentPart::Text { text } => {
                                        serde_json::json!({"type": "text", "text": text})
                                    }
                                    maix_core::ContentPart::ImageUrl { image_url } => {
                                        serde_json::json!({
                                            "type": "image",
                                            "source": {"type": "url", "url": image_url.url}
                                        })
                                    }
                                })
                                .collect();
                            serde_json::json!(blocks)
                        }
                    };
                    // If this is a tool result message, add tool_result blocks
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        anth_messages.push(serde_json::json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": msg.content.text().unwrap_or("")
                            }]
                        }));
                    } else {
                        anth_messages.push(serde_json::json!({
                            "role": "user",
                            "content": content
                        }));
                    }
                }
                Role::Assistant => {
                    let mut blocks: Vec<serde_json::Value> = Vec::new();
                    if let Some(text) = msg.content.text() {
                        if !text.is_empty() {
                            blocks.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            let input: serde_json::Value =
                                serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.function.name,
                                "input": input
                            }));
                        }
                    }
                    if !blocks.is_empty() {
                        anth_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": blocks
                        }));
                    }
                }
                Role::Tool => {
                    // Tool results are handled in the User branch above
                    // But some flows send Role::Tool messages directly
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        anth_messages.push(serde_json::json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": msg.content.text().unwrap_or("")
                            }]
                        }));
                    }
                }
            }
        }

        (system, anth_messages)
    }

    /// Convert OpenAI ToolDef to Anthropic tool format.
    fn convert_tools(&self, tools: &[maix_core::ToolDef]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters
                })
            })
            .collect()
    }

    /// Build the request body for Anthropic Messages API.
    fn build_body(&self, req: &ChatRequest, stream: bool) -> serde_json::Value {
        let (system, messages) = self.convert_messages(&req.messages);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens.unwrap_or(8192),
            "messages": messages,
            "stream": stream,
        });

        if !system.is_empty() {
            body["system"] = serde_json::Value::String(system);
        }

        if let Some(tools) = &req.tools {
            let anth_tools = self.convert_tools(tools);
            if !anth_tools.is_empty() {
                body["tools"] = serde_json::json!(anth_tools);
            }
        }

        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        body
    }
}

/// Anthropic streaming response types.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicStreamMessage },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: serde_json::Value,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: usize,
        delta: serde_json::Value,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: serde_json::Value,
        usage: Option<AnthropicUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: serde_json::Value },
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamMessage {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<serde_json::Value>,
    model: String,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

/// Parse an Anthropic streaming event from a JSON line.
fn parse_stream_event(line: &str) -> Option<Result<AnthropicStreamEvent, MaixError>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Skip event: lines
    if trimmed.starts_with("event:") {
        return None;
    }
    let data = trimmed.strip_prefix("data: ").or_else(|| trimmed.strip_prefix("data:"))?;
    if data.is_empty() {
        return None;
    }
    match serde_json::from_str::<AnthropicStreamEvent>(data) {
        Ok(event) => Some(Ok(event)),
        Err(e) => Some(Err(MaixError::Provider(format!("anthropic SSE parse: {e}")))),
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn chat(&self, req: ChatRequest) -> MaixResult<ChatResponse> {
        let body = self.build_body(&req, false);
        tracing::debug!(body = %body, "anthropic chat request");

        let resp = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(super::http_err)?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MaixError::Provider(format!("Anthropic HTTP {status}: {text}")));
        }

        let resp_body: serde_json::Value = resp.json().await.map_err(|e| {
            MaixError::Provider(format!("Anthropic response parse: {e}"))
        })?;

        // Extract text content
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        if let Some(content) = resp_body["content"].as_array() {
            for block in content {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            text.push_str(t);
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or_default().to_string(),
                            call_type: "function".into(),
                            function: FunctionCall {
                                name: block["name"].as_str().unwrap_or_default().to_string(),
                                arguments: block["input"].to_string(),
                            },
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = resp_body["usage"].clone();
        let token_usage = TokenUsage {
            prompt_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
            completion_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
            total_tokens: usage["input_tokens"].as_u64().unwrap_or(0)
                + usage["output_tokens"].as_u64().unwrap_or(0),
            cache_read_tokens: usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
            cache_write_tokens: usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        };

        let message = Message {
            role: Role::Assistant,
            content: maix_core::MessageContent::Text(text),
            name: None,
            tool_call_id: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            reasoning_content: None,
        };

        Ok(ChatResponse {
            message,
            usage: token_usage,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> MaixResult<ChatStream> {
        let body = self.build_body(&req, true);
        tracing::debug!(body = %body, "anthropic chat stream request");

        let resp = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(super::http_err)?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MaixError::Provider(format!("Anthropic HTTP {status}: {text}")));
        }

        // Convert Anthropic SSE stream to OpenAI-compatible ChatStream
        Ok(ChatStream::from_anthropic(resp))
    }

    fn context_window(&self) -> usize {
        self.capabilities.max_context
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }
}

/// Extension: create ChatStream from Anthropic SSE response.
impl ChatStream {
    /// Create a ChatStream that converts Anthropic SSE events to OpenAI-compatible chunks.
    pub fn from_anthropic(resp: reqwest::Response) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(Self::read_anthropic_sse(resp, tx));
        Self::from_receiver(rx)
    }

    async fn read_anthropic_sse(
        resp: reqwest::Response,
        tx: tokio::sync::mpsc::Sender<Result<super::ChatChunk, MaixError>>,
    ) {
        use futures::StreamExt;

        let mut byte_stream = Box::pin(resp.bytes_stream());
        let mut buffer = Vec::new();
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_args = String::new();

        while let Some(item) = byte_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    let mut pos = 0;
                    while let Some(newline) = buffer[pos..].iter().position(|&b| b == b'\n') {
                        let line_end = pos + newline;
                        let line = String::from_utf8_lossy(&buffer[pos..line_end]);

                        if let Some(event) = parse_stream_event(&line) {
                            match event {
                                Ok(AnthropicStreamEvent::ContentBlockDelta { index: _, delta }) => {
                                    // Text delta
                                    if let Some(text) = delta["text"].as_str() {
                                        let chunk = super::ChatChunk {
                                            choices: vec![super::ChoiceDelta {
                                                index: 0,
                                                delta: Some(super::DeltaContent {
                                                    role: None,
                                                    content: Some(text.to_string()),
                                                    reasoning_content: None,
                                                    tool_calls: None,
                                                }),
                                                finish_reason: None,
                                            }],
                                            usage: None,
                                        };
                                        let _ = tx.send(Ok(chunk)).await;
                                    }
                                    // Tool input delta
                                    if let Some(partial_json) = delta["partial_json"].as_str() {
                                        current_tool_args.push_str(partial_json);
                                        let chunk = super::ChatChunk {
                                            choices: vec![super::ChoiceDelta {
                                                index: 0,
                                                delta: Some(super::DeltaContent {
                                                    role: None,
                                                    content: None,
                                                    reasoning_content: None,
                                                    tool_calls: Some(vec![super::DeltaToolCall {
                                                        index: 0,
                                                        id: current_tool_id.clone(),
                                                        function: Some(super::DeltaFunction {
                                                            name: None,
                                                            arguments: Some(
                                                                partial_json.to_string(),
                                                            ),
                                                        }),
                                                    }]),
                                                }),
                                                finish_reason: None,
                                            }],
                                            usage: None,
                                        };
                                        let _ = tx.send(Ok(chunk)).await;
                                    }
                                }
                                Ok(AnthropicStreamEvent::ContentBlockStart {
                                    index: _,
                                    content_block,
                                }) => {
                                    // Tool use block start
                                    if content_block["type"].as_str() == Some("tool_use") {
                                        current_tool_id =
                                            content_block["id"].as_str().map(String::from);
                                        current_tool_name =
                                            content_block["name"].as_str().map(String::from);
                                        current_tool_args.clear();

                                        let chunk = super::ChatChunk {
                                            choices: vec![super::ChoiceDelta {
                                                index: 0,
                                                delta: Some(super::DeltaContent {
                                                    role: None,
                                                    content: None,
                                                    reasoning_content: None,
                                                    tool_calls: Some(vec![
                                                        super::DeltaToolCall {
                                                            index: 0,
                                                            id: current_tool_id.clone(),
                                                            function: Some(super::DeltaFunction {
                                                                name: current_tool_name.clone(),
                                                                arguments: Some(String::new()),
                                                            }),
                                                        },
                                                    ]),
                                                }),
                                                finish_reason: None,
                                            }],
                                            usage: None,
                                        };
                                        let _ = tx.send(Ok(chunk)).await;
                                    }
                                }
                                Ok(AnthropicStreamEvent::MessageDelta { delta, usage }) => {
                                    let stop_reason =
                                        delta["stop_reason"].as_str().map(String::from);
                                    let finish_reason = match stop_reason.as_deref() {
                                        Some("end_turn") => Some("stop".into()),
                                        Some("tool_use") => Some("tool_calls".into()),
                                        other => other.map(String::from),
                                    };
                                    let mut chunk = super::ChatChunk {
                                        choices: vec![super::ChoiceDelta {
                                            index: 0,
                                            delta: Some(super::DeltaContent {
                                                role: None,
                                                content: None,
                                                reasoning_content: None,
                                                tool_calls: None,
                                            }),
                                            finish_reason,
                                        }],
                                        usage: usage.map(|u| TokenUsage {
                                            prompt_tokens: u.input_tokens,
                                            completion_tokens: u.output_tokens,
                                            total_tokens: u.input_tokens + u.output_tokens,
                                            cache_read_tokens: u.cache_read_input_tokens.unwrap_or(0),
                                            cache_write_tokens: u.cache_creation_input_tokens.unwrap_or(0),
                                        }),
                                    };
                                    // If tool use ended, emit final tool call with complete args
                                    if stop_reason.as_deref() == Some("tool_use") {
                                        if let Some(ref id) = current_tool_id {
                                            chunk.choices[0].delta = Some(super::DeltaContent {
                                                role: None,
                                                content: None,
                                                reasoning_content: None,
                                                tool_calls: Some(vec![super::DeltaToolCall {
                                                    index: 0,
                                                    id: Some(id.clone()),
                                                    function: Some(super::DeltaFunction {
                                                        name: current_tool_name.clone(),
                                                        arguments: Some(
                                                            current_tool_args.clone(),
                                                        ),
                                                    }),
                                                }]),
                                            });
                                        }
                                    }
                                    let _ = tx.send(Ok(chunk)).await;
                                    // Reset tool state
                                    current_tool_id = None;
                                    current_tool_name = None;
                                    current_tool_args.clear();
                                }
                                Ok(AnthropicStreamEvent::MessageStop) => {
                                    return;
                                }
                                Ok(AnthropicStreamEvent::Error { error }) => {
                                    let msg = error["message"]
                                        .as_str()
                                        .unwrap_or("unknown anthropic error");
                                    let _ = tx
                                        .send(Err(MaixError::Provider(format!(
                                            "Anthropic: {msg}"
                                        ))))
                                        .await;
                                    return;
                                }
                                Ok(_) => {} // Ping, ContentBlockStop, MessageStart — ignore
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    return;
                                }
                            }
                        }

                        pos = line_end + 1;
                    }
                    if pos > 0 {
                        buffer.drain(..pos);
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Err(MaixError::Http(e.to_string())))
                        .await;
                    return;
                }
            }
        }
    }
}
