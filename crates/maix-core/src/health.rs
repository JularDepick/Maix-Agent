//! Health check system — diagnostic checks for configuration and environment.

/// Health check status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pass => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

/// Result of a health check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

/// Health checker that runs diagnostic checks.
pub struct HealthChecker {
    checks: Vec<Box<dyn Fn() -> CheckResult>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut checker = Self::new();
        checker.add_check(|| CheckResult {
            name: "Config".into(),
            status: if dirs_config_exists() {
                CheckStatus::Pass
            } else {
                CheckStatus::Warn
            },
            message: if dirs_config_exists() {
                "Config file found".into()
            } else {
                "No config file found".into()
            },
            fix_hint: if dirs_config_exists() {
                None
            } else {
                Some("Run 'maix init' to create a config".into())
            },
        });
        checker.add_check(|| {
            let has_key = std::env::var("OPENAI_API_KEY").is_ok()
                || std::env::var("ANTHROPIC_API_KEY").is_ok()
                || std::env::var("MAIX_API_KEY").is_ok();
            CheckResult {
                name: "API Key".into(),
                status: if has_key {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                message: if has_key {
                    "API key configured".into()
                } else {
                    "No API key found".into()
                },
                fix_hint: if has_key {
                    None
                } else {
                    Some("Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or MAIX_API_KEY".into())
                },
            }
        });
        checker.add_check(|| {
            let has_git = std::process::Command::new("git")
                .args(["--version"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            CheckResult {
                name: "Git".into(),
                status: if has_git {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Warn
                },
                message: if has_git {
                    "Git available".into()
                } else {
                    "Git not found".into()
                },
                fix_hint: None,
            }
        });
        checker
    }

    pub fn add_check<F: Fn() -> CheckResult + 'static>(&mut self, check: F) {
        self.checks.push(Box::new(check));
    }

    pub fn run_all(&self) -> Vec<CheckResult> {
        self.checks.iter().map(|c| c()).collect()
    }

    pub fn run_and_report(&self) -> String {
        let results = self.run_all();
        let mut output = String::from("=== Maix Health Check ===\n\n");

        let mut all_pass = true;
        for result in &results {
            let icon = result.status.icon();
            output.push_str(&format!("[{}] {}: {}\n", icon, result.name, result.message));
            if let Some(fix) = &result.fix_hint {
                output.push_str(&format!("      Fix: {}\n", fix));
            }
            if result.status != CheckStatus::Pass {
                all_pass = false;
            }
        }

        if all_pass {
            output.push_str("\nAll checks passed.\n");
        }

        output
    }

    pub fn all_pass(&self) -> bool {
        self.run_all()
            .iter()
            .all(|r| r.status == CheckStatus::Pass)
    }

    pub fn check_count(&self) -> usize {
        self.checks.len()
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

fn dirs_config_exists() -> bool {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if home.is_empty() {
        return false;
    }
    let config_path = std::path::Path::new(&home).join(".maix").join("settings.json");
    config_path.exists()
        || std::path::Path::new(&home)
            .join(".maix")
            .join("config.toml")
            .exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_checker_new() {
        let checker = HealthChecker::new();
        assert_eq!(checker.check_count(), 0);
    }

    #[test]
    fn test_health_checker_defaults() {
        let checker = HealthChecker::with_defaults();
        assert!(checker.check_count() >= 3);
    }

    #[test]
    fn test_run_all() {
        let checker = HealthChecker::with_defaults();
        let results = checker.run_all();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| !r.name.is_empty()));
    }

    #[test]
    fn test_run_and_report() {
        let checker = HealthChecker::with_defaults();
        let report = checker.run_and_report();
        assert!(report.contains("Health Check"));
        assert!(report.contains("[OK]") || report.contains("[WARN]") || report.contains("[FAIL]"));
    }

    #[test]
    fn test_check_status_icon() {
        assert_eq!(CheckStatus::Pass.icon(), "OK");
        assert_eq!(CheckStatus::Warn.icon(), "WARN");
        assert_eq!(CheckStatus::Fail.icon(), "FAIL");
    }

    #[test]
    fn test_custom_check() {
        let mut checker = HealthChecker::new();
        checker.add_check(|| CheckResult {
            name: "custom".into(),
            status: CheckStatus::Pass,
            message: "all good".into(),
            fix_hint: None,
        });
        assert_eq!(checker.check_count(), 1);
        let results = checker.run_all();
        assert_eq!(results[0].status, CheckStatus::Pass);
    }
}
