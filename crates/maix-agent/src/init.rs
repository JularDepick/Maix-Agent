//! Project initialization — scan project and generate MAIX.md.

use std::path::Path;

/// Detected project type.
#[derive(Debug, Clone, Copy)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust"),
            ProjectType::Node => write!(f, "Node.js"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect the project type from root directory markers.
pub fn detect_project_type(root: &Path) -> ProjectType {
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("package.json").exists() {
        ProjectType::Node
    } else if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        ProjectType::Python
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
        ProjectType::Java
    } else {
        ProjectType::Unknown
    }
}

/// Scan key project files and return their contents.
pub fn scan_project_files(root: &Path) -> String {
    let key_files = [
        "README.md",
        "README",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "setup.py",
        "go.mod",
        "pom.xml",
        "build.gradle",
        ".gitignore",
        "Makefile",
        "justfile",
        "Dockerfile",
        ".editorconfig",
    ];

    let mut context = String::new();
    for file in &key_files {
        let path = root.join(file);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                // Truncate large files
                let truncated = if content.len() > 2000 {
                    let end = content.char_indices().nth(2000).map(|(i, _)| i).unwrap_or(content.len());
                    format!("{}...\n(truncated)", &content[..end])
                } else {
                    content
                };
                context.push_str(&format!("=== {file} ===\n{truncated}\n\n"));
            }
        }
    }
    context
}

/// Build a directory tree string (2 levels deep).
pub fn build_dir_tree(root: &Path) -> String {
    let mut result = String::new();
    build_tree_recursive(root, "", 0, 2, &mut result);
    result
}

fn build_tree_recursive(
    dir: &Path,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    output: &mut String,
) {
    if depth >= max_depth {
        return;
    }

    let skip = [".git", "node_modules", "target", ".venv", "__pycache__", ".maix"];

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            !skip.contains(&name_str.as_ref())
        })
        .collect();

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i + 1 == entries.len();
        let connector = if is_last { "└── " } else { "├── " };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or_else(|_| {
            std::fs::metadata(entry.path())
                .map(|m| m.is_dir())
                .unwrap_or(false)
        });

        if is_dir {
            output.push_str(&format!("{prefix}{connector}{name_str}/\n"));
            let new_prefix = if is_last { "    " } else { "│   " };
            build_tree_recursive(
                &entry.path(),
                &format!("{prefix}{new_prefix}"),
                depth + 1,
                max_depth,
                output,
            );
        } else {
            output.push_str(&format!("{prefix}{connector}{name_str}\n"));
        }
    }
}

/// Generate a default MAIX.md template based on project type.
pub fn generate_maix_md(project_type: ProjectType, dir_tree: &str, key_files: &str) -> String {
    let build_cmds = match project_type {
        ProjectType::Rust => {
            "- Build: `cargo build`\n\
             - Test: `cargo test`\n\
             - Format: `cargo fmt`\n\
             - Lint: `cargo clippy`"
        }
        ProjectType::Node => {
            "- Install: `npm install`\n\
             - Build: `npm run build`\n\
             - Test: `npm test`\n\
             - Lint: `npm run lint`"
        }
        ProjectType::Python => {
            "- Install: `pip install -e .` or `poetry install`\n\
             - Test: `pytest`\n\
             - Format: `black .`\n\
             - Lint: `ruff check .`"
        }
        ProjectType::Go => {
            "- Build: `go build ./...`\n\
             - Test: `go test ./...`\n\
             - Format: `gofmt -w .`\n\
             - Lint: `golangci-lint run`"
        }
        ProjectType::Java => {
            "- Build: `mvn compile` or `./gradlew build`\n\
             - Test: `mvn test` or `./gradlew test`\n\
             - Format: `mvn spotless:apply`"
        }
        ProjectType::Unknown => {
            "- (auto-detect build commands from project files)"
        }
    };

    format!(
        r#"# Maix Project Instructions

## Project Overview
- Type: {project_type}
- Generated by: `maix init`

## Build Commands
{build_cmds}

## Project Structure
```
{dir_tree}```

## Key Files
{key_files}

## Coding Style
- Follow existing conventions in the codebase
- Use consistent naming patterns
- Keep functions focused and small

## Notes
- This file was auto-generated. Edit it to add project-specific instructions.
- Maix reads this file at the start of each session for context.
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_rust_project() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // The maix-agent crate itself is a Rust project
        let type_ = detect_project_type(&root);
        assert!(matches!(type_, ProjectType::Rust));
    }

    #[test]
    fn test_build_dir_tree() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let tree = build_dir_tree(&root);
        assert!(!tree.is_empty());
        // Should contain src/ directory
        assert!(tree.contains("src/"));
    }

    #[test]
    fn test_scan_project_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let files = scan_project_files(&root);
        assert!(files.contains("Cargo.toml"));
    }

    #[test]
    fn test_generate_maix_md() {
        let md = generate_maix_md(ProjectType::Rust, "src/\n  main.rs", "=== Cargo.toml ===\n...");
        assert!(md.contains("cargo build"));
        assert!(md.contains("Rust"));
    }
}
