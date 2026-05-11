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
