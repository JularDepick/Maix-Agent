//! Search tools: grep, glob.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{normalize_path, simple_glob_match, RiskLevel, Tool, ToolCtx, ToolDef};

// ---------------------------------------------------------------------------
// grep
// ---------------------------------------------------------------------------

pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "grep".into(),
            description: "Search file contents using regex patterns. Returns matching file paths or matching lines with context.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory to search in (default: .)" },
                    "glob": { "type": "string", "description": "File glob filter, e.g. \"*.rs\" or \"*.{ts,tsx}\"" },
                    "output_mode": { "type": "string", "description": "Output mode: \"files_with_matches\" (default) or \"content\"" },
                    "head_limit": { "type": "integer", "description": "Max results (default: 250)" }
                },
                "required": ["pattern"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let pattern_str = args["pattern"].as_str().unwrap_or_default();
        let path_str = args["path"].as_str().unwrap_or(".");
        let glob_filter = args["glob"].as_str();
        let output_mode = args["output_mode"].as_str().unwrap_or("files_with_matches");
        let head_limit = args["head_limit"].as_u64().unwrap_or(250) as usize;

        // Input validation
        if pattern_str.is_empty() {
            return Err(maix_core::MaixError::Tool("grep: pattern is required".into()));
        }
        if pattern_str.len() > 1000 {
            return Err(maix_core::MaixError::Tool("grep: pattern too long (max 1KB)".into()));
        }
        if head_limit == 0 || head_limit > 10_000 {
            return Err(maix_core::MaixError::Tool("grep: head_limit must be 1-10000".into()));
        }

        let re = regex::Regex::new(pattern_str)
            .map_err(|e| {
                let suggestion = crate::suggest_fix("grep", &e.to_string());
                maix_core::MaixError::Tool(format!("grep: invalid regex: {e}\n{suggestion}"))
            })?;

        // Sandbox: resolve path and ensure it stays within working_dir
        let root = {
            let candidate = ctx.working_dir.join(path_str);
            let canonical = candidate.canonicalize().unwrap_or_else(|_| {
                // For non-existent paths, canonicalize parent and rejoin
                let parent = candidate.parent().unwrap_or(&candidate);
                let file_name = candidate.file_name();
                parent.canonicalize().ok()
                    .and_then(|p| file_name.map(|f| p.join(f)))
                    .unwrap_or(candidate)
            });
            let work_canonical = ctx.working_dir.canonicalize().unwrap_or_else(|_| ctx.working_dir.clone());
            if !canonical.starts_with(&work_canonical) {
                return Err(maix_core::MaixError::Tool("grep: path escapes working directory".into()));
            }
            normalize_path(&canonical)
        };

        let skip_dirs: &[&str] = &[".git", "node_modules", "target", ".venv", "__pycache__"];
        let mut results: Vec<String> = Vec::new();

        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if results.len() >= head_limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                if results.len() >= head_limit {
                    break;
                }
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if file_type.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                // Apply glob filter (match against path relative to working_dir)
                if let Some(glob_pat) = glob_filter {
                    let match_path = entry.path().strip_prefix(&ctx.working_dir)
                        .unwrap_or(&entry.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    if !simple_glob_match(glob_pat, &match_path) {
                        continue;
                    }
                }

                // Read file, skip binary
                let raw = match std::fs::read(entry.path()) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if !raw.is_empty() && raw[..raw.len().min(8192)].contains(&0) {
                    continue;
                }
                let content = match String::from_utf8(raw) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let rel_path = entry.path().strip_prefix(&root)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                if output_mode == "files_with_matches" {
                    if re.is_match(&content) {
                        results.push(rel_path.to_string());
                    }
                } else {
                    for (line_no, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            results.push(format!("{}:{}: {}", rel_path, line_no + 1, line));
                            if results.len() >= head_limit {
                                break;
                            }
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No matches found for pattern: {pattern_str}"))
        } else {
            let mut out = results.join("\n");
            if results.len() >= head_limit {
                out.push_str(&format!("\n(truncated at {head_limit} results)"));
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// glob
// ---------------------------------------------------------------------------

pub struct GlobTool;

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "glob".into(),
            description: "Find files by glob pattern (e.g. \"**/*.rs\", \"src/**/*.ts\"). Returns matching file paths sorted by modification time.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern, e.g. \"**/*.rs\" or \"src/**/*.ts\"" },
                    "path": { "type": "string", "description": "Base directory (default: .)" },
                    "head_limit": { "type": "integer", "description": "Max results (default: 250)" }
                },
                "required": ["pattern"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let pattern_str = args["pattern"].as_str().unwrap_or_default();
        let path_str = args["path"].as_str().unwrap_or(".");
        let head_limit = args["head_limit"].as_u64().unwrap_or(250) as usize;

        // Input validation
        if pattern_str.is_empty() {
            return Err(maix_core::MaixError::Tool("glob: pattern is required".into()));
        }
        if pattern_str.len() > 500 {
            return Err(maix_core::MaixError::Tool("glob: pattern too long (max 500 chars)".into()));
        }
        if head_limit == 0 || head_limit > 10_000 {
            return Err(maix_core::MaixError::Tool("glob: head_limit must be 1-10000".into()));
        }

        // Sandbox: resolve path and ensure it stays within working_dir
        let root = {
            let candidate = ctx.working_dir.join(path_str);
            let canonical = candidate.canonicalize().unwrap_or_else(|_| {
                let parent = candidate.parent().unwrap_or(&candidate);
                let file_name = candidate.file_name();
                parent.canonicalize().ok()
                    .and_then(|p| file_name.map(|f| p.join(f)))
                    .unwrap_or(candidate)
            });
            let work_canonical = ctx.working_dir.canonicalize().unwrap_or_else(|_| ctx.working_dir.clone());
            if !canonical.starts_with(&work_canonical) {
                return Err(maix_core::MaixError::Tool("glob: path escapes working directory".into()));
            }
            normalize_path(&canonical)
        };

        let skip_dirs: &[&str] = &[".git", "node_modules", "target", ".venv", "__pycache__"];
        let mut paths: Vec<(std::time::SystemTime, String)> = Vec::new();

        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if paths.len() >= head_limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                if paths.len() >= head_limit {
                    break;
                }
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if file_type.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                // Match path relative to working_dir so patterns like
                // "crates/maix-tools/**/*.rs" match correctly
                let match_path = entry.path().strip_prefix(&ctx.working_dir)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                // Display path relative to root
                let rel = entry.path().strip_prefix(&root)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");

                if simple_glob_match(pattern_str, &match_path) {
                    let mtime = entry.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    paths.push((mtime, rel));
                }
            }
        }

        // Sort by modification time, newest first
        paths.sort_by_key(|b| std::cmp::Reverse(b.0));

        if paths.is_empty() {
            Ok(format!("No files matched pattern: {pattern_str}"))
        } else {
            let count = paths.len();
            let list: Vec<String> = paths.into_iter().map(|(_, p)| p).collect();
            Ok(format!("{} files matched:\n{}", count, list.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_double_star_basic() {
        assert!(simple_glob_match("**/*.rs", "src/lib.rs"));
        assert!(simple_glob_match("**/*.rs", "lib.rs"));
        assert!(simple_glob_match("**/*.rs", "a/b/c/d.rs"));
        assert!(!simple_glob_match("**/*.rs", "a/b/c/d.txt"));
    }

    #[test]
    fn glob_double_star_with_prefix() {
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/lib.rs"));
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/git.rs"));
        assert!(simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-tools/src/mcp/client.rs"));
        assert!(!simple_glob_match("crates/maix-tools/**/*.rs", "crates/maix-core/src/lib.rs"));
    }

    #[test]
    fn glob_single_star() {
        assert!(simple_glob_match("*.rs", "lib.rs"));
        assert!(!simple_glob_match("*.rs", "src/lib.rs"));
        assert!(simple_glob_match("src/*.rs", "src/lib.rs"));
        assert!(!simple_glob_match("src/*.rs", "src/sub/lib.rs"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(simple_glob_match("?.rs", "a.rs"));
        assert!(!simple_glob_match("?.rs", "ab.rs"));
        assert!(!simple_glob_match("?.rs", "/.rs"));
    }

    #[test]
    fn glob_brace_alternatives() {
        assert!(simple_glob_match("*.{rs,toml}", "lib.rs"));
        assert!(simple_glob_match("*.{rs,toml}", "Cargo.toml"));
        assert!(!simple_glob_match("*.{rs,toml}", "lib.txt"));
    }

    #[test]
    fn glob_double_star_only() {
        assert!(simple_glob_match("**", "anything/at/all"));
        assert!(simple_glob_match("**", "file.txt"));
    }
}
