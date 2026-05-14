//! MAIX.md hierarchy loader — equivalent to Claude Code's CLAUDE.md system.
//!
//! Loads project instructions from multiple locations, merging them into
//! the system prompt. Supports `#include` directives for modularity.
//!
//! Loading order (later entries override earlier):
//! 1. ~/.maix/MAIX.md — global user instructions
//! 2. {project_root}/MAIX.md — project-level conventions
//! 3. {parent_dirs}/MAIX.md — monorepo support (up to git root)
//! 4. {project_root}/.maix/MAIX.md — local project instructions (gitignored)
//! 5. Subdirectory MAIX.md — loaded dynamically when files in those dirs are accessed

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct MaixMdLoader {
    project_root: PathBuf,
    user_home: PathBuf,
}

impl MaixMdLoader {
    pub fn new(project_root: PathBuf, user_home: PathBuf) -> Self {
        Self {
            project_root,
            user_home,
        }
    }

    /// Load all MAIX.md files in hierarchy order, merged into a single string.
    /// The result is suitable for injection into the system prompt.
    pub fn load_all(&self, current_file: Option<&Path>) -> String {
        let mut sections = Vec::new();
        let mut visited = HashSet::new();

        // 1. Global user instructions
        let global = self.user_home.join("MAIX.md");
        if let Some(content) = self.load_file(&global, &mut visited) {
            sections.push(format!("## User Instructions (Global)\n{}", content));
        }

        // 2. Project root
        let project = self.project_root.join("MAIX.md");
        if let Some(content) = self.load_file(&project, &mut visited) {
            sections.push(format!("## Project Instructions\n{}", content));
        }

        // 3. Parent directories up to git root
        let git_root = self.find_git_root();
        let mut ancestor = self.project_root.parent();
        while let Some(dir) = ancestor {
            if Some(dir) == git_root.as_deref() {
                break;
            }
            let md = dir.join("MAIX.md");
            if let Some(content) = self.load_file(&md, &mut visited) {
                sections.push(format!("## Instructions ({})\n{}", dir.display(), content));
            }
            ancestor = dir.parent();
        }

        // 4. Local .maix/MAIX.md
        let local = self.project_root.join(".maix").join("MAIX.md");
        if let Some(content) = self.load_file(&local, &mut visited) {
            sections.push(format!("## Local Instructions\n{}", content));
        }

        // 5. Subdirectory MAIX.md (if current_file is in a subdirectory)
        if let Some(file_path) = current_file {
            if let Ok(relative) = file_path.strip_prefix(&self.project_root) {
                let mut dir = relative.parent();
                while let Some(d) = dir {
                    if d.as_os_str().is_empty() {
                        break;
                    }
                    let md = self.project_root.join(d).join("MAIX.md");
                    if let Some(content) = self.load_file(&md, &mut visited) {
                        sections.push(format!("## Instructions ({}/)\n{}", d.display(), content));
                    }
                    dir = d.parent();
                }
            }
        }

        sections.join("\n\n---\n\n")
    }

    /// Load a single file, processing `#include` directives recursively.
    /// Returns None if the file doesn't exist or can't be read.
    fn load_file(&self, path: &Path, visited: &mut HashSet<PathBuf>) -> Option<String> {
        let canon = path.canonicalize().ok()?;
        if visited.contains(&canon) {
            return None; // Avoid circular includes
        }
        visited.insert(canon);

        let content = std::fs::read_to_string(path).ok()?;

        // Process #include directives
        let mut result = String::new();
        for line in content.lines() {
            if let Some(include_path) = line.strip_prefix("#include ") {
                let include_path = include_path.trim();
                let resolved = if include_path.starts_with('/') || include_path.starts_with('\\') {
                    PathBuf::from(include_path)
                } else {
                    path.parent()
                        .unwrap_or(&self.project_root)
                        .join(include_path)
                };
                if let Some(included) = self.load_file(&resolved, visited) {
                    result.push_str(&included);
                    result.push('\n');
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        Some(result)
    }

    /// Walk up from project_root to find the git repository root.
    fn find_git_root(&self) -> Option<PathBuf> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&self.project_root)
            .output()
            .ok()?;

        if output.status.success() {
            let path = String::from_utf8(output.stdout).ok()?;
            Some(PathBuf::from(path.trim()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let md_path = dir.path().join("MAIX.md");
        fs::write(&md_path, "# Test\nUse Rust conventions.").unwrap();

        let loader = MaixMdLoader::new(dir.path().to_path_buf(), dir.path().to_path_buf());
        let result = loader.load_all(None);
        assert!(result.contains("Use Rust conventions."));
    }

    #[test]
    fn test_include_directive() {
        let dir = tempfile::tempdir().unwrap();
        let md_path = dir.path().join("MAIX.md");
        let included = dir.path().join("rules.md");
        fs::write(&included, "Always use `cargo fmt`.").unwrap();
        fs::write(&md_path, "#include rules.md\nEnd.").unwrap();

        let loader = MaixMdLoader::new(dir.path().to_path_buf(), dir.path().to_path_buf());
        let result = loader.load_all(None);
        assert!(result.contains("Always use `cargo fmt`."));
        assert!(result.contains("End."));
    }

    #[test]
    fn test_circular_include() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("A.md");
        let b = dir.path().join("B.md");
        fs::write(&a, "#include B.md\nFrom A").unwrap();
        fs::write(&b, "#include A.md\nFrom B").unwrap();

        // Test load_file directly (not load_all, which only loads MAIX.md)
        let loader = MaixMdLoader::new(dir.path().to_path_buf(), dir.path().to_path_buf());
        let mut visited = HashSet::new();
        let result = loader.load_file(&a, &mut visited).unwrap_or_default();
        // Should not hang; circular reference detected
        assert!(result.contains("From A") || result.contains("From B"));
    }

    #[test]
    fn test_hierarchy_merge() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let project = dir.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();

        fs::write(home.join("MAIX.md"), "Global: be concise.").unwrap();
        fs::write(project.join("MAIX.md"), "Project: use tabs.").unwrap();

        let loader = MaixMdLoader::new(project.clone(), home);
        let result = loader.load_all(None);
        assert!(result.contains("be concise"));
        assert!(result.contains("use tabs"));
    }
}
