//! Diff utilities — compute diff statistics and enhanced diff output.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Statistics about a diff.
#[derive(Debug)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub hunks: usize,
}

impl DiffStats {
    pub fn format(&self) -> String {
        format!(
            "{} file(s) changed, {} insertion(+), {} deletion(-)",
            self.files_changed, self.insertions, self.deletions
        )
    }
}

/// Compute diff statistics between two strings.
pub fn compute_diff_stats(old: &str, new: &str) -> DiffStats {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let max_len = old_lines.len().max(new_lines.len());
    let mut insertions = 0usize;
    let mut deletions = 0usize;
    let mut hunks = 0usize;
    let mut in_hunk = false;

    for i in 0..max_len {
        let ol = old_lines.get(i).copied();
        let nl = new_lines.get(i).copied();

        match (ol, nl) {
            (Some(o), Some(n)) => {
                if o != n {
                    deletions += 1;
                    insertions += 1;
                    if !in_hunk {
                        hunks += 1;
                        in_hunk = true;
                    }
                } else {
                    in_hunk = false;
                }
            }
            (Some(_), None) => {
                deletions += 1;
                if !in_hunk {
                    hunks += 1;
                    in_hunk = true;
                }
            }
            (None, Some(_)) => {
                insertions += 1;
                if !in_hunk {
                    hunks += 1;
                    in_hunk = true;
                }
            }
            (None, None) => {}
        }
    }

    DiffStats {
        files_changed: 1,
        insertions,
        deletions,
        hunks,
    }
}

/// Generate a side-by-side diff view.
pub fn side_by_side_diff(old: &str, new: &str, width: usize) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let half = (width - 7) / 2;

    let mut result = String::new();
    result.push_str(&format!(
        "{:<width$} | {}\n",
        "--- OLD",
        "+++ NEW",
        width = half + 2
    ));
    result.push_str(&format!("{}\n", "-".repeat(width)));

    let max_len = old_lines.len().max(new_lines.len());
    for i in 0..max_len {
        let ol = old_lines.get(i).copied().unwrap_or("");
        let nl = new_lines.get(i).copied().unwrap_or("");

        let left = if ol.len() > half {
            format!("{}…", &ol[..half - 1])
        } else {
            format!("{:<width$}", ol, width = half)
        };

        let right = if nl.len() > half {
            format!("{}…", &nl[..half - 1])
        } else {
            nl.to_string()
        };

        let marker = if ol != nl { "|" } else { " " };
        result.push_str(&format!("{} {} {}\n", left, marker, right));
    }

    result
}

/// Generate a word-level diff highlighting changes within lines.
pub fn word_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max_len = old_lines.len().max(new_lines.len());

    let mut result = String::new();

    for i in 0..max_len {
        let ol = old_lines.get(i).copied().unwrap_or("");
        let nl = new_lines.get(i).copied().unwrap_or("");

        if ol == nl {
            result.push_str(&format!("  {}\n", ol));
        } else {
            // Show word-level changes
            let old_words: Vec<&str> = ol.split_whitespace().collect();
            let new_words: Vec<&str> = nl.split_whitespace().collect();

            result.push_str(&format!("- {}\n", ol));
            result.push_str(&format!("+ {}\n", nl));

            // Mark changed words
            let mut marker = String::from("  ");
            let max_words = old_words.len().max(new_words.len());
            for j in 0..max_words {
                let ow = old_words.get(j).copied().unwrap_or("");
                let nw = new_words.get(j).copied().unwrap_or("");
                if ow != nw {
                    marker.push_str(&"~".repeat(nw.len().max(ow.len())));
                    marker.push(' ');
                } else {
                    marker.push_str(&" ".repeat(ow.len() + 1));
                }
            }
            if marker.trim().len() > 2 {
                result.push_str(&format!("{}\n", marker));
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Compute diff statistics between two texts.
pub struct DiffStatsTool;

#[async_trait]
impl Tool for DiffStatsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "diff_stats".into(),
            description: "Compute diff statistics (insertions, deletions, hunks) between old and new text.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "old_text": { "type": "string", "description": "Original text" },
                    "new_text": { "type": "string", "description": "Modified text" }
                },
                "required": ["old_text", "new_text"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let old_text = args["old_text"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'old_text'".into()))?;
        let new_text = args["new_text"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'new_text'".into()))?;

        let stats = compute_diff_stats(old_text, new_text);

        let mut lines = vec![
            stats.format(),
            format!("Hunks: {}", stats.hunks),
            "".to_string(),
        ];

        // Show word diff for the first few changed lines
        let wd = word_diff(old_text, new_text);
        let preview: String = wd.lines().take(30).collect::<Vec<_>>().join("\n");
        lines.push(preview);

        Ok(lines.join("\n"))
    }
}

/// Estimate token count for text using simple heuristics.
pub struct TokenEstimateTool;

#[async_trait]
impl Tool for TokenEstimateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "token_estimate".into(),
            description: "Estimate the token count for a given text. Uses character-based estimation (~4 chars per token for English, ~2 for CJK).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text to estimate tokens for" },
                    "model": { "type": "string", "description": "Model name for estimation (default: 'gpt-4')" }
                },
                "required": ["text"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let text = args["text"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'text'".into()))?;
        let _model = args["model"].as_str().unwrap_or("gpt-4");

        let chars = text.chars().count();
        let words = text.split_whitespace().count();
        let lines = text.lines().count();

        // Estimate tokens: ~4 chars per token for English, ~2 for CJK
        let cjk_chars = text.chars().filter(|c| {
            let cp = *c as u32;
            (0x4E00..=0x9FFF).contains(&cp)   // CJK Unified
                || (0x3400..=0x4DBF).contains(&cp) // CJK Extension A
                || (0x3000..=0x303F).contains(&cp) // CJK Symbols
                || (0xFF00..=0xFFEF).contains(&cp) // Fullwidth
        }).count();
        let non_cjk_chars = chars.saturating_sub(cjk_chars);

        let estimated_tokens = (non_cjk_chars / 4) + (cjk_chars / 2);

        // Cost estimation (DeepSeek pricing: ~$0.27/M input, $1.10/M output)
        let input_cost = estimated_tokens as f64 * 0.27 / 1_000_000.0;
        let output_cost_est = estimated_tokens as f64 * 1.10 / 1_000_000.0;

        Ok(format!(
            "Token estimate:\n  Characters: {}\n  Words: {}\n  Lines: {}\n  CJK characters: {}\n  Estimated tokens: ~{}\n  Est. input cost: ${:.6}\n  Est. output cost: ${:.6}",
            chars, words, lines, cjk_chars, estimated_tokens, input_cost, output_cost_est
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_stats_no_change() {
        let stats = compute_diff_stats("hello\nworld", "hello\nworld");
        assert_eq!(stats.insertions, 0);
        assert_eq!(stats.deletions, 0);
    }

    #[test]
    fn test_diff_stats_with_changes() {
        let stats = compute_diff_stats("hello\nworld", "hello\nuniverse");
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.deletions, 1);
    }

    #[test]
    fn test_diff_stats_additions() {
        let stats = compute_diff_stats("hello", "hello\nworld");
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.deletions, 0);
    }

    #[test]
    fn test_side_by_side() {
        let result = side_by_side_diff("hello\nworld", "hello\nuniverse", 60);
        assert!(result.contains("OLD"));
        assert!(result.contains("NEW"));
    }

    #[test]
    fn test_word_diff() {
        let result = word_diff("hello world", "hello universe");
        assert!(result.contains("- hello world"));
        assert!(result.contains("+ hello universe"));
    }
}
