//! Permission system — tool, skill, and path-level authorization (Phase 1.1).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Granular permission for a single resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Tool { name: String },
    Skill { name: String },
    PathRead(PathBuf),
    PathWrite(PathBuf),
    Shell,
    Network,
}

impl Permission {
    pub fn tool(name: &str) -> Self { Self::Tool { name: name.into() } }
    pub fn skill(name: &str) -> Self { Self::Skill { name: name.into() } }
    pub fn path_read(path: PathBuf) -> Self { Self::PathRead(path) }
    pub fn path_write(path: PathBuf) -> Self { Self::PathWrite(path) }
}

/// A set of granted permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    granted: HashSet<Permission>,
    /// When true, all tools are auto-granted (YOLO mode).
    auto_approve_all: bool,
    /// Restrict all file operations to this root.
    sandbox_root: Option<PathBuf>,
}

impl PermissionSet {
    pub fn new() -> Self { Self::default() }

    pub fn with_auto_approve(mut self, auto: bool) -> Self {
        self.auto_approve_all = auto;
        self
    }

    pub fn with_sandbox_root(mut self, root: PathBuf) -> Self {
        self.sandbox_root = Some(root);
        self
    }

    pub fn grant(&mut self, perm: Permission) { self.granted.insert(perm); }
    pub fn revoke(&mut self, perm: &Permission) { self.granted.remove(perm); }

    pub fn add_tool(&mut self, name: &str) { self.grant(Permission::tool(name)); }
    pub fn add_skill(&mut self, name: &str) { self.grant(Permission::skill(name)); }

    /// Check if a tool is allowed.
    pub fn can_use_tool(&self, name: &str) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::tool(name))
    }

    /// Check if a skill is allowed.
    pub fn can_use_skill(&self, name: &str) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::skill(name))
    }

    /// Check if a file read path is within the sandbox.
    pub fn can_read_path(&self, path: &Path) -> bool {
        if self.auto_approve_all { return true; }
        if self.granted.contains(&Permission::path_read(path.to_path_buf())) {
            return true;
        }
        self.is_within_sandbox(path)
    }

    /// Check if a file write path is within the sandbox.
    pub fn can_write_path(&self, path: &Path) -> bool {
        if self.auto_approve_all { return true; }
        if self.granted.contains(&Permission::path_write(path.to_path_buf())) {
            return true;
        }
        self.is_within_sandbox(path)
    }

    /// Check if shell execution is allowed.
    pub fn can_use_shell(&self) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::Shell)
    }

    /// Check if network access is allowed.
    pub fn can_use_network(&self) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::Network)
    }

    fn is_within_sandbox(&self, path: &Path) -> bool {
        if let Some(ref root) = self.sandbox_root {
            match path.canonicalize() {
                Ok(canon) => canon.starts_with(root),
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_permission() {
        let mut ps = PermissionSet::new();
        ps.add_tool("fs_read");
        assert!(ps.can_use_tool("fs_read"));
        assert!(!ps.can_use_tool("shell_exec"));
    }

    #[test]
    fn test_auto_approve() {
        let ps = PermissionSet::new().with_auto_approve(true);
        assert!(ps.can_use_tool("any_tool"));
        assert!(ps.can_use_shell());
        assert!(ps.can_use_network());
    }

    #[test]
    fn test_skill_permission() {
        let mut ps = PermissionSet::new();
        ps.add_skill("code-review");
        assert!(ps.can_use_skill("code-review"));
        assert!(!ps.can_use_skill("shell-tool"));
    }
}
