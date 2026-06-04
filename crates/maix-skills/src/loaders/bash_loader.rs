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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new() {
        let _loader = BashLoader::new();
    }

    #[test]
    fn test_default() {
        let _loader = BashLoader;
    }

    #[test]
    fn test_interpreter() {
        let loader = BashLoader::new();
        let interp = loader.interpreter();
        if cfg!(target_os = "windows") {
            assert_eq!(interp, "bash");
        } else {
            assert_eq!(interp, "/bin/bash");
        }
    }

    #[test]
    fn test_can_load_missing() {
        let loader = BashLoader::new();
        let entry = crate::loader_registry::SkillEntry {
            name: "test".into(),
            version: "1.0.0".into(),
            path: PathBuf::from("/nonexistent/skill"),
            loader_type: crate::loader_registry::LoaderType::Bash,
            enabled: true,
        };
        assert!(!loader.can_load(&entry));
    }
}
