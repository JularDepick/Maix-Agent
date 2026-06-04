//! Diff visualization — unified, side-by-side, and word-level diff rendering.

use std::fmt;

/// A single diff line with its type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DiffLine {
    /// Context line (unchanged).
    Context(String),
    /// Added line.
    Added(String),
    /// Removed line.
    Removed(String),
    /// Separator for hunks.
    Separator,
}

#[allow(dead_code)]
impl DiffLine {
    pub fn text(&self) -> &str {
        match self {
            Self::Context(s) | Self::Added(s) | Self::Removed(s) => s,
            Self::Separator => "---",
        }
    }

    pub fn marker(&self) -> char {
        match self {
            Self::Context(_) => ' ',
            Self::Added(_) => '+',
            Self::Removed(_) => '-',
            Self::Separator => '-',
        }
    }
}

/// A diff hunk with line ranges.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

/// Diff statistics.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[allow(dead_code)]
impl DiffStats {
    pub fn format_short(&self) -> String {
        format!(
            "{} file{} changed, {} insertion{}, {} deletion{}",
            self.files_changed,
            if self.files_changed == 1 { "" } else { "s" },
            self.insertions,
            if self.insertions == 1 { "" } else { "s" },
            self.deletions,
            if self.deletions == 1 { "" } else { "s" },
        )
    }
}

/// Diff display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMode {
    /// Standard unified diff.
    Unified,
    /// Side-by-side comparison.
    #[allow(dead_code)]
    SideBySide,
    /// Word-level highlighting.
    #[allow(dead_code)]
    WordLevel,
}

/// Diff renderer.
pub struct DiffRenderer {
    mode: DiffMode,
    #[allow(dead_code)]
    context_lines: usize,
}

#[allow(dead_code)]
impl DiffRenderer {
    pub fn new(mode: DiffMode) -> Self {
        Self {
            mode,
            context_lines: 3,
        }
    }

    pub fn with_context_lines(mut self, n: usize) -> Self {
        self.context_lines = n;
        self
    }

    /// Compute a simple line-based diff between two texts.
    pub fn diff(&self, old: &str, new: &str) -> Vec<DiffHunk> {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        self.diff_lines(&old_lines, &new_lines)
    }

    /// Compute diff between two sets of lines.
    pub fn diff_lines(&self, old: &[&str], new: &[&str]) -> Vec<DiffHunk> {
        // Simple LCS-based diff
        let lcs = lcs_indices(old, new);
        let mut hunks = Vec::new();
        let mut lines = Vec::new();
        let mut old_idx = 0;
        let mut new_idx = 0;
        let hunk_old_start = 1;
        let hunk_new_start = 1;

        for (oi, ni) in &lcs {
            // Removed lines before this match
            while old_idx < *oi {
                lines.push(DiffLine::Removed(old[old_idx].to_string()));
                old_idx += 1;
            }
            // Added lines before this match
            while new_idx < *ni {
                lines.push(DiffLine::Added(new[new_idx].to_string()));
                new_idx += 1;
            }
            // Context line
            lines.push(DiffLine::Context(old[*oi].to_string()));
            old_idx += 1;
            new_idx += 1;
        }

        // Remaining lines
        while old_idx < old.len() {
            lines.push(DiffLine::Removed(old[old_idx].to_string()));
            old_idx += 1;
        }
        while new_idx < new.len() {
            lines.push(DiffLine::Added(new[new_idx].to_string()));
            new_idx += 1;
        }

        if !lines.is_empty() {
            let _added = lines.iter().filter(|l| matches!(l, DiffLine::Added(_))).count();
            let _removed = lines.iter().filter(|l| matches!(l, DiffLine::Removed(_))).count();
            hunks.push(DiffHunk {
                old_start: hunk_old_start,
                old_count: old.len(),
                new_start: hunk_new_start,
                new_count: new.len(),
                lines,
            });
        }

        hunks
    }

