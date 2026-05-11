//! Identity / Personality system (Phase 2.3).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality_traits: Vec<String>,
    pub knowledge_domains: Vec<String>,
    pub tone: String,
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
            .with_tone("professional"),
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
            .with_tone("constructive"),
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
            .with_tone("analytical"),
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
        .with_tone("formal");

        let prompt = id.build_prompt("Tools: none", "Memory: empty");
        assert!(prompt.contains("You are Tester"));
        assert!(prompt.contains("precise"));
        assert!(prompt.contains("formal"));
    }

    #[test]
    fn test_identity_manager_defaults() {
        let mgr = IdentityManager::new().with_defaults();
        assert_eq!(mgr.count(), 3);
        assert!(mgr.active().is_some());
        assert_eq!(mgr.active_name(), Some("Maix"));
    }

    #[test]
    fn test_identity_activate() {
        let mut mgr = IdentityManager::new().with_defaults();
        assert!(mgr.activate("Architect").is_ok());
        assert_eq!(mgr.active_name(), Some("Architect"));
    }
}
