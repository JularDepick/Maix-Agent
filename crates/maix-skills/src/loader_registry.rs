//! Skill Loader Registry — scans skill directories and matches loaders to skill types.

use std::collections::HashMap;
use std::path::PathBuf;

/// Type of skill loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderType {
    Rust,
    Markdown,
    Executable,
    Bash,
}

/// Info about a discovered skill.
#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub loader_type: LoaderType,
    pub enabled: bool,
}

/// Registry that auto-scans skill directories and routes to correct loaders.
pub struct LoaderRegistry {
    skills: HashMap<String, SkillEntry>,
    scan_dirs: Vec<PathBuf>,
}

impl Default for LoaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            scan_dirs: Vec::new(),
        }
    }

    pub fn with_scan_dir(mut self, dir: PathBuf) -> Self {
        self.scan_dirs.push(dir);
        self
    }

    pub fn register(&mut self, entry: SkillEntry) {
        self.skills.insert(entry.name.clone(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<&SkillEntry> {
        self.skills.values().collect()
    }

    pub fn remove(&mut self, name: &str) -> Option<SkillEntry> {
        self.skills.remove(name)
    }

    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Scan registered directories for skills.
    pub fn scan(&mut self) -> Vec<SkillEntry> {
        let mut found = Vec::new();
        for dir in &self.scan_dirs.clone() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check for maix-skill.toml or SKILL.md
                        let manifest = path.join("maix-skill.toml");
                        let skill_md = path.join("SKILL.md");
                        if manifest.exists() || skill_md.exists() {
                            let name = path.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let loader_type = if manifest.exists() {
                                LoaderType::Executable
                            } else {
                                LoaderType::Markdown
                            };
                            let entry = SkillEntry {
                                name: name.clone(),
                                version: "0.1.0".into(),
                                path,
                                loader_type,
                                enabled: true,
                            };
                            self.skills.insert(name, entry.clone());
                            found.push(entry);
                        }
                    }
                }
            }
        }
        found
    }
}
