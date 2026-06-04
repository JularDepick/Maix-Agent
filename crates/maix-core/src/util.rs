//! Shared utilities — key masking, sanitization, console init.

/// Set console to UTF-8 on Windows (fixes garbled output on GBK terminals).
pub fn init_console_utf8() {
    #[cfg(windows)]
    {
        extern "system" {
            fn SetConsoleOutputCP(code_page: u32) -> i32;
            fn SetConsoleCP(code_page: u32) -> i32;
        }
        unsafe {
            SetConsoleOutputCP(65001);
            SetConsoleCP(65001);
        }
    }
    #[cfg(not(windows))]
    let _ = ();
}

/// Mask an API key for safe logging (show first 4 chars only).
pub fn mask_key(key: &str) -> String {
    if key.len() <= 4 {
        return "***".into();
    }
    let prefix: String = key.chars().take(4).collect();
    format!("{prefix}...")
}

/// Sanitize a string that might contain API keys or secrets.
pub fn sanitize_for_log(s: &str) -> String {
    let mut result = s.to_string();
    // Replace bearer tokens
    let patterns = [
        ("Bearer ", "sk-"),
        ("Bearer ", "api-"),
        ("X-API-Key: ", ""),
        ("Authorization: Bearer ", ""),
    ];
    for (prefix, key_prefix) in &patterns {
        if let Some(pos) = result.find(prefix) {
            let start = pos + prefix.len();
            if result[start..].starts_with(key_prefix) {
                let end = result[start..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .map(|p| start + p)
                    .unwrap_or(result.len());
                result.replace_range(start..end, &mask_key(&result[start..end]));
            }
        }
    }
    result
}

/// Check if a string contains sensitive patterns (API keys, tokens).
pub fn contains_sensitive(s: &str) -> bool {
    let patterns = ["sk-", "api-key", "Bearer ", "eyJ", "-----BEGIN"];
    patterns.iter().any(|p| s.to_lowercase().contains(&p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("abc"), "***");
    }

    #[test]
    fn test_mask_key_normal() {
        let masked = mask_key("sk-1234567890abcdef");
        assert_eq!(masked, "sk-1...");
    }

    #[test]
    fn test_sanitize_bearer_token() {
        let log = "Authorization: Bearer sk-mytoken12345 extra";
        let sanitized = sanitize_for_log(log);
        assert!(!sanitized.contains("sk-mytoken12345"));
    }

    #[test]
    fn test_contains_sensitive() {
        assert!(contains_sensitive("my key is sk-abc123"));
        assert!(!contains_sensitive("hello world"));
    }
}