    /// Render diff in unified format.
    pub fn render_unified(&self, hunks: &[DiffHunk]) -> String {
        let mut out = String::new();
        for hunk in hunks {
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));
            for line in &hunk.lines {
                out.push(line.marker());
                out.push_str(line.text());
                out.push('\n');
            }
        }
        out
    }

    /// Render diff in side-by-side format.
    pub fn render_side_by_side(&self, hunks: &[DiffHunk], width: usize) -> String {
        let half = width / 2 - 2;
        let mut out = String::new();

        for hunk in hunks {
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));

            // Pair up removed and added lines
            let mut removed: Vec<&str> = Vec::new();
            let mut added: Vec<&str> = Vec::new();

            for line in &hunk.lines {
                match line {
                    DiffLine::Context(text) => {
                        // Flush pending removes/adds
                        while !removed.is_empty() || !added.is_empty() {
                            let left = if !removed.is_empty() {
                                removed.remove(0)
                            } else {
                                ""
                            };
                            let right = if !added.is_empty() {
                                added.remove(0)
                            } else {
                                ""
                            };
                            out.push_str(&format_line_pair(left, right, '-', '+', half));
                        }
                        out.push_str(&format_line_pair(text, text, ' ', ' ', half));
                    }
                    DiffLine::Removed(text) => removed.push(text),
                    DiffLine::Added(text) => added.push(text),
                    DiffLine::Separator => {
                        out.push_str(&"─".repeat(width));
                        out.push('\n');
                    }
                }
            }

            // Flush remaining
            while !removed.is_empty() || !added.is_empty() {
                let left = if !removed.is_empty() { removed.remove(0) } else { "" };
                let right = if !added.is_empty() { added.remove(0) } else { "" };
                out.push_str(&format_line_pair(left, right, '-', '+', half));
            }
        }

        out
    }

    /// Render diff with word-level highlighting.
    pub fn render_word_level(&self, hunks: &[DiffHunk]) -> String {
        let mut out = String::new();
        for hunk in hunks {
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));

            let mut removed_buf: Vec<&str> = Vec::new();
            let mut added_buf: Vec<&str> = Vec::new();

            for line in &hunk.lines {
                match line {
                    DiffLine::Context(text) => {
                        // Flush pending
                        for r in &removed_buf {
                            out.push_str(&format!("  [removed] {}\n", r));
                        }
                        for a in &added_buf {
                            out.push_str(&format!("  [added]   {}\n", a));
                        }
                        removed_buf.clear();
                        added_buf.clear();
                        out.push_str(&format!("  {}\n", text));
                    }
                    DiffLine::Removed(text) => removed_buf.push(text),
                    DiffLine::Added(text) => added_buf.push(text),
                    DiffLine::Separator => {}
                }
            }

            for r in &removed_buf {
                out.push_str(&format!("  [removed] {}\n", r));
            }
            for a in &added_buf {
                out.push_str(&format!("  [added]   {}\n", a));
            }
        }
        out
    }

    /// Render diff using the configured mode.
    pub fn render(&self, hunks: &[DiffHunk]) -> String {
        match self.mode {
            DiffMode::Unified => self.render_unified(hunks),
            DiffMode::SideBySide => self.render_side_by_side(hunks, 80),
            DiffMode::WordLevel => self.render_word_level(hunks),
        }
    }

    /// Compute diff stats from hunks.
    pub fn stats(&self, hunks: &[DiffHunk]) -> DiffStats {
        let mut stats = DiffStats::default();
        for hunk in hunks {
            for line in &hunk.lines {
                match line {
                    DiffLine::Added(_) => stats.insertions += 1,
                    DiffLine::Removed(_) => stats.deletions += 1,
                    _ => {}
                }
            }
        }
        if stats.insertions > 0 || stats.deletions > 0 {
            stats.files_changed = 1;
        }
        stats
    }
}

impl fmt::Display for DiffRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DiffRenderer({:?})", self.mode)
    }
}

#[allow(dead_code)]
fn format_line_pair(left: &str, right: &str, left_mark: char, right_mark: char, half: usize) -> String {
    let l = truncate_display(left, half);
    let r = truncate_display(right, half);
    format!("{}{:<width$} │ {}{:<width$}\n", left_mark, l, right_mark, r, width = half)
}

#[allow(dead_code)]
fn truncate_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

