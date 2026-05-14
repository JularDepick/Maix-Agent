//! Identity / Personality system (Phase 2.3).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Verbosity level for responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Verbosity {
    Terse,
    #[default]
    Concise,
    Detailed,
    Verbose,
}


impl std::fmt::Display for Verbosity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terse => write!(f, "terse"),
            Self::Concise => write!(f, "concise"),
            Self::Detailed => write!(f, "detailed"),
            Self::Verbose => write!(f, "verbose"),
        }
    }
}

/// Explanation level targeting different audiences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ExplanationLevel {
    Junior,
    Mid,
    #[default]
    Senior,
    Expert,
}


impl std::fmt::Display for ExplanationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Junior => write!(f, "junior"),
            Self::Mid => write!(f, "mid"),
            Self::Senior => write!(f, "senior"),
            Self::Expert => write!(f, "expert"),
        }
    }
}

/// Behavior settings for the persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorSettings {
    /// Prefer fs_edit over fs_write for small changes.
    pub prefer_edit: bool,
    /// Ask before writing files.
    pub ask_before_write: bool,
    /// Show reasoning/thinking process.
    pub thinking_out_loud: bool,
    /// Max tool rounds per turn.
    pub max_tool_rounds: u32,
}

impl Default for BehaviorSettings {
    fn default() -> Self {
        Self {
            prefer_edit: true,
            ask_before_write: false,
            thinking_out_loud: false,
            max_tool_rounds: 16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality_traits: Vec<String>,
    pub knowledge_domains: Vec<String>,
    pub tone: String,
    /// How verbose responses should be.
    #[serde(default)]
    pub verbosity: Verbosity,
    /// Target audience level.
    #[serde(default)]
    pub explanation_level: ExplanationLevel,
    /// Behavior preferences.
    #[serde(default)]
    pub behavior: BehaviorSettings,
    /// Preferred tools (agent will prefer these).
    #[serde(default)]
    pub tools_preferred: Vec<String>,
    /// Tools to avoid unless explicitly needed.
    #[serde(default)]
    pub tools_avoid: Vec<String>,
    /// Extra system prompt additions.
    #[serde(default)]
    pub extra_prompt: Option<String>,
}

impl Identity {
    pub fn new(
        id: String,
        name: String,
        description: String,
        system_prompt: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            system_prompt,
            personality_traits: vec![],
            knowledge_domains: vec![],
            tone: "professional".into(),
            verbosity: Verbosity::default(),
            explanation_level: ExplanationLevel::default(),
            behavior: BehaviorSettings::default(),
            tools_preferred: vec![],
            tools_avoid: vec![],
            extra_prompt: None,
        }
    }

    pub fn with_traits(mut self, traits: Vec<String>) -> Self {
        self.personality_traits = traits;
        self
    }

    pub fn with_domains(mut self, domains: Vec<String>) -> Self {
        self.knowledge_domains = domains;
        self
    }

    pub fn with_tone(mut self, tone: impl Into<String>) -> Self {
        self.tone = tone.into();
        self
    }

    pub fn with_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    pub fn with_explanation_level(mut self, level: ExplanationLevel) -> Self {
        self.explanation_level = level;
        self
    }

    pub fn with_behavior(mut self, behavior: BehaviorSettings) -> Self {
        self.behavior = behavior;
        self
    }

    pub fn with_tools_preferred(mut self, tools: Vec<String>) -> Self {
        self.tools_preferred = tools;
        self
    }

    pub fn with_extra_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.extra_prompt = Some(prompt.into());
        self
    }

    /// Get the verbosity instruction for the system prompt.
    fn verbosity_instruction(&self) -> &str {
        match self.verbosity {
            Verbosity::Terse => "Be extremely brief. One-line answers when possible. No explanations unless asked.",
            Verbosity::Concise => "Be concise. Short, direct answers. Skip unnecessary preamble.",
            Verbosity::Detailed => "Provide thorough explanations with context. Include relevant examples.",
            Verbosity::Verbose => "Be comprehensive. Explain reasoning, provide examples, cover edge cases.",
        }
    }

