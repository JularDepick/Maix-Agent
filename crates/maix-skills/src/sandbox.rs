//! Skill process sandbox — working directory isolation and resource limits.

use std::path::{Path, PathBuf};

/// Sandbox configuration for a skill execution.
pub struct SkillSandbox {
    root: PathBuf,
    max_memory_mb: u64,
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

    pub fn with_cpu_limit(mut self, secs: u64) -> Self {
        self.max_cpu_secs = secs;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn network_allowed(&self) -> bool {
        self.network_allowed
    }

    pub fn max_memory_mb(&self) -> u64 {
        self.max_memory_mb
    }

    pub fn max_cpu_secs(&self) -> u64 {
        self.max_cpu_secs
    }

    /// Create a per-execution isolated working directory.
    pub fn create_work_dir(&self, skill_name: &str) -> Result<PathBuf, std::io::Error> {
        let dir = self.root.join(skill_name);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Spawn a command with resource limits enforced.
    ///
    /// CPU time is enforced via a tokio timeout wrapper.
    /// On Unix, memory limits are set via `setrlimit` in a `pre_exec` hook.
    pub async fn spawn_limited(
        &self,
        command: &str,
        args: &[&str],
        working_dir: &Path,
    ) -> Result<SandboxOutput, SandboxError> {
        use tokio::process::Command;

        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // On Unix, set memory limits via setrlimit
        #[cfg(unix)]
        {
            let max_mem = self.max_memory_mb;
            unsafe {
                cmd.pre_exec(move || {
                    set_memory_limit(max_mem);
                    Ok(())
                });
            }
        }

        let child = cmd.spawn().map_err(|e| SandboxError::Spawn(e.to_string()))?;

        let timeout = std::time::Duration::from_secs(self.max_cpu_secs);
        match tokio::time::timeout(timeout, wait_with_output(child)).await {
            Ok(result) => result,
            Err(_) => Err(SandboxError::Timeout(self.max_cpu_secs)),
        }
    }
}

/// Output from a sandboxed execution.
#[derive(Debug)]
pub struct SandboxOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Sandbox execution errors.
#[derive(Debug)]
pub enum SandboxError {
    Spawn(String),
    Io(String),
    Timeout(u64),
    OomKilled,
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Timeout(secs) => write!(f, "process timed out after {secs}s"),
            Self::OomKilled => write!(f, "process killed due to memory limit"),
        }
    }
}

async fn wait_with_output(
    child: tokio::process::Child,
) -> Result<SandboxOutput, SandboxError> {
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| SandboxError::Io(e.to_string()))?;

    Ok(SandboxOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Set memory limit via setrlimit (Unix only).
#[cfg(unix)]
fn set_memory_limit(max_mb: u64) {
    let limit = max_mb * 1024 * 1024;
    let rlimit = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };
    unsafe {
        // RLIMIT_AS limits the virtual memory (address space)
        libc::setrlimit(libc::RLIMIT_AS, &rlimit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new_defaults() {
        let sb = SkillSandbox::new(PathBuf::from("/tmp/skill"));
        assert_eq!(sb.root(), Path::new("/tmp/skill"));
        assert!(!sb.network_allowed());
        assert_eq!(sb.max_memory_mb(), 512);
        assert_eq!(sb.max_cpu_secs(), 60);
    }

    #[test]
    fn test_with_network() {
        let sb = SkillSandbox::new(PathBuf::from("/tmp")).with_network();
        assert!(sb.network_allowed());
    }

    #[test]
    fn test_with_memory_limit() {
        let sb = SkillSandbox::new(PathBuf::from("/tmp")).with_memory_limit(1024);
        assert_eq!(sb.max_memory_mb(), 1024);
    }

    #[test]
    fn test_with_cpu_limit() {
        let sb = SkillSandbox::new(PathBuf::from("/tmp")).with_cpu_limit(120);
        assert_eq!(sb.max_cpu_secs(), 120);
    }

    #[test]
    fn test_root_accessor() {
        let sb = SkillSandbox::new(PathBuf::from("/workspace/skills"));
        assert_eq!(sb.root().to_str().unwrap(), "/workspace/skills");
    }

    #[test]
    fn test_network_default_off() {
        let sb = SkillSandbox::new(PathBuf::from("/tmp"));
        assert!(!sb.network_allowed());
    }

    #[test]
    fn test_sandbox_output_debug() {
        let out = SandboxOutput {
            stdout: "hello".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let debug = format!("{:?}", out);
        assert!(debug.contains("hello"));
    }

    #[test]
    fn test_sandbox_error_display() {
        assert!(SandboxError::Spawn("test".into()).to_string().contains("spawn"));
        assert!(SandboxError::Timeout(30).to_string().contains("30s"));
        assert!(SandboxError::OomKilled.to_string().contains("memory"));
    }
}
