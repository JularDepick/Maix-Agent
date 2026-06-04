//! Helper functions for the App.

use std::path::PathBuf;

/// Truncate a string to max_chars characters, safe on UTF-8 boundaries.
#[allow(dead_code)]
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(3).max(1);
    let truncated: String = s.chars().take(keep).collect();
    format!("{truncated}...")
}

/// Format byte size with smart unit.
#[allow(dead_code)]
pub fn format_size(bytes: usize) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Levenshtein distance for string similarity.
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0usize; len2 + 1]; len1 + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(len1 + 1) {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate().take(len2 + 1) {
        *cell = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}

/// Parse a duration string like "5m", "30s", "1h", "2d" into a Duration.
#[allow(dead_code)]
pub fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "s" | "S" => Some(std::time::Duration::from_secs(num)),
        "m" | "M" => Some(std::time::Duration::from_secs(num * 60)),
        "h" | "H" => Some(std::time::Duration::from_secs(num * 3600)),
        "d" | "D" => Some(std::time::Duration::from_secs(num * 86400)),
        _ => None,
    }
}

/// Suggest a fix for common error patterns.
pub fn suggest_fix(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("connection refused") || lower.contains("connect") {
        "检查服务是否运行: /health"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "请求超时，请稍后重试或检查网络连接"
    } else if lower.contains("unauthorized") || lower.contains("401") || lower.contains("403") {
        "认证失败，请检查 API 密钥配置"
    } else if lower.contains("rate limit") || lower.contains("429") {
        "请求频率过高，请稍后重试"
    } else if lower.contains("not found") || lower.contains("404") {
        "资源不存在，请检查路径或 ID"
    } else if lower.contains("model") && lower.contains("not") {
        "模型不可用，使用 /model 查看可用模型"
    } else if lower.contains("context") && lower.contains("length") {
        "上下文过长，使用 /compact 压缩上下文"
    } else if lower.contains("memory") || lower.contains("oom") {
        "内存不足，尝试 /clear 清空对话"
    } else {
        ""
    }
}

/// Get the home directory.
pub fn dirs_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
        assert_eq!(truncate_str("你好世界", 3), "你...");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1048576), "1.0MB");
        assert_eq!(format_size(1073741824), "1.0GB");
        assert_eq!(format_size(512), "512B");
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5m"), Some(std::time::Duration::from_secs(300)));
        assert_eq!(parse_duration("30s"), Some(std::time::Duration::from_secs(30)));
        assert_eq!(parse_duration("1h"), Some(std::time::Duration::from_secs(3600)));
        assert_eq!(parse_duration("2d"), Some(std::time::Duration::from_secs(172800)));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("5x"), None);
    }

    #[test]
    fn test_suggest_fix() {
        assert_eq!(suggest_fix("connection refused"), "检查服务是否运行: /health");
        assert_eq!(suggest_fix("request timeout"), "请求超时，请稍后重试或检查网络连接");
        assert_eq!(suggest_fix("unknown error"), "");
    }
}