    /// Get the explanation level instruction.
    fn explanation_instruction(&self) -> &str {
        match self.explanation_level {
            ExplanationLevel::Junior => "Explain concepts clearly as if teaching a beginner. Define technical terms.",
            ExplanationLevel::Mid => "Assume solid fundamentals. Briefly explain advanced concepts.",
            ExplanationLevel::Senior => "Assume deep technical knowledge. Focus on nuances and trade-offs.",
            ExplanationLevel::Expert => "Assume expert-level knowledge. Be direct, skip basics entirely.",
        }
    }

    /// Build the full system prompt for this identity.
    pub fn build_prompt(&self, tools_section: &str, memory_context: &str) -> String {
        let mut parts = vec![format!("You are {}, {}.", self.name, self.description)];

        if !self.personality_traits.is_empty() {
            parts.push(format!(
                "Personality: {}.",
                self.personality_traits.join(", ")
            ));
        }

        if !self.knowledge_domains.is_empty() {
            parts.push(format!(
                "You specialize in: {}.",
                self.knowledge_domains.join(", ")
            ));
        }

        parts.push(format!("Tone: {}.", self.tone));
        parts.push(self.verbosity_instruction().to_string());
        parts.push(self.explanation_instruction().to_string());

        if !self.tools_preferred.is_empty() {
            parts.push(format!(
                "Preferred tools: {}.",
                self.tools_preferred.join(", ")
            ));
        }

        if !self.tools_avoid.is_empty() {
            parts.push(format!(
                "Avoid unless explicitly needed: {}.",
                self.tools_avoid.join(", ")
            ));
        }

        if let Some(ref extra) = self.extra_prompt {
            parts.push(extra.clone());
        }

        if !tools_section.is_empty() {
            parts.push(tools_section.to_string());
        }

        if !memory_context.is_empty() {
            parts.push(format!("\n## Memory Context\n{memory_context}"));
        }

        // Date placeholder - callers should inject actual date
        let current_date = "{current_date}";
        parts.push(format!("\nCurrent date: {current_date}"));

        parts.join("\n")
    }
}

#[derive(Debug, Default)]
pub struct IdentityManager {
    identities: HashMap<String, Identity>,
    active: Option<String>,
}

