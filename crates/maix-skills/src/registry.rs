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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill_dir(dir: &std::path::Path, name: &str, version: &str) {
        let toml = format!(
            "[skill]\nname = \"{name}\"\nversion = \"{version}\"\n"
        );
        std::fs::write(dir.join("maix-skill.toml"), toml).unwrap();
    }

    #[test]
    fn test_install_and_list() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        make_skill_dir(skill_src.path(), "my-skill", "1.0.0");

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        let name = reg.install(skill_src.path()).unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(reg.count(), 1);
        assert!(reg.list().contains(&"my-skill"));
    }

    #[test]
    fn test_remove() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        make_skill_dir(skill_src.path(), "removable", "1.0.0");

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();
        assert_eq!(reg.count(), 1);

        reg.remove("removable").unwrap();
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn test_remove_not_found() {
        let skills_dir = tempfile::tempdir().unwrap();
        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        assert!(reg.remove("nonexistent").is_err());
    }

    #[test]
    fn test_enable_disable() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        make_skill_dir(skill_src.path(), "toggle-skill", "1.0.0");

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();

        // Installed skills are enabled by default
        assert_eq!(reg.enabled().len(), 1);

        reg.disable("toggle-skill").unwrap();
        assert_eq!(reg.enabled().len(), 0);
        assert_eq!(reg.count(), 1); // still installed

        reg.enable("toggle-skill").unwrap();
        assert_eq!(reg.enabled().len(), 1);
    }

    #[test]
    fn test_enable_not_found() {
        let skills_dir = tempfile::tempdir().unwrap();
        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        assert!(reg.enable("nonexistent").is_err());
    }

    #[test]
    fn test_get() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        make_skill_dir(skill_src.path(), "gettable", "2.0.0");

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();

        let skill = reg.get("gettable").unwrap();
        assert_eq!(skill.manifest.skill.version, "2.0.0");
        assert!(skill.enabled);
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_build_prompt_prefix() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        let toml = r#"
[skill]
name = "prompt-skill"
version = "1.0"

[prompt]
system = "You are helpful."
"#;
        std::fs::write(skill_src.path().join("maix-skill.toml"), toml).unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();

        let prefix = reg.build_prompt_prefix();
        assert!(prefix.contains("prompt-skill"));
        assert!(prefix.contains("You are helpful."));
    }

    #[test]
    fn test_required_native_tools() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        let toml = r#"
[skill]
name = "tool-skill"
version = "1.0"

[tools]
native = ["read_file", "write_file"]
"#;
        std::fs::write(skill_src.path().join("maix-skill.toml"), toml).unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();

        let tools = reg.required_native_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_disable_not_found() {
        let skills_dir = tempfile::tempdir().unwrap();
        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        assert!(reg.disable("nonexistent").is_err());
    }

    #[test]
    fn test_build_prompt_prefix_disabled_excluded() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        let toml = r#"
[skill]
name = "disabled-skill"
version = "1.0"

[prompt]
system = "Should not appear."
"#;
        std::fs::write(skill_src.path().join("maix-skill.toml"), toml).unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();
        reg.disable("disabled-skill").unwrap();

        let prefix = reg.build_prompt_prefix();
        assert!(!prefix.contains("Should not appear."));
    }

    #[test]
    fn test_build_prompt_prefix_multiple_skills() {
        let skills_dir = tempfile::tempdir().unwrap();

        let src1 = tempfile::tempdir().unwrap();
        std::fs::write(src1.path().join("maix-skill.toml"),
            "[skill]\nname = \"skill-a\"\nversion = \"1.0\"\n[prompt]\nsystem = \"System A\"\n").unwrap();

        let src2 = tempfile::tempdir().unwrap();
        std::fs::write(src2.path().join("maix-skill.toml"),
            "[skill]\nname = \"skill-b\"\nversion = \"1.0\"\n[prompt]\nsystem = \"System B\"\n").unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(src1.path()).unwrap();
        reg.install(src2.path()).unwrap();

        let prefix = reg.build_prompt_prefix();
        assert!(prefix.contains("System A"));
        assert!(prefix.contains("System B"));
        assert!(prefix.contains("skill-a"));
        assert!(prefix.contains("skill-b"));
    }

    #[test]
    fn test_required_native_tools_dedup() {
        let skills_dir = tempfile::tempdir().unwrap();

        let src1 = tempfile::tempdir().unwrap();
        std::fs::write(src1.path().join("maix-skill.toml"),
            "[skill]\nname = \"tool-a\"\nversion = \"1.0\"\n[tools]\nnative = [\"read_file\", \"grep\"]\n").unwrap();

        let src2 = tempfile::tempdir().unwrap();
        std::fs::write(src2.path().join("maix-skill.toml"),
            "[skill]\nname = \"tool-b\"\nversion = \"1.0\"\n[tools]\nnative = [\"read_file\", \"write_file\"]\n").unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(src1.path()).unwrap();
        reg.install(src2.path()).unwrap();

        let tools = reg.required_native_tools();
        // read_file appears in both but should be deduped
        assert_eq!(tools.iter().filter(|t| t.as_str() == "read_file").count(), 1);
        assert!(tools.contains(&"grep".to_string()));
        assert!(tools.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_enabled_all_disabled() {
        let skills_dir = tempfile::tempdir().unwrap();
        let skill_src = tempfile::tempdir().unwrap();
        std::fs::write(skill_src.path().join("maix-skill.toml"),
            "[skill]\nname = \"only-skill\"\nversion = \"1.0\"\n").unwrap();

        let mut reg = SkillRegistry::new(skills_dir.path().to_path_buf());
        reg.install(skill_src.path()).unwrap();
        reg.disable("only-skill").unwrap();

        assert!(reg.enabled().is_empty());
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
