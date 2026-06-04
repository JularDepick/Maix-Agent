//! Command handler implementations.

mod ask;
mod identity;
mod memory;
mod server;
mod session;
mod skill;
mod system;
mod task;
mod tool;

pub use ask::cmd_ask;
pub use identity::{cmd_architecture, cmd_identity};
pub use memory::cmd_memory;
pub use server::cmd_server;
pub use session::cmd_session;
pub use skill::cmd_skill;
pub use system::{cmd_config, cmd_cost, cmd_doctor, cmd_health, cmd_init, cmd_update};
pub use task::cmd_task;
pub use tool::cmd_tool;

use maix_core::proto::maix::core::v1 as pb;

pub(super) fn parse_agent_mode(mode: &str) -> pb::AgentMode {
    match mode.to_lowercase().as_str() {
        "plan" => pb::AgentMode::Plan,
        "agent" => pb::AgentMode::Agent,
        "yolo" => pb::AgentMode::Yolo,
        _ => pb::AgentMode::Unspecified,
    }
}

pub(super) fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub(super) fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

pub(super) fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

pub(super) async fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("maix-cli")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("build client: {e}"))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("download returned {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read download: {e}"))?;

    std::fs::write(dest, &bytes).map_err(|e| format!("write file: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_str_chinese() {
        // "你好" = 6 bytes
        assert_eq!(truncate_str("你好世界", 7), "你好");
        assert_eq!(truncate_str("你好世界", 6), "你好");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_parse_agent_mode() {
        assert_eq!(parse_agent_mode("plan"), pb::AgentMode::Plan);
        assert_eq!(parse_agent_mode("PLAN"), pb::AgentMode::Plan);
        assert_eq!(parse_agent_mode("agent"), pb::AgentMode::Agent);
        assert_eq!(parse_agent_mode("yolo"), pb::AgentMode::Yolo);
        assert_eq!(parse_agent_mode("unknown"), pb::AgentMode::Unspecified);
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn test_truncate_str_zero_bytes() {
        assert_eq!(truncate_str("hello", 0), "");
    }

    #[test]
    fn test_truncate_str_exact_boundary() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_format_number_single_digit() {
        assert_eq!(format_number(7), "7");
    }

    #[test]
    fn test_format_number_exactly_1000() {
        assert_eq!(format_number(1000), "1,000");
    }

    #[test]
    fn test_parse_agent_mode_empty() {
        assert_eq!(parse_agent_mode(""), pb::AgentMode::Unspecified);
    }

    #[test]
    fn test_truncate_str_mid_codepoint() {
        // "你好世界" = 4 chars * 3 bytes = 12 bytes
        // max_bytes=5 falls inside second char (bytes 3..5), should back up to byte 3
        assert_eq!(truncate_str("你好世界", 5), "你");
    }

    #[test]
    fn test_truncate_str_mixed_ascii_chinese() {
        // "a你好b" = 1 + 3 + 3 + 1 = 8 bytes
        assert_eq!(truncate_str("a你好b", 5), "a你");
        assert_eq!(truncate_str("a你好b", 4), "a你");
        assert_eq!(truncate_str("a你好b", 1), "a");
    }

    #[test]
    fn test_format_number_large() {
        assert_eq!(format_number(1_000_000_000), "1,000,000,000");
    }

    #[test]
    fn test_parse_agent_mode_case_insensitive() {
        assert_eq!(parse_agent_mode("Agent"), pb::AgentMode::Agent);
        assert_eq!(parse_agent_mode("YOLO"), pb::AgentMode::Yolo);
        assert_eq!(parse_agent_mode("Plan"), pb::AgentMode::Plan);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Create source structure
        std::fs::write(src.path().join("file1.txt"), "hello").unwrap();
        std::fs::create_dir_all(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub").join("file2.txt"), "world").unwrap();

        let dest = dst.path().join("copy");
        copy_dir_recursive(src.path(), &dest).unwrap();

        assert_eq!(std::fs::read_to_string(dest.join("file1.txt")).unwrap(), "hello");
        assert_eq!(std::fs::read_to_string(dest.join("sub").join("file2.txt")).unwrap(), "world");
    }

    #[test]
    fn test_copy_dir_recursive_empty() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let dest = dst.path().join("copy");
        copy_dir_recursive(src.path(), &dest).unwrap();
        assert!(dest.exists());
    }
}