impl IdentityManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, identity: Identity) {
        self.identities.insert(identity.name.clone(), identity);
    }

    pub fn remove(&mut self, name: &str) -> Option<Identity> {
        self.identities.remove(name)
    }

    pub fn activate(&mut self, name: &str) -> Result<(), String> {
        if self.identities.contains_key(name) {
            self.active = Some(name.to_string());
            Ok(())
        } else {
            Err(format!("identity not found: {name}"))
        }
    }

    pub fn active(&self) -> Option<&Identity> {
        self.active
            .as_ref()
            .and_then(|name| self.identities.get(name))
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn list(&self) -> Vec<&Identity> {
        let mut ids: Vec<&Identity> = self.identities.values().collect();
        ids.sort_by(|a, b| a.name.cmp(&b.name));
        ids
    }

    pub fn get(&self, name: &str) -> Option<&Identity> {
        self.identities.get(name)
    }

    pub fn count(&self) -> usize {
        self.identities.len()
    }

    /// Build default identities.
    pub fn with_defaults(mut self) -> Self {
        self.register(
            Identity::new(
                "default-maix".into(),
                "Maix".into(),
                "a helpful, concise AI agent".into(),
                "You are Maix, an intelligent AI agent. Be concise and helpful.".into(),
            )
            .with_traits(vec!["helpful".into(), "concise".into(), "practical".into()])
            .with_domains(vec!["general programming".into(), "software engineering".into()])
            .with_tone("professional")
            .with_verbosity(Verbosity::Concise)
            .with_explanation_level(ExplanationLevel::Senior)
            .with_tools_preferred(vec!["fs_edit".into(), "grep".into(), "shell_exec".into()]),
        );

        self.register(
            Identity::new(
                "code-reviewer".into(),
                "Code Reviewer".into(),
                "a meticulous code reviewer who catches bugs and suggests improvements".into(),
                "You are a senior code reviewer. Focus on correctness, safety, and readability. Point out potential bugs and suggest concrete improvements.".into(),
            )
            .with_traits(vec!["meticulous".into(), "constructive".into(), "thorough".into()])
            .with_domains(vec!["code review".into(), "software quality".into(), "best practices".into()])
            .with_tone("constructive")
            .with_verbosity(Verbosity::Detailed)
            .with_explanation_level(ExplanationLevel::Senior),
        );

        self.register(
            Identity::new(
                "architect".into(),
                "Architect".into(),
                "a system architect who designs scalable solutions".into(),
                "You are a system architect. Think about trade-offs, scalability, and maintainability. Consider the big picture before diving into details.".into(),
            )
            .with_traits(vec!["analytical".into(), "forward-thinking".into(), "pragmatic".into()])
            .with_domains(vec!["system design".into(), "architecture".into(), "distributed systems".into()])
            .with_tone("analytical")
            .with_verbosity(Verbosity::Detailed)
            .with_explanation_level(ExplanationLevel::Senior),
        );

        self.register(
            Identity::new(
                "terse-coder".into(),
                "Terse Coder".into(),
                "an extremely concise coder who writes minimal code".into(),
                "You are a terse coder. Write code with minimal explanation. One-line answers when possible. No fluff.".into(),
            )
            .with_traits(vec!["efficient".into(), "minimal".into()])
            .with_domains(vec!["coding".into()])
            .with_tone("terse")
            .with_verbosity(Verbosity::Terse)
            .with_explanation_level(ExplanationLevel::Expert)
            .with_behavior(BehaviorSettings {
                prefer_edit: true,
                ask_before_write: false,
                thinking_out_loud: false,
                max_tool_rounds: 8,
            }),
        );

        self.activate("Maix").ok();
        self
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_prompt() {
        let id = Identity::new(
            "test".into(),
            "Tester".into(),
            "a test identity".into(),
            "You are a tester".into(),
        )
        .with_traits(vec!["precise".into()])
        .with_tone("formal")
        .with_verbosity(Verbosity::Detailed);

        let prompt = id.build_prompt("Tools: none", "Memory: empty");
        assert!(prompt.contains("You are Tester"));
        assert!(prompt.contains("precise"));
        assert!(prompt.contains("formal"));
        assert!(prompt.contains("thorough explanations"));
    }

    #[test]
    fn test_identity_manager_defaults() {
        let mgr = IdentityManager::new().with_defaults();
        assert_eq!(mgr.count(), 4);
        assert!(mgr.active().is_some());
        assert_eq!(mgr.active_name(), Some("Maix"));
    }

    #[test]
    fn test_identity_activate() {
        let mut mgr = IdentityManager::new().with_defaults();
        assert!(mgr.activate("Architect").is_ok());
        assert_eq!(mgr.active_name(), Some("Architect"));
    }

    #[test]
    fn test_verbosity_terse() {
        let id = Identity::new(
            "t".into(), "T".into(), "d".into(), "p".into(),
        ).with_verbosity(Verbosity::Terse);
        let prompt = id.build_prompt("", "");
        assert!(prompt.contains("extremely brief"));
    }

    #[test]
    fn test_explanation_level_junior() {
        let id = Identity::new(
            "j".into(), "J".into(), "d".into(), "p".into(),
        ).with_explanation_level(ExplanationLevel::Junior);
        let prompt = id.build_prompt("", "");
        assert!(prompt.contains("beginner"));
    }

    #[test]
    fn test_tools_preferred() {
        let id = Identity::new(
            "t".into(), "T".into(), "d".into(), "p".into(),
        ).with_tools_preferred(vec!["fs_edit".into(), "grep".into()]);
        let prompt = id.build_prompt("", "");
        assert!(prompt.contains("Preferred tools: fs_edit, grep"));
    }
}