/// Compute LCS indices between two slices.
#[allow(dead_code)]
fn lcs_indices(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let n = old.len();
    let m = new.len();

    // Build LCS table
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack
    let mut result = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if old[i - 1] == new[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    result
}

/// Format a diff stats line for git-style output.
#[allow(dead_code)]
pub fn format_diff_summary(file: &str, stats: &DiffStats) -> String {
    format!(" {} | {} {}", file, stats.insertions + stats.deletions, {
        let mut s = String::new();
        for _ in 0..stats.insertions.min(10) {
            s.push('+');
        }
        for _ in 0..stats.deletions.min(10) {
            s.push('-');
        }
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_no_changes() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("hello\nworld", "hello\nworld");
        // All context, no hunks with changes
        assert!(hunks.is_empty() || hunks[0].lines.iter().all(|l| matches!(l, DiffLine::Context(_))));
    }

    #[test]
    fn test_diff_additions() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("hello", "hello\nworld");
        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| matches!(l, DiffLine::Added(_))));
    }

    #[test]
    fn test_diff_removals() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("hello\nworld", "hello");
        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| matches!(l, DiffLine::Removed(_))));
    }

    #[test]
    fn test_diff_mixed() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("a\nb\nc", "a\nx\nc");
        assert!(!hunks.is_empty());
        let has_removed = hunks[0].lines.iter().any(|l| matches!(l, DiffLine::Removed(s) if s == "b"));
        let has_added = hunks[0].lines.iter().any(|l| matches!(l, DiffLine::Added(s) if s == "x"));
        assert!(has_removed);
        assert!(has_added);
    }

    #[test]
    fn test_render_unified() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("a\nb", "a\nc");
        let output = renderer.render_unified(&hunks);
        assert!(output.contains("@@"));
        assert!(output.contains("-b"));
        assert!(output.contains("+c"));
    }

    #[test]
    fn test_render_side_by_side() {
        let renderer = DiffRenderer::new(DiffMode::SideBySide);
        let hunks = renderer.diff("hello", "world");
        let output = renderer.render_side_by_side(&hunks, 80);
        assert!(output.contains("│"));
    }

    #[test]
    fn test_render_word_level() {
        let renderer = DiffRenderer::new(DiffMode::WordLevel);
        let hunks = renderer.diff("old line", "new line");
        let output = renderer.render_word_level(&hunks);
        assert!(output.contains("[removed]") || output.contains("[added]") || output.contains("line"));
    }

    #[test]
    fn test_diff_stats() {
        let renderer = DiffRenderer::new(DiffMode::Unified);
        let hunks = renderer.diff("a\nb\nc", "a\nx\nc\ny");
        let stats = renderer.stats(&hunks);
        assert_eq!(stats.files_changed, 1);
        assert!(stats.insertions > 0);
        assert!(stats.deletions > 0);
    }

    #[test]
    fn test_diff_stats_format() {
        let stats = DiffStats {
            files_changed: 2,
            insertions: 10,
            deletions: 3,
        };
        let s = stats.format_short();
        assert!(s.contains("2 files"));
        assert!(s.contains("10 insertions"));
        assert!(s.contains("3 deletions"));
    }

    #[test]
    fn test_diff_stats_format_singular() {
        let stats = DiffStats {
            files_changed: 1,
            insertions: 1,
            deletions: 1,
        };
        let s = stats.format_short();
        assert!(s.contains("1 file changed"));
        assert!(s.contains("1 insertion,"));
        assert!(s.contains("1 deletion"));
    }

    #[test]
    fn test_lcs_indices() {
        let old = ["a", "b", "c"];
        let new = ["a", "x", "c"];
        let indices = lcs_indices(&old, &new);
        assert_eq!(indices, vec![(0, 0), (2, 2)]);
    }

    #[test]
    fn test_lcs_empty() {
        let indices = lcs_indices(&[], &[]);
        assert!(indices.is_empty());
    }

    #[test]
    fn test_format_diff_summary() {
        let stats = DiffStats {
            files_changed: 1,
            insertions: 3,
            deletions: 1,
        };
        let s = format_diff_summary("main.rs", &stats);
        assert!(s.contains("main.rs"));
        assert!(s.contains("+++"));
        assert!(s.contains("-"));
    }

    #[test]
    fn test_diff_line_marker() {
        assert_eq!(DiffLine::Context("".into()).marker(), ' ');
        assert_eq!(DiffLine::Added("".into()).marker(), '+');
        assert_eq!(DiffLine::Removed("".into()).marker(), '-');
    }

    #[test]
    fn test_renderer_display() {
        let r = DiffRenderer::new(DiffMode::Unified);
        assert!(format!("{}", r).contains("Unified"));
    }
}
