use super::ChatChunk;
use futures_core::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// A streaming chat completion — yields `ChatChunk` items parsed from SSE.
pub struct ChatStream {
    receiver: mpsc::Receiver<Result<ChatChunk, maix_core::MaixError>>,
}

impl ChatStream {
    /// Create a new ChatStream from an HTTP response.
    /// Spawns a background task to read SSE events.
    pub fn new(resp: reqwest::Response) -> Self {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(Self::read_sse(resp, tx));
        Self { receiver: rx }
    }

    /// Create a ChatStream from an existing receiver (for non-OpenAI protocols).
    pub fn from_receiver(rx: mpsc::Receiver<Result<ChatChunk, maix_core::MaixError>>) -> Self {
        Self { receiver: rx }
    }

    async fn read_sse(
        resp: reqwest::Response,
        tx: mpsc::Sender<Result<ChatChunk, maix_core::MaixError>>,
    ) {
        use futures::StreamExt;

        let mut byte_stream = Box::pin(resp.bytes_stream());
        let mut buffer = Vec::new();

        while let Some(item) = byte_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    // Parse complete lines from buffer
                    let mut pos = 0;
                    while let Some(newline) = buffer[pos..].iter().position(|&b| b == b'\n') {
                        let line_end = pos + newline;
                        let line = &buffer[pos..line_end];
                        let line_str = String::from_utf8_lossy(line);
                        let trimmed = line_str.trim();

                        if let Some(data) = trimmed.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                return; // stream complete
                            }
                            match serde_json::from_str::<ChatChunk>(data) {
                                Ok(chunk) => {
                                    let _ = tx.send(Ok(chunk)).await;
                                }
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(maix_core::MaixError::Provider(format!(
                                            "SSE parse: {e}"
                                        ))))
                                        .await;
                                    return;
                                }
                            }
                        }

                        pos = line_end + 1; // skip newline
                    }
                    // Keep unprocessed bytes
                    if pos > 0 {
                        buffer.drain(..pos);
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Err(maix_core::MaixError::Http(e.to_string())))
                        .await;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_parse_chunk() {
        let data = r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk: crate::ChatChunk = serde_json::from_str(data).unwrap();
        let delta = chunk.choices[0].delta.as_ref().unwrap();
        assert_eq!(delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_parse_chunk_with_reasoning() {
        let data = r#"{"choices":[{"index":0,"delta":{"reasoning_content":"Let me think...","content":null},"finish_reason":null}]}"#;
        let chunk: crate::ChatChunk = serde_json::from_str(data).unwrap();
        let delta = chunk.choices[0].delta.as_ref().unwrap();
        assert_eq!(delta.reasoning_content.as_deref(), Some("Let me think..."));
        assert!(delta.content.is_none());
    }

    #[test]
    fn test_parse_chunk_with_tool_call() {
        let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"path\""}}]},"finish_reason":null}]}"#;
        let chunk: crate::ChatChunk = serde_json::from_str(data).unwrap();
        let tool_calls = chunk.choices[0].delta.as_ref().unwrap().tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(tool_calls[0].function.as_ref().unwrap().name.as_deref(), Some("read_file"));
    }

    #[test]
    fn test_parse_finish() {
        let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}}"#;
        let chunk: crate::ChatChunk = serde_json::from_str(data).unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.total_tokens, 30);
    }
}

impl Stream for ChatStream {
    type Item = Result<ChatChunk, maix_core::MaixError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}
