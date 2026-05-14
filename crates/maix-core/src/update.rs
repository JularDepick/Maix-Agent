//! Update checker — checks GitHub releases for new versions.

use std::time::{Duration, Instant};

/// Update information.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub download_url: String,
}

/// Checks for updates via GitHub releases API.
pub struct UpdateChecker {
    current_version: String,
    repo: String,
    check_interval: Duration,
    last_check: Option<Instant>,
    last_result: Option<Option<UpdateInfo>>,
}

impl UpdateChecker {
    pub fn new(current_version: &str) -> Self {
        Self {
            current_version: current_version.to_string(),
            repo: "JularDepick/Maix-Agent".to_string(),
            check_interval: Duration::from_secs(3600),
            last_check: None,
            last_result: None,
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn needs_check(&self) -> bool {
        match self.last_check {
            Some(last) => last.elapsed() >= self.check_interval,
            None => true,
        }
    }

    pub fn mark_checked(&mut self) {
        self.last_check = Some(Instant::now());
    }

    pub fn set_result(&mut self, result: Option<UpdateInfo>) {
        self.last_result = Some(result);
    }

    pub fn cached_result(&self) -> Option<&UpdateInfo> {
        self.last_result.as_ref().and_then(|r| r.as_ref())
    }

    pub fn format_update_notice(info: &UpdateInfo) -> String {
        format!(
            "New version available: {} → {}\nDownload: {}",
            info.current, info.latest, info.download_url
        )
    }

    /// Parse a GitHub release JSON to extract update info.
    pub fn parse_release_json(&self, json: &str) -> Option<UpdateInfo> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let tag = value.get("tag_name")?.as_str()?;
        let url = value
            .get("html_url")
            .and_then(|u| u.as_str())
            .unwrap_or_default();

        let version = tag.trim_start_matches('v');
        if version != self.current_version {
            Some(UpdateInfo {
                current: self.current_version.clone(),
                latest: version.to_string(),
                download_url: url.to_string(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_checker_new() {
        let checker = UpdateChecker::new("0.1.1");
        assert_eq!(checker.current_version(), "0.1.1");
        assert_eq!(checker.repo(), "JularDepick/Maix-Agent");
    }

    #[test]
    fn test_needs_check_initial() {
        let checker = UpdateChecker::new("0.1.0");
        assert!(checker.needs_check());
    }

    #[test]
    fn test_mark_checked() {
        let mut checker = UpdateChecker::new("0.1.0");
        checker.mark_checked();
        assert!(!checker.needs_check());
    }

    #[test]
    fn test_parse_release_newer() {
        let checker = UpdateChecker::new("0.1.0");
        let json = r#"{"tag_name": "v0.2.0", "html_url": "https://github.com/test/releases/0.2.0"}"#;
        let info = checker.parse_release_json(json).unwrap();
        assert_eq!(info.latest, "0.2.0");
        assert_eq!(info.current, "0.1.0");
    }

    #[test]
    fn test_parse_release_same() {
        let checker = UpdateChecker::new("0.1.0");
        let json = r#"{"tag_name": "v0.1.0", "html_url": "https://example.com"}"#;
        assert!(checker.parse_release_json(json).is_none());
    }

    #[test]
    fn test_format_update_notice() {
        let info = UpdateInfo {
            current: "0.1.0".into(),
            latest: "0.2.0".into(),
            download_url: "https://example.com".into(),
        };
        let notice = UpdateChecker::format_update_notice(&info);
        assert!(notice.contains("0.1.0"));
        assert!(notice.contains("0.2.0"));
    }

    #[test]
    fn test_cached_result() {
        let mut checker = UpdateChecker::new("0.1.0");
        assert!(checker.cached_result().is_none());
        checker.set_result(Some(UpdateInfo {
            current: "0.1.0".into(),
            latest: "0.2.0".into(),
            download_url: "test".into(),
        }));
        assert!(checker.cached_result().is_some());
    }

    #[test]
    fn test_with_interval() {
        let checker = UpdateChecker::new("0.1.0").with_interval(Duration::from_secs(60));
        assert_eq!(checker.check_interval, Duration::from_secs(60));
    }
}
