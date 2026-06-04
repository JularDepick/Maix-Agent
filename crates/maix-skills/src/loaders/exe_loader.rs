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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_entry(name: &str, path: &str) -> SkillEntry {
        crate::loader_registry::SkillEntry {
            name: name.into(),
            version: "1.0.0".into(),
            path: PathBuf::from(path),
            loader_type: crate::loader_registry::LoaderType::Executable,
            enabled: true,
        }
    }

    #[test]
    fn test_find_exe_run_sh() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("run.sh"), "#!/bin/bash\necho hello").unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        let exe = loader.find_exe(&entry);
        assert!(exe.is_some());
        assert!(exe.unwrap().ends_with("run.sh"));
    }

    #[test]
    fn test_find_exe_run() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("run"), "#!/bin/bash\necho hello").unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        let exe = loader.find_exe(&entry);
        assert!(exe.is_some());
        assert!(exe.unwrap().ends_with("run"));
    }

    #[test]
    fn test_find_exe_main() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main"), "#!/bin/bash\necho hello").unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        let exe = loader.find_exe(&entry);
        assert!(exe.is_some());
        assert!(exe.unwrap().ends_with("main"));
    }

    #[test]
    fn test_find_exe_none() {
        let dir = tempfile::tempdir().unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.find_exe(&entry).is_none());
    }

    #[test]
    fn test_can_load_true() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("run.sh"), "#!/bin/bash\necho hello").unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(loader.can_load(&entry));
    }

    #[test]
    fn test_can_load_false() {
        let dir = tempfile::tempdir().unwrap();
        let loader = ExeLoader::new();
        let entry = make_entry("test", dir.path().to_str().unwrap());
        assert!(!loader.can_load(&entry));
    }

    #[test]
    fn test_default() {
        let loader = ExeLoader;
        assert!(!loader.can_load(&make_entry("x", "/nonexistent")));
    }
}
