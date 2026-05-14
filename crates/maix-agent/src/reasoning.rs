//! Reasoning chain tracking — records and displays agent's decision process.

use serde::{Deserialize, Serialize};

/// Type of reasoning step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReasoningKind {
    Thinking,
    ToolSelection,
    Decision,
    Reflection,
}

impl ReasoningKind {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::ToolSelection => "tool",
            Self::Decision => "decide",
            Self::Reflection => "reflect",
        }
    }
}

/// A single reasoning step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub kind: ReasoningKind,
    pub content: String,
    pub confidence: f32,
}

/// Reasoning chain for an agent response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    steps: Vec<ReasoningStep>,
    collapsed: bool,
}

impl ReasoningChain {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            collapsed: false,
        }
    }

    pub fn push(&mut self, kind: ReasoningKind, content: &str) {
        self.steps.push(ReasoningStep {
            kind,
            content: content.to_string(),
            confidence: 0.8,
        });
    }

    pub fn push_with_confidence(&mut self, kind: ReasoningKind, content: &str, confidence: f32) {
        self.steps.push(ReasoningStep {
            kind,
            content: content.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
        });
    }

    pub fn steps(&self) -> &[ReasoningStep] {
        &self.steps
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn collapse(&mut self) {
        self.collapsed = true;
    }

    pub fn expand(&mut self) {
        self.collapsed = false;
    }

    pub fn clear(&mut self) {
        self.steps.clear();
    }

    /// Format for display.
    pub fn render(&self) -> Vec<String> {
        if self.collapsed {
            return vec![format!("  [{} reasoning steps hidden]", self.steps.len())];
        }
        self.steps
            .iter()
            .map(|s| {
                format!(
                    "  [{}] {} (conf: {:.0}%)",
                    s.kind.icon(),
                    s.content,
                    s.confidence * 100.0
                )
            })
            .collect()
    }

    pub fn average_confidence(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.steps.iter().map(|s| s.confidence).sum();
        sum / self.steps.len() as f32
    }
}

impl Default for ReasoningChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_new() {
        let chain = ReasoningChain::new();
        assert!(chain.is_empty());
        assert!(!chain.is_collapsed());
    }

    #[test]
    fn test_chain_push() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "Analyzing request");
        chain.push(ReasoningKind::ToolSelection, "Using grep for search");
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn test_chain_push_with_confidence() {
        let mut chain = ReasoningChain::new();
        chain.push_with_confidence(ReasoningKind::Decision, "High confidence", 0.95);
        assert!((chain.steps()[0].confidence - 0.95).abs() < 0.01);
    }

    #[test]
    fn test_chain_confidence_clamp() {
        let mut chain = ReasoningChain::new();
        chain.push_with_confidence(ReasoningKind::Thinking, "test", 1.5);
        assert!((chain.steps()[0].confidence - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_chain_toggle() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "test");
        chain.toggle();
        assert!(chain.is_collapsed());
        chain.toggle();
        assert!(!chain.is_collapsed());
    }

    #[test]
    fn test_chain_render_expanded() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "Step 1");
        let lines = chain.render();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Step 1"));
        assert!(lines[0].contains("thinking"));
    }

    #[test]
    fn test_chain_render_collapsed() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "Step 1");
        chain.push(ReasoningKind::Decision, "Step 2");
        chain.collapse();
        let lines = chain.render();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("2 reasoning steps hidden"));
    }

    #[test]
    fn test_chain_clear() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "test");
        chain.clear();
        assert!(chain.is_empty());
    }

    #[test]
    fn test_chain_average_confidence() {
        let mut chain = ReasoningChain::new();
        chain.push_with_confidence(ReasoningKind::Thinking, "a", 0.8);
        chain.push_with_confidence(ReasoningKind::Decision, "b", 0.6);
        assert!((chain.average_confidence() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_chain_empty_average() {
        let chain = ReasoningChain::new();
        assert!((chain.average_confidence()).abs() < 0.01);
    }

    #[test]
    fn test_reasoning_kind_icon() {
        assert_eq!(ReasoningKind::Thinking.icon(), "thinking");
        assert_eq!(ReasoningKind::ToolSelection.icon(), "tool");
        assert_eq!(ReasoningKind::Decision.icon(), "decide");
        assert_eq!(ReasoningKind::Reflection.icon(), "reflect");
    }

    #[test]
    fn test_chain_serialize() {
        let mut chain = ReasoningChain::new();
        chain.push(ReasoningKind::Thinking, "test");
        let json = serde_json::to_string(&chain).unwrap();
        assert!(json.contains("test"));
    }
}
