//! Working directory sandbox validation.

use std::path::{Path, PathBuf};

/// Validates that a path is within the allowed working directory.
/// Prevents path traversal and unauthorized file access.
pub struct WorkDirSandbox {
    root: PathBuf,
}

impl WorkDirSandbox {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve and validate a path relative to the sandbox root.
    /// Returns the resolved absolute path if valid, or an error if the path escapes the sandbox.
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, String> {
        let candidate = self.root.join(relative);
        let canonical = candidate.canonicalize().map_err(|e| format!("path error: {e}"))?;
        let root_canonical = self.root.canonicalize().map_err(|e| format!("root error: {e}"))?;
        if canonical.starts_with(&root_canonical) {
            Ok(canonical)
        } else {
            Err("path escapes sandbox".into())
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_valid() {
        let sandbox = WorkDirSandbox::new(std::env::temp_dir());
        // Create the target path so canonicalize succeeds
        let test_path = std::env::temp_dir().join("maix-test-sandbox");
        let _ = std::fs::create_dir_all(&test_path);
        let result = sandbox.resolve(Path::new("maix-test-sandbox"));
        assert!(result.is_ok());
        let _ = std::fs::remove_dir_all(&test_path);
    }

    #[test]
    fn test_root_accessor() {
        let tmp = std::env::temp_dir();
        let sandbox = WorkDirSandbox::new(tmp.clone());
        assert_eq!(sandbox.root(), tmp);
    }
}
