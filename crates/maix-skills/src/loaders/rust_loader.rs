//! Rust-native skill loader — loads skills from maix-skill.toml or SKILL.md manifests.

use crate::loader_registry::SkillEntry;
use crate::manifest::SkillManifest;

pub struct RustLoader;

impl Default for RustLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl RustLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn can_load(&self, entry: &SkillEntry) -> bool {
        entry.path.join("maix-skill.toml").exists() || entry.path.join("SKILL.md").exists()
    }

    /// Parse and validate the skill manifest from the entry's directory.
    pub fn load(&self, entry: &SkillEntry) -> Result<SkillManifest, String> {
        SkillManifest::from_dir(&entry.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_entry(name: &str, path: &str) -> SkillEntry {
        crate::loader_registry::SkillEntry {
            name: name.into(),
            version: "1.0.0".into(),
            path: PathBuf::from(path),
            loader_type: crate::loader_registry::LoaderType::Rust,
            enabled: true,
        }
    }

    #[test]
    fn test_can_load_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("maix-skill.toml"),
            "[skill]\nname = \"test\"\nversion = \"1.0\"\n",
        )
        .unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.can_load(&entry));
    }

    #[test]
    fn test_can_load_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: test\nversion: 1.0\n---\nBody",
        )
        .unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.can_load(&entry));
    }

    #[test]
    fn test_can_load_missing() {
        let loader = RustLoader::new();
        let entry = make_entry("test", "/nonexistent");
        assert!(!loader.can_load(&entry));
    }

    #[test]
    fn test_load_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("maix-skill.toml"),
            "[skill]\nname = \"my-skill\"\nversion = \"2.0\"\n",
        )
        .unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("my-skill", dir.path().to_str().unwrap());
        let manifest = loader.load(&entry).unwrap();
        assert_eq!(manifest.skill.name, "my-skill");
        assert_eq!(manifest.skill.version, "2.0");
    }

    #[test]
    fn test_load_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: md-skill\nversion: 1.0\n---\n## System Prompt\nHello",
        )
        .unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("md-skill", dir.path().to_str().unwrap());
        let manifest = loader.load(&entry).unwrap();
        assert_eq!(manifest.skill.name, "md-skill");
        assert_eq!(manifest.prompt.system.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_load_prefers_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: md-wins\nversion: 1.0\n---\nBody",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("maix-skill.toml"),
            "[skill]\nname = \"toml-loses\"\nversion = \"2.0\"\n",
        )
        .unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        let manifest = loader.load(&entry).unwrap();
        assert_eq!(manifest.skill.name, "md-wins");
    }

    #[test]
    fn test_load_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let loader = RustLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.load(&entry).is_err());
    }

    #[test]
    fn test_default() {
        let loader = RustLoader;
        assert!(!loader.can_load(&make_entry("x", "/nonexistent")));
    }
}
