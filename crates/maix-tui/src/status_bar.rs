#![allow(dead_code)]
//! Status bar — real-time display of model, tokens, cost, context, and git branch.

use std::time::Instant;

/// Status bar data and rendering.
pub struct StatusBar {
    pub model_name: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub context_used_pct: f64,
    pub git_branch: String,
    pub mode: String,
    pub cache_hit_rate: f32,
    pub last_update: Instant,
}

impl StatusBar {
    pub fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            context_used_pct: 0.0,
            git_branch: detect_git_branch(),
            mode: "agent".to_string(),
            cache_hit_rate: 0.0,
            last_update: Instant::now(),
        }
    }

    pub fn update_tokens(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.last_update = Instant::now();
    }

    pub fn update_cost(&mut self, cost: f64) {
        self.cost_usd += cost;
    }

    pub fn update_context(&mut self, used_pct: f64) {
        self.context_used_pct = used_pct;
    }

    pub fn update_cache(&mut self, hit_rate: f32) {
        self.cache_hit_rate = hit_rate;
    }

    pub fn set_mode(&mut self, mode: &str) {
        self.mode = mode.to_string();
    }

    pub fn refresh_branch(&mut self) {
        self.git_branch = detect_git_branch();
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Format left section: model and mode.
    pub fn format_left(&self) -> String {
        format!(" {} [{}]", self.model_name, self.mode)
    }

    /// Format center section: tokens and context.
    pub fn format_center(&self) -> String {
        format!(
            "{} in/{} out | ctx: {:.0}%",
            self.input_tokens, self.output_tokens, self.context_used_pct
        )
    }

    /// Format right section: cost, cache, and git branch.
    pub fn format_right(&self) -> String {
        let cache = if self.cache_hit_rate > 0.0 {
            format!(" | cache: {:.0}%", self.cache_hit_rate * 100.0)
        } else {
            String::new()
        };
        format!("${:.4}{} | {} ", self.cost_usd, cache, self.git_branch)
    }

    /// Full status line.
    pub fn format_line(&self, width: usize) -> String {
        let left = self.format_left();
        let center = self.format_center();
        let right = self.format_right();

        let left_len = left.len();
        let center_len = center.len();
        let right_len = right.len();

        if left_len + center_len + right_len + 4 >= width {
            return format!("{} {} {}", left, center, right);
        }

        let padding = width.saturating_sub(left_len + center_len + right_len);
        let left_pad = padding / 2;
        let right_pad = padding - left_pad;

        format!(
            "{}{}{}{}{}",
            left,
            " ".repeat(left_pad),
            center,
            " ".repeat(right_pad),
            right
        )
    }
}

fn detect_git_branch() -> String {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "no-git".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_new() {
        let sb = StatusBar::new("gpt-4o");
        assert_eq!(sb.model_name, "gpt-4o");
        assert_eq!(sb.total_tokens(), 0);
    }

    #[test]
    fn test_update_tokens() {
        let mut sb = StatusBar::new("claude-sonnet-4-20250514");
        sb.update_tokens(1000, 500);
        assert_eq!(sb.input_tokens, 1000);
        assert_eq!(sb.output_tokens, 500);
        assert_eq!(sb.total_tokens(), 1500);
    }

    #[test]
    fn test_update_cost() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.update_cost(0.05);
        sb.update_cost(0.03);
        assert!((sb.cost_usd - 0.08).abs() < 0.001);
    }

    #[test]
    fn test_format_left() {
        let sb = StatusBar::new("gpt-4o");
        assert!(sb.format_left().contains("gpt-4o"));
        assert!(sb.format_left().contains("[agent]"));
    }

    #[test]
    fn test_format_center() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.update_tokens(1000, 500);
        sb.update_context(45.5);
        let center = sb.format_center();
        assert!(center.contains("1000 in"));
        assert!(center.contains("500 out"));
        assert!(center.contains("46%"));
    }

    #[test]
    fn test_format_right() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.update_cost(0.1234);
        let right = sb.format_right();
        assert!(right.contains("$0.1234"));
    }

    #[test]
    fn test_format_right_with_cache() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.update_cost(0.05);
        sb.update_cache(0.725);
        let right = sb.format_right();
        assert!(right.contains("cache: 72%"));
    }

    #[test]
    fn test_format_line() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.update_tokens(500, 200);
        sb.update_cost(0.01);
        let line = sb.format_line(80);
        assert!(line.len() <= 80);
    }

    #[test]
    fn test_set_mode() {
        let mut sb = StatusBar::new("gpt-4o");
        sb.set_mode("yolo");
        assert_eq!(sb.mode, "yolo");
    }

    #[test]
    fn test_detect_git_branch() {
        let branch = detect_git_branch();
        // Should return something (either branch name or "no-git")
        assert!(!branch.is_empty());
    }
}
