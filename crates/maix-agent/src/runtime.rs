//! Agent Runtime — scans and loads agent configurations from ~/.maix/agents/

use maix_core::IdentityManager;
use std::path::PathBuf;

/// Agent configuration loaded from disk.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub name: String,
    pub description: String,
    pub tone: String,
    pub traits: Vec<String>,
    pub domains: Vec<String>,
    pub system_prompt: String,
}

/// Runtime that manages agent profiles and identity selection.
pub struct AgentRuntime {
    agents_dir: PathBuf,
    profiles: Vec<AgentProfile>,
    active: Option<String>,
    #[allow(dead_code)]
    identity_manager: IdentityManager,
}

impl Default for AgentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRuntime {
    pub fn new() -> Self {
        let agents_dir = maix_core::config::default_memory_dir()
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("agents");
        Self {
            agents_dir,
            profiles: Vec::new(),
            active: None,
            identity_manager: IdentityManager::new().with_defaults(),
        }
    }

    pub fn with_dir(mut self, dir: PathBuf) -> Self {
        self.agents_dir = dir;
        self
    }

    pub fn agents_dir(&self) -> &PathBuf {
        &self.agents_dir
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// Scan ~/.maix/agents/ for TOML agent configs and load them.
    pub fn scan(&mut self) -> Vec<AgentProfile> {
        self.profiles.clear();
        if let Ok(entries) = std::fs::read_dir(&self.agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Some(profile) = self.parse_profile(&content, &path) {
                            self.profiles.push(profile);
                        }
                    }
                }
            }
        }
        self.profiles.clone()
    }

    fn parse_profile(&self, content: &str, path: &std::path::Path) -> Option<AgentProfile> {
        let table: toml::Table = toml::from_str(content).ok()?;
        let name = table.get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let description = table.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tone = table.get("tone")
            .and_then(|v| v.as_str())
            .unwrap_or("professional")
            .to_string();
        let traits = table.get("traits")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let domains = table.get("domains")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let system_prompt = table.get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Some(AgentProfile {
            name,
            description,
            tone,
            traits,
            domains,
            system_prompt,
        })
    }

    pub fn profiles(&self) -> &[AgentProfile] {
        &self.profiles
    }

    pub fn set_active(&mut self, name: &str) -> bool {
        if self.profiles.iter().any(|p| p.name == name) {
            self.active = Some(name.to_string());
            true
        } else {
            false
        }
    }
}
