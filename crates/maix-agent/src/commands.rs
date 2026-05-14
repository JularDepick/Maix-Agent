//! Custom slash commands from markdown templates.
//!
//! Commands are discovered from:
//! - `~/.maix/commands/*.md` → `/user:<name>`
//! - `{project}/.maix/commands/*.md` → `/project:<name>`

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CustomCommand {
    pub name: String,
    pub template: String,
    pub source: PathBuf,
}

/// Discover custom commands from user and project directories.
pub fn discover_commands(project_root: &Path, home: &Path) -> Vec<CustomCommand> {
    let mut commands = Vec::new();

    // User-level commands: ~/.maix/commands/*.md
    let user_dir = home.join(".maix").join("commands");
    discover_in_dir(&user_dir, "user:", &mut commands);

    // Project-level commands: {project}/.maix/commands/*.md
    let project_dir = project_root.join(".maix").join("commands");
    discover_in_dir(&project_dir, "project:", &mut commands);

    commands
}

fn discover_in_dir(dir: &Path, prefix: &str, commands: &mut Vec<CustomCommand>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let name = format!("{prefix}{stem}");
        let template = std::fs::read_to_string(&path).unwrap_or_default();
        commands.push(CustomCommand {
            name,
            template,
            source: path,
        });
    }
}

/// Render a custom command template, replacing `$ARGUMENTS` with the user input.
pub fn render_command(cmd: &CustomCommand, arguments: &str) -> String {
    cmd.template.replace("$ARGUMENTS", arguments)
}

/// List all custom command names (for tab completion).
pub fn list_command_names(project_root: &Path, home: &Path) -> Vec<String> {
    discover_commands(project_root, home)
        .into_iter()
        .map(|c| c.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_commands() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let project = dir.path().join("project");
        let user_cmd_dir = home.join(".maix").join("commands");
        let proj_cmd_dir = project.join(".maix").join("commands");
        fs::create_dir_all(&user_cmd_dir).unwrap();
        fs::create_dir_all(&proj_cmd_dir).unwrap();
        fs::write(user_cmd_dir.join("review.md"), "Review this:\n$ARGUMENTS").unwrap();
        fs::write(proj_cmd_dir.join("test.md"), "Run tests.").unwrap();

        let cmds = discover_commands(&project, &home);
        assert_eq!(cmds.len(), 2);
        let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"user:review"));
        assert!(names.contains(&"project:test"));
    }

    #[test]
    fn test_render_command() {
        let cmd = CustomCommand {
            name: "test".into(),
            template: "Review this code:\n$ARGUMENTS\nPlease check.".into(),
            source: PathBuf::new(),
        };
        let rendered = render_command(&cmd, "src/main.rs");
        assert!(rendered.contains("src/main.rs"));
        assert!(rendered.contains("Review this code:"));
        assert!(!rendered.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_render_no_arguments() {
        let cmd = CustomCommand {
            name: "test".into(),
            template: "Just a prompt.".into(),
            source: PathBuf::new(),
        };
        let rendered = render_command(&cmd, "anything");
        assert_eq!(rendered, "Just a prompt.");
    }
}
