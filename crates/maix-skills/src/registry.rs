//! Skill registry — load, track, and manage installed skills.

use super::manifest::SkillManifest;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct InstalledSkill {
    pub manifest: SkillManifest,
    pub path: PathBuf,
    pub enabled: bool,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

pub struct SkillRegistry {
    skills: HashMap<String, InstalledSkill>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self { skills: HashMap::new(), skills_dir }
    }

    /// Install a skill from a directory containing maix-skill.toml.
    pub fn install(&mut self, source: &Path) -> Result<String, String> {
        let manifest = SkillManifest::from_path(source)?;
        let name = manifest.skill.name.clone();

        // Copy to skills directory
        let dst = self.skills_dir.join(&name);
        if !dst.exists() {
            copy_dir::copy_dir(source, &dst)
                .map_err(|e| format!("copy skill to {}: {e}", dst.display()))?;
        }

        let skill = InstalledSkill {
            manifest,
            path: dst,
            enabled: true,
            loaded_at: chrono::Utc::now(),
        };

        let key = name.clone();
        self.skills.insert(key, skill);
        Ok(name)
    }

    /// Remove a skill by name.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if let Some(skill) = self.skills.remove(name) {
            let _ = std::fs::remove_dir_all(&skill.path);
            Ok(())
        } else {
            Err(format!("skill not found: {name}"))
        }
    }

    /// Enable a disabled skill.
    pub fn enable(&mut self, name: &str) -> Result<(), String> {
        self.skills
            .get_mut(name)
            .map(|s| s.enabled = true)
            .ok_or_else(|| format!("skill not found: {name}"))
    }

    /// Disable a skill without removing it.
    pub fn disable(&mut self, name: &str) -> Result<(), String> {
        self.skills
            .get_mut(name)
            .map(|s| s.enabled = false)
            .ok_or_else(|| format!("skill not found: {name}"))
    }

    /// List all installed skill names.
    pub fn list(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&InstalledSkill> {
        self.skills.get(name)
    }

    /// Get all enabled skills.
    pub fn enabled(&self) -> Vec<&InstalledSkill> {
        self.skills.values().filter(|s| s.enabled).collect()
    }

    /// Build the combined system prompt prefix from all enabled skills.
    pub fn build_prompt_prefix(&self) -> String {
        let mut parts = Vec::new();
        for skill in self.skills.values().filter(|s| s.enabled) {
            if let Some(system) = &skill.manifest.prompt.system {
                parts.push(format!("[{}] {}", skill.manifest.skill.name, system));
            }
        }
        parts.join("\n")
    }

    /// Collect all native tool names required by enabled skills.
    pub fn required_native_tools(&self) -> Vec<String> {
        let mut tools = Vec::new();
        for skill in self.skills.values().filter(|s| s.enabled) {
            for t in &skill.manifest.tools.native {
                if !tools.contains(t) {
                    tools.push(t.clone());
                }
            }
        }
        tools
    }

    pub fn count(&self) -> usize {
        self.skills.len()
    }
}

// Simple recursive directory copy (no extra dep)
mod copy_dir {
    use std::fs;
    use std::path::Path;

    pub fn copy_dir(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if ty.is_dir() {
                copy_dir(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}
