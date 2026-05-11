//! Independent executable skill loader — loads skills as external processes.

use crate::loader_registry::SkillEntry;
use std::path::PathBuf;

pub struct ExeLoader;

impl Default for ExeLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ExeLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn find_exe(&self, entry: &SkillEntry) -> Option<PathBuf> {
        let base = &entry.path;
        let candidates = [
            base.join("run"),
            base.join("run.exe"),
            base.join("run.sh"),
            base.join("main"),
            base.join("main.exe"),
        ];
        candidates.iter().find(|p| p.exists()).cloned()
    }

    pub fn can_load(&self, entry: &SkillEntry) -> bool {
        self.find_exe(entry).is_some()
    }
}
