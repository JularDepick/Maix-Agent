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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml_manifest() {
        let toml_str = r#"
[skill]
name = "test-skill"
version = "1.0.0"
description = "A test skill"

[prompt]
system = "You are a test assistant."

[tools]
native = ["read_file", "write_file"]

[knowledge]
files = ["docs/*.md"]
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.skill.name, "test-skill");
        assert_eq!(manifest.skill.version, "1.0.0");
        assert_eq!(manifest.skill.description.as_deref(), Some("A test skill"));
        assert_eq!(manifest.prompt.system.as_deref(), Some("You are a test assistant."));
        assert_eq!(manifest.tools.native, vec!["read_file", "write_file"]);
        assert_eq!(manifest.knowledge.files, vec!["docs/*.md"]);
    }

    #[test]
    fn test_parse_toml_minimal() {
        let toml_str = r#"
[skill]
name = "minimal"
version = "0.1.0"
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.skill.name, "minimal");
        assert_eq!(manifest.skill.runtime, "prompt-only"); // default
        assert!(manifest.skill.description.is_none());
        assert!(manifest.prompt.system.is_none());
        assert!(manifest.tools.native.is_empty());
    }

    #[test]
    fn test_from_skill_md() {
        let md = r#"---
name: md-skill
version: 2.0.0
description: From markdown
runtime: prompt-only
author: Test
---

## System Prompt

You are a markdown-based skill.

## Tools

- native: read_file, grep

## Knowledge

- README.md
- docs/*.md
"#;
        let manifest = SkillManifest::from_skill_md(md).unwrap();
        assert_eq!(manifest.skill.name, "md-skill");
        assert_eq!(manifest.skill.version, "2.0.0");
        assert_eq!(manifest.skill.description.as_deref(), Some("From markdown"));
        assert_eq!(manifest.skill.author.as_deref(), Some("Test"));
        assert_eq!(manifest.prompt.system.as_deref(), Some("You are a markdown-based skill."));
        assert_eq!(manifest.tools.native, vec!["read_file", "grep"]);
        assert_eq!(manifest.knowledge.files, vec!["README.md", "docs/*.md"]);
    }

    #[test]
    fn test_from_skill_md_missing_frontmatter() {
        let md = "No frontmatter here";
        assert!(SkillManifest::from_skill_md(md).is_err());
    }

    #[test]
    fn test_from_skill_md_missing_name() {
        let md = "---\nversion: 1.0.0\n---\nBody";
        assert!(SkillManifest::from_skill_md(md).is_err());
    }

    #[test]
    fn test_from_skill_md_default_version() {
        let md = "---\nname: no-version\n---\nBody";
        let manifest = SkillManifest::from_skill_md(md).unwrap();
        assert_eq!(manifest.skill.version, "0.1.0");
    }

    #[test]
    fn test_runtime_enum() {
        let toml_str = r#"
[skill]
name = "wasm-skill"
version = "1.0"
runtime = "wasm"
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.runtime(), SkillRuntime::Wasm);

        let toml_str = r#"
[skill]
name = "native-skill"
version = "1.0"
runtime = "native"
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.runtime(), SkillRuntime::Native);

        let toml_str = r#"
[skill]
name = "prompt-skill"
version = "1.0"
runtime = "prompt-only"
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.runtime(), SkillRuntime::PromptOnly);
    }

    #[test]
    fn test_from_path_toml() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"
[skill]
name = "file-skill"
version = "1.0.0"
"#;
        std::fs::write(dir.path().join("maix-skill.toml"), toml_content).unwrap();
        let manifest = SkillManifest::from_path(dir.path()).unwrap();
        assert_eq!(manifest.skill.name, "file-skill");
    }

    #[test]
    fn test_from_dir_prefers_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        let md_content = "---\nname: md-first\nversion: 1.0\n---\nBody";
        let toml_content = "[skill]\nname = \"toml-first\"\nversion = \"2.0\"\n";
        std::fs::write(dir.path().join("SKILL.md"), md_content).unwrap();
        std::fs::write(dir.path().join("maix-skill.toml"), toml_content).unwrap();
        let manifest = SkillManifest::from_dir(dir.path()).unwrap();
        assert_eq!(manifest.skill.name, "md-first");
    }

    #[test]
    fn test_knowledge_path() {
        let toml_str = "[skill]\nname = \"kp\"\nversion = \"1.0\"\n";
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        let path = manifest.knowledge_path(std::path::Path::new("/skills/kp"), "docs/readme.md");
        assert_eq!(path, std::path::PathBuf::from("/skills/kp/docs/readme.md"));
    }

    #[test]
    fn test_runtime_unknown_defaults_to_prompt_only() {
        let toml_str = r#"
[skill]
name = "unknown-rt"
version = "1.0"
runtime = "python"
"#;
        let manifest: SkillManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.runtime(), SkillRuntime::PromptOnly);
    }

    #[test]
    fn test_from_skill_md_with_user_prefix() {
        let md = r#"---
name: up-skill
version: 1.0
---

## System Prompt

Be helpful.

## User Prefix

Always respond in JSON.
"#;
        let manifest = SkillManifest::from_skill_md(md).unwrap();
        assert_eq!(manifest.skill.name, "up-skill");
        assert_eq!(manifest.prompt.system.as_deref(), Some("Be helpful."));
        assert_eq!(manifest.prompt.user_prefix.as_deref(), Some("Always respond in JSON."));
    }

    #[test]
    fn test_from_skill_md_unknown_heading() {
        let md = r#"---
name: heading-skill
version: 1.0
---

## System Prompt

Be helpful.

## Random Heading

This should be ignored.

## Tools

- native: read_file
"#;
        let manifest = SkillManifest::from_skill_md(md).unwrap();
        assert_eq!(manifest.skill.name, "heading-skill");
        assert_eq!(manifest.tools.native, vec!["read_file"]);
    }
}
