//! Multi-file search and replace tool.

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::path::PathBuf;

/// Multi-file search and replace.
pub struct MultiEditTool;

/// Result of a multi-edit operation.
#[derive(Debug)]
pub struct MultiEditResult {
    pub files_scanned: u32,
    pub files_modified: u32,
    pub replacements: u32,
}

impl MultiEditTool {
    /// Find files matching a glob pattern under root.
    fn find_files(root: &std::path::Path, pattern: &str) -> Vec<PathBuf> {
        let glob_pattern = if pattern.contains('/') || pattern.contains('\\') {
            root.join(pattern).to_string_lossy().to_string()
        } else {
            root.join("**").join(pattern).to_string_lossy().to_string()
        };

        glob::glob(&glob_pattern)
            .ok()
            .map(|paths| paths.filter_map(|p| p.ok()).filter(|p| p.is_file()).collect())
            .unwrap_or_default()
    }

    /// Count how many times `search` appears in `content`.
    fn count_matches(content: &str, search: &str, case_sensitive: bool) -> u32 {
        if case_sensitive {
            content.matches(search).count() as u32
        } else {
            let content_lower = content.to_lowercase();
            let search_lower = search.to_lowercase();
            content_lower.matches(&*search_lower).count() as u32
        }
    }

    /// Case-insensitive string replace.
    fn replace_insensitive(content: &str, search: &str, replace: &str) -> String {
        let mut result = String::new();
        let content_lower = content.to_lowercase();
        let search_lower = search.to_lowercase();
        let mut last = 0;

        for (i, _) in content_lower.match_indices(&*search_lower) {
            result.push_str(&content[last..i]);
            result.push_str(replace);
            last = i + content[i..].to_lowercase().find(&*search_lower).map(|j| j + search_lower.len()).unwrap_or(search.len());
        }
        result.push_str(&content[last..]);
        result
    }

    /// Execute a multi-edit operation.
    pub async fn execute(
        root: &std::path::Path,
        search: &str,
        replace: &str,
        file_pattern: &str,
        case_sensitive: bool,
        use_regex: bool,
        dry_run: bool,
    ) -> MaixResult<(MultiEditResult, Vec<String>)> {
        let files = Self::find_files(root, file_pattern);
        let mut result = MultiEditResult {
            files_scanned: 0,
            files_modified: 0,
            replacements: 0,
        };
        let mut preview_lines = Vec::new();

        let re = if use_regex {
            Some(regex::Regex::new(search)
                .map_err(|e| maix_core::MaixError::Tool(format!("Invalid regex: {e}")))?)
        } else {
            None
        };

        for file_path in &files {
            result.files_scanned += 1;

            let content = match tokio::fs::read_to_string(file_path).await {
                Ok(c) => c,
                Err(_) => continue, // skip binary/unreadable files
            };

            let new_content = if let Some(ref re) = re {
                if case_sensitive {
                    re.replace_all(&content, replace).to_string()
                } else {
                    let re_i = regex::Regex::new(&format!("(?i){search}"))
                        .map_err(|e| maix_core::MaixError::Tool(format!("Invalid regex: {e}")))?;
                    re_i.replace_all(&content, replace).to_string()
                }
            } else if case_sensitive {
                content.replace(search, replace)
            } else {
                Self::replace_insensitive(&content, search, replace)
            };

            if new_content == content {
                continue;
            }

            let count = Self::count_matches(&content, search, case_sensitive);
            result.replacements += count;

            let rel_path = file_path.strip_prefix(root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            if dry_run {
                preview_lines.push(format!("  {rel_path}: {count} replacement(s)"));
            } else {
                tokio::fs::write(file_path, &new_content).await
                    .map_err(maix_core::MaixError::Io)?;
                result.files_modified += 1;
                preview_lines.push(format!("  {rel_path}: {count} replacement(s) applied"));
            }
        }

        Ok((result, preview_lines))
    }
}

// ---------------------------------------------------------------------------
// Tool
// /// ---------------------------------------------------------------------------

#[async_trait]
impl Tool for MultiEditTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "multi_edit".into(),
            description: "Search and replace text across multiple files matching a glob pattern. Supports regex and dry-run preview mode.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "search": { "type": "string", "description": "Search text or regex pattern" },
                    "replace": { "type": "string", "description": "Replacement text" },
                    "file_pattern": { "type": "string", "description": "File glob pattern (e.g. '*.rs', '*.ts')" },
                    "regex": { "type": "boolean", "description": "Use regex (default: false)" },
                    "case_sensitive": { "type": "boolean", "description": "Case sensitive (default: true)" },
                    "dry_run": { "type": "boolean", "description": "Preview mode — don't modify files (default: false)" }
                },
                "required": ["search", "replace", "file_pattern"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let search = args["search"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'search'".into()))?;
        let replace = args["replace"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'replace'".into()))?;
        let file_pattern = args["file_pattern"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'file_pattern'".into()))?;
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(true);
        let use_regex = args["regex"].as_bool().unwrap_or(false);
        let dry_run = args["dry_run"].as_bool().unwrap_or(false);

        let (result, preview) = MultiEditTool::execute(
            &ctx.working_dir, search, replace, file_pattern, case_sensitive, use_regex, dry_run,
        ).await?;

        let mut lines = vec![
            format!("Scanned: {} files | Modified: {} | Replacements: {}", result.files_scanned, result.files_modified, result.replacements),
        ];

        if dry_run {
            lines[0] = format!("[DRY RUN] {}", lines[0]);
        }

        if !preview.is_empty() {
            lines.push("".to_string());
            lines.extend(preview);
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_matches() {
        assert_eq!(MultiEditTool::count_matches("hello world hello", "hello", true), 2);
        assert_eq!(MultiEditTool::count_matches("Hello world hello", "hello", true), 1);
        assert_eq!(MultiEditTool::count_matches("Hello world hello", "hello", false), 2);
    }

    #[test]
    fn test_replace_insensitive() {
        let result = MultiEditTool::replace_insensitive("Hello World hello", "hello", "hi");
        assert_eq!(result, "hi World hi");
    }
}
