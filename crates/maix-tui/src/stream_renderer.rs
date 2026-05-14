#![allow(dead_code)]
//! Stream renderer — real-time token display with abort support.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Events emitted during streaming.
#[derive(Debug, Clone)]
pub enum RenderEvent {
    Token(String),
    Done,
    Aborted,
    Error(String),
}

/// Streaming response renderer.
pub struct StreamRenderer {
    buffer: String,
    aborted: Arc<AtomicBool>,
}

impl StreamRenderer {
    pub fn new() -> (Self, Arc<AtomicBool>) {
        let aborted = Arc::new(AtomicBool::new(false));
        let renderer = Self {
            buffer: String::new(),
            aborted: aborted.clone(),
        };
        (renderer, aborted)
    }

    pub fn append_token(&mut self, token: &str) {
        if !self.aborted.load(Ordering::Relaxed) {
            self.buffer.push_str(token);
        }
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::Relaxed);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn take_buffer(&mut self) -> String {
        std::mem::take(&mut self.buffer)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.aborted.store(false, Ordering::Relaxed);
    }

    /// Process a stream of tokens, returning events.
    pub fn process_tokens(&mut self, tokens: &[String]) -> Vec<RenderEvent> {
        let mut events = Vec::new();
        for token in tokens {
            if self.is_aborted() {
                events.push(RenderEvent::Aborted);
                break;
            }
            self.append_token(token);
            events.push(RenderEvent::Token(token.clone()));
        }
        if !self.is_aborted() {
            events.push(RenderEvent::Done);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_renderer_basic() {
        let (mut renderer, _) = StreamRenderer::new();
        renderer.append_token("Hello");
        renderer.append_token(" World");
        assert_eq!(renderer.buffer(), "Hello World");
    }

    #[test]
    fn test_stream_renderer_abort() {
        let (renderer, _aborted) = StreamRenderer::new();
        assert!(!renderer.is_aborted());
        renderer.abort();
        assert!(renderer.is_aborted());
    }

    #[test]
    fn test_stream_renderer_no_append_after_abort() {
        let (mut renderer, _) = StreamRenderer::new();
        renderer.append_token("Hello");
        renderer.abort();
        renderer.append_token(" World");
        assert_eq!(renderer.buffer(), "Hello"); // World not appended
    }

    #[test]
    fn test_take_buffer() {
        let (mut renderer, _) = StreamRenderer::new();
        renderer.append_token("test");
        let content = renderer.take_buffer();
        assert_eq!(content, "test");
        assert!(renderer.buffer().is_empty());
    }

    #[test]
    fn test_clear() {
        let (mut renderer, _) = StreamRenderer::new();
        renderer.append_token("test");
        renderer.abort();
        renderer.clear();
        assert!(renderer.buffer().is_empty());
        assert!(!renderer.is_aborted());
    }

    #[test]
    fn test_process_tokens() {
        let (mut renderer, _) = StreamRenderer::new();
        let tokens = vec!["Hello".into(), " ".into(), "World".into()];
        let events = renderer.process_tokens(&tokens);
        assert_eq!(events.len(), 4); // 3 tokens + Done
        assert_eq!(renderer.buffer(), "Hello World");
    }

    #[test]
    fn test_process_tokens_aborted() {
        let (renderer, aborted) = StreamRenderer::new();
        let mut renderer = renderer;
        renderer.abort();
        let tokens = vec!["Hello".into(), "World".into()];
        let events = renderer.process_tokens(&tokens);
        assert!(events.iter().any(|e| matches!(e, RenderEvent::Aborted)));
    }

    #[test]
    fn test_render_event_clone() {
        let event = RenderEvent::Token("test".into());
        let cloned = event.clone();
        match cloned {
            RenderEvent::Token(s) => assert_eq!(s, "test"),
            _ => panic!("expected Token"),
        }
    }
}
