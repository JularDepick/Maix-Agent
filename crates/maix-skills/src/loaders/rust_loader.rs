//! Rust-native skill loader — loads skills compiled into the maix binary.

use crate::loader_registry::SkillEntry;

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
        entry.path.join("maix-skill.toml").exists()
    }

    pub fn load(&self, _entry: &SkillEntry) -> Result<(), String> {
        Ok(())
    }
}
