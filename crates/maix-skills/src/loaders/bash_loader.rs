//! Bash script loader — loads bash scripts that can bootstrap Python/Node/other runtimes.

use crate::loader_registry::SkillEntry;
use std::path::PathBuf;

pub struct BashLoader;

impl Default for BashLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl BashLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn find_script(&self, entry: &SkillEntry) -> Option<PathBuf> {
        let candidates = [
            entry.path.join("run.sh"),
            entry.path.join("install.sh"),
            entry.path.join("skill.sh"),
        ];
        candidates.iter().find(|p| p.exists()).cloned()
    }

    pub fn can_load(&self, entry: &SkillEntry) -> bool {
        self.find_script(entry).is_some()
    }

    pub fn interpreter(&self) -> &str {
        if cfg!(target_os = "windows") { "bash" } else { "/bin/bash" }
    }
}
