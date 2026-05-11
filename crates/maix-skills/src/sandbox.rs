//! Skill process sandbox — working directory isolation and resource limits.

use std::path::{Path, PathBuf};

/// Sandbox configuration for a skill execution.
pub struct SkillSandbox {
    root: PathBuf,
    #[allow(dead_code)]
    max_memory_mb: u64,
    #[allow(dead_code)]
    max_cpu_secs: u64,
    network_allowed: bool,
}

impl SkillSandbox {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            max_memory_mb: 512,
            max_cpu_secs: 60,
            network_allowed: false,
        }
    }

    pub fn with_network(mut self) -> Self {
        self.network_allowed = true;
        self
    }

    pub fn with_memory_limit(mut self, mb: u64) -> Self {
        self.max_memory_mb = mb;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn network_allowed(&self) -> bool {
        self.network_allowed
    }

    /// Create a per-execution isolated working directory.
    pub fn create_work_dir(&self, skill_name: &str) -> Result<PathBuf, std::io::Error> {
        let dir = self.root.join(skill_name);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}
