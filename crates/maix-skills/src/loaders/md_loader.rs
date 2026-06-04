//! SKILL.md declarative skill loader — loads prompt-only skills from markdown files.

use crate::loader_registry::SkillEntry;

pub struct MarkdownLoader;

impl Default for MarkdownLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn can_load(&self, entry: &SkillEntry) -> bool {
        entry.path.join("SKILL.md").exists()
    }

    pub fn load(&self, entry: &SkillEntry) -> Result<String, std::io::Error> {
        let path = entry.path.join("SKILL.md");
        std::fs::read_to_string(path)
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
            loader_type: crate::loader_registry::LoaderType::Markdown,
            enabled: true,
        }
    }

    #[test]
    fn test_can_load_with_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "---\nname: test\n---\nBody").unwrap();
        let loader = MarkdownLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.can_load(&entry));
    }

    #[test]
    fn test_can_load_without_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        let loader = MarkdownLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(!loader.can_load(&entry));
    }

    #[test]
    fn test_load_success() {
        let dir = tempfile::tempdir().unwrap();
        let content = "---\nname: test\nversion: 1.0\n---\n## System Prompt\nHello";
        std::fs::write(dir.path().join("SKILL.md"), content).unwrap();
        let loader = MarkdownLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        let result = loader.load(&entry).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_load_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let loader = MarkdownLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.load(&entry).is_err());
    }

    #[test]
    fn test_default() {
        let loader = MarkdownLoader;
        assert!(!loader.can_load(&make_entry("x", "/nonexistent")));
    }
}
