//! SKILL manifest — parse maix-skill.toml + SKILL.md (Phase 2.5)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub skill: SkillMeta,
    #[serde(default)]
    pub prompt: SkillPrompt,
    #[serde(default)]
    pub tools: SkillTools,
    #[serde(default)]
    pub knowledge: SkillKnowledge,
    #[serde(default)]
    pub sandbox: SkillSandbox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub version: String,
    #[serde(default = "default_runtime")]
    pub runtime: String, // "wasm" | "native" | "prompt-only"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn default_runtime() -> String {
    "prompt-only".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillPrompt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prefix: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillTools {
    #[serde(default)]
    pub native: Vec<String>,
    #[serde(default)]
    pub custom: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillKnowledge {
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillSandbox {
    #[serde(default)]
    pub fs_read: Vec<String>,
    #[serde(default)]
    pub fs_write: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub shell: bool,
}

impl SkillManifest {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let toml_path = if path.is_dir() {
            path.join("maix-skill.toml")
        } else {
            path.to_path_buf()
        };

        let content =
            std::fs::read_to_string(&toml_path).map_err(|e| format!("read manifest: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("parse manifest: {e}"))
    }

    pub fn runtime(&self) -> SkillRuntime {
        match self.skill.runtime.as_str() {
            "wasm" => SkillRuntime::Wasm,
            "native" => SkillRuntime::Native,
            _ => SkillRuntime::PromptOnly,
        }
    }

    /// Absolute path to the given knowledge file (relative to skill dir).
    pub fn knowledge_path(&self, skill_dir: &Path, relative: &str) -> PathBuf {
        skill_dir.join(relative)
    }

    /// Parse from SKILL.md format (Phase 2.5).
    /// Format: YAML frontmatter between --- delimiters, then markdown body.
    pub fn from_skill_md(content: &str) -> Result<Self, String> {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err("SKILL.md: missing frontmatter".into());
        }
        let frontmatter = parts[1].trim();
        let body = parts[2];

        // Parse frontmatter as YAML-like key-value
        let mut name = String::new();
        let mut version = String::new();
        let mut description = String::new();
        let mut runtime = String::new();
        let mut author = String::new();

        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some((k, v)) = line.split_once(':') {
                let v = v.trim();
                match k.trim().to_lowercase().as_str() {
                    "name" => name = v.to_string(),
                    "version" => version = v.to_string(),
                    "description" => description = v.to_string(),
                    "runtime" => runtime = v.to_string(),
                    "author" => author = v.to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err("SKILL.md: name is required".into());
        }
        if version.is_empty() {
            version = "0.1.0".into();
        }
        if runtime.is_empty() {
            runtime = "prompt-only".into();
        }

        // Parse body sections
        let mut system_prompt = String::new();
        let mut user_prefix = String::new();
        let mut native_tools: Vec<String> = Vec::new();
        let mut knowledge_files: Vec<String> = Vec::new();
        let mut current_section = "";

        for line in body.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
                let heading = trimmed.trim_start_matches('#').trim().to_lowercase();
                match heading.as_str() {
                    "system prompt" | "system" => current_section = "system",
                    "user prefix" | "user" => current_section = "user",
                    "tools" => current_section = "tools",
                    "knowledge files" | "knowledge" => current_section = "knowledge",
                    "sandbox" => current_section = "sandbox",
                    _ => current_section = "",
                }
                continue;
            }

            match current_section {
                "system"
                    if !trimmed.is_empty() => {
                        if !system_prompt.is_empty() {
                            system_prompt.push('\n');
                        }
                        system_prompt.push_str(trimmed);
                    }
                "user"
                    if !trimmed.is_empty() => {
                        if !user_prefix.is_empty() {
                            user_prefix.push('\n');
                        }
                        user_prefix.push_str(trimmed);
                    }
                "tools"
                    if trimmed.starts_with("- native:") => {
                        let tools_str = trimmed.strip_prefix("- native:").unwrap_or("").trim();
                        native_tools.extend(tools_str.split(',').map(|s| s.trim().to_string()));
                    }
                "knowledge"
                    if trimmed.starts_with("- ") => {
                        let file = trimmed.strip_prefix("- ").unwrap_or("").trim();
                        if !file.is_empty() {
                            knowledge_files.push(file.to_string());
                        }
                    }
                _ => {}
            }
        }

        Ok(SkillManifest {
            skill: SkillMeta {
                name,
                version,
                runtime,
                description: if description.is_empty() { None } else { Some(description) },
                author: if author.is_empty() { None } else { Some(author) },
            },
            prompt: SkillPrompt {
                system: if system_prompt.is_empty() { None } else { Some(system_prompt) },
                user_prefix: if user_prefix.is_empty() { None } else { Some(user_prefix) },
            },
            tools: SkillTools {
                native: native_tools,
                custom: vec![],
            },
            knowledge: SkillKnowledge {
                files: knowledge_files,
            },
            sandbox: SkillSandbox::default(),
        })
    }

    /// Detect and parse from either SKILL.md or maix-skill.toml.
    pub fn from_dir(skill_dir: &Path) -> Result<Self, String> {
        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.exists() {
            let content = std::fs::read_to_string(&skill_md)
                .map_err(|e| format!("read SKILL.md: {e}"))?;
            return Self::from_skill_md(&content);
        }
        Self::from_path(skill_dir)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillRuntime {
    Wasm,
    Native,
    PromptOnly,
}
