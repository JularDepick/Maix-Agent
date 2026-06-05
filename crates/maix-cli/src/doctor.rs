//! /doctor diagnostics — check Maix-Agent environment health.

use maix_core::client::MaixClient;
use maix_core::Config;

#[derive(Debug)]
pub enum DiagStatus {
    Pass,
    Warn,
    Fail,
}

impl std::fmt::Display for DiagStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagStatus::Pass => write!(f, "✅"),
            DiagStatus::Warn => write!(f, "⚠️"),
            DiagStatus::Fail => write!(f, "❌"),
        }
    }
}

#[derive(Debug)]
pub struct DiagnosticResult {
    pub name: String,
    pub status: DiagStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

/// Run all diagnostics and return results.
pub async fn run_diagnostics(config: &Config, client: &MaixClient) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();

    // 1. Daemon connection
    results.push(check_daemon(client).await);

    // 2. API Key
    results.push(check_api_key(config));

    // 3. Network connectivity
    results.push(check_network(config).await);

    // 4. Provider availability
    results.push(check_provider(client).await);

    // 5. Database
    results.push(check_database());

    // 6. Tools
    results.push(check_tools(client).await);

    // 7. Disk space
    results.push(check_disk_space());

    // 8. Git
    results.push(check_git());

    // 9. Skills directory
    results.push(check_skills_dir());

    // 10. Version update
    results.push(check_version().await);

    results
}

async fn check_daemon(client: &MaixClient) -> DiagnosticResult {
    match client.health_check().await {
        Ok(resp) => DiagnosticResult {
            name: "Daemon 连接".into(),
            status: DiagStatus::Pass,
            message: format!(
                "maix.exe running (uptime {}s, sessions: {})",
                resp.uptime_secs, resp.active_sessions
            ),
            fix_hint: None,
        },
        Err(e) => DiagnosticResult {
            name: "Daemon 连接".into(),
            status: DiagStatus::Fail,
            message: format!("无法连接: {e}"),
            fix_hint: Some("运行 `maix daemon` 启动守护进程".into()),
        },
    }
}

fn check_api_key(config: &Config) -> DiagnosticResult {
    if config.api_key.is_empty() {
        DiagnosticResult {
            name: "API Key".into(),
            status: DiagStatus::Fail,
            message: "未配置 API Key".into(),
            fix_hint: Some("在 ~/.maix/settings.json 中设置 api_key 或设置 MAIX_API_KEY 环境变量".into()),
        }
    } else {
        let masked = if config.api_key.len() > 4 {
            let prefix: String = config.api_key.chars().take(4).collect();
            format!("{prefix}...")
        } else {
            "***".into()
        };
        DiagnosticResult {
            name: "API Key".into(),
            status: DiagStatus::Pass,
            message: format!("已配置 ({masked})"),
            fix_hint: None,
        }
    }
}

async fn check_network(config: &Config) -> DiagnosticResult {
    if config.api_base.is_empty() {
        return DiagnosticResult {
            name: "网络连通".into(),
            status: DiagStatus::Warn,
            message: "API base URL 未配置".into(),
            fix_hint: Some("在 ~/.maix/settings.json 中设置 api_base".into()),
        };
    }

    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("{}/models", config.api_base.trim_end_matches('/'));
    match client.get(&url).send().await {
        Ok(resp) => {
            let elapsed = start.elapsed().as_millis();
            let code = resp.status().as_u16();
            if code == 401 || code == 403 {
                return DiagnosticResult {
                    name: "网络连通".into(),
                    status: DiagStatus::Warn,
                    message: format!("可达但认证失败 (HTTP {code})，请检查 API Key"),
                    fix_hint: Some("确认 ~/.maix/settings.json 中的 api_key 正确".into()),
                };
            }
            let status = if elapsed > 2000 {
                DiagStatus::Warn
            } else {
                DiagStatus::Pass
            };
            let msg = if elapsed > 2000 {
                format!("可达 ({elapsed}ms)，延迟较高")
            } else {
                format!("可达 ({elapsed}ms)")
            };
            DiagnosticResult {
                name: "网络连通".into(),
                status,
                message: msg,
                fix_hint: if elapsed > 2000 {
                    Some("建议使用本地代理降低延迟".into())
                } else {
                    None
                },
            }
        }
        Err(e) => DiagnosticResult {
            name: "网络连通".into(),
            status: DiagStatus::Fail,
            message: format!("不可达: {e}"),
            fix_hint: Some("检查网络连接和 API base URL".into()),
        },
    }
}

async fn check_provider(client: &MaixClient) -> DiagnosticResult {
    match client.get_config().await {
        Ok(cfg) => DiagnosticResult {
            name: "Provider".into(),
            status: DiagStatus::Pass,
            message: format!("{} / {}", cfg.active_provider, cfg.model),
            fix_hint: None,
        },
        Err(e) => DiagnosticResult {
            name: "Provider".into(),
            status: DiagStatus::Fail,
            message: format!("获取配置失败: {e}"),
            fix_hint: None,
        },
    }
}

fn check_database() -> DiagnosticResult {
    let db_path = maix_core::config::default_memory_dir()
        .parent()
        .unwrap_or(&std::path::PathBuf::from("."))
        .join("maix.db");
    if db_path.exists() {
        let size = std::fs::metadata(&db_path)
            .map(|m| m.len())
            .unwrap_or(0);
        DiagnosticResult {
            name: "数据库".into(),
            status: DiagStatus::Pass,
            message: format!("{} ({} KB)", db_path.display(), size / 1024),
            fix_hint: None,
        }
    } else {
        DiagnosticResult {
            name: "数据库".into(),
            status: DiagStatus::Warn,
            message: "数据库文件不存在（首次运行时自动创建）".into(),
            fix_hint: None,
        }
    }
}

async fn check_tools(client: &MaixClient) -> DiagnosticResult {
    match client.list_tools().await {
        Ok(tools) => DiagnosticResult {
            name: "工具注册".into(),
            status: DiagStatus::Pass,
            message: format!("{} 个工具已加载", tools.len()),
            fix_hint: None,
        },
        Err(e) => DiagnosticResult {
            name: "工具注册".into(),
            status: DiagStatus::Fail,
            message: format!("获取工具列表失败: {e}"),
            fix_hint: None,
        },
    }
}

fn check_disk_space() -> DiagnosticResult {
    // Check if ~/.maix directory is writable
    let maix_dir = dirs_home().join(".maix");
    match std::fs::create_dir_all(&maix_dir) {
        Ok(_) => DiagnosticResult {
            name: "磁盘空间".into(),
            status: DiagStatus::Pass,
            message: format!("{} 可写", maix_dir.display()),
            fix_hint: None,
        },
        Err(e) => DiagnosticResult {
            name: "磁盘空间".into(),
            status: DiagStatus::Fail,
            message: format!("无法写入 {}: {e}", maix_dir.display()),
            fix_hint: Some("检查目录权限和磁盘空间".into()),
        },
    }
}

fn check_git() -> DiagnosticResult {
    match std::process::Command::new("git").arg("--version").output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            DiagnosticResult {
                name: "Git".into(),
                status: DiagStatus::Pass,
                message: version,
                fix_hint: None,
            }
        }
        Err(_) => DiagnosticResult {
            name: "Git".into(),
            status: DiagStatus::Warn,
            message: "git 未安装或不在 PATH 中".into(),
            fix_hint: Some("安装 git: https://git-scm.com/".into()),
        },
    }
}

fn dirs_home() -> std::path::PathBuf {
    home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn check_skills_dir() -> DiagnosticResult {
    let skills_dir = dirs_home().join(".maix").join("skills");
    if skills_dir.exists() {
        let count = std::fs::read_dir(&skills_dir)
            .map(|entries| entries.filter(|e| e.as_ref().map(|e| e.path().is_dir()).unwrap_or(false)).count())
            .unwrap_or(0);
        DiagnosticResult {
            name: "Skills 目录".into(),
            status: DiagStatus::Pass,
            message: format!("{} ({} 个技能)", skills_dir.display(), count),
            fix_hint: None,
        }
    } else {
        DiagnosticResult {
            name: "Skills 目录".into(),
            status: DiagStatus::Pass,
            message: format!("{} (不存在，首次安装技能时自动创建)", skills_dir.display()),
            fix_hint: None,
        }
    }
}

async fn check_version() -> DiagnosticResult {
    let current = env!("CARGO_PKG_VERSION");
    let url = "https://api.github.com/repos/JularDepick/Maix-Agent/releases/latest";

    let client = match reqwest::Client::builder()
        .user_agent(format!("maix-cli/{}", current))
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return DiagnosticResult {
                name: "版本检查".into(),
                status: DiagStatus::Warn,
                message: format!("当前 v{} (无法检查更新)", current),
                fix_hint: None,
            };
        }
    };

    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(tag) = json.get("tag_name").and_then(|t| t.as_str()) {
                        let latest = tag.trim_start_matches('v');
                        if latest != current {
                            return DiagnosticResult {
                                name: "版本检查".into(),
                                status: DiagStatus::Warn,
                                message: format!("当前 v{}, 最新 v{}", current, latest),
                                fix_hint: Some("运行 `maix update` 更新".into()),
                            };
                        }
                    }
                }
            }
            DiagnosticResult {
                name: "版本检查".into(),
                status: DiagStatus::Pass,
                message: format!("v{} (最新版)", current),
                fix_hint: None,
            }
        }
        _ => DiagnosticResult {
            name: "版本检查".into(),
            status: DiagStatus::Warn,
            message: format!("v{} (无法检查更新)", current),
            fix_hint: None,
        },
    }
}

/// Format diagnostic results as a human-readable string.
pub fn format_diagnostics(results: &[DiagnosticResult]) -> String {
    let mut out = String::from("Maix-Agent Doctor\n\n");
    let mut fixes = Vec::new();

    for r in results {
        out.push_str(&format!("{} {:<16} {}\n", r.status, r.name, r.message));
        if let Some(fix) = &r.fix_hint {
            fixes.push(format!("- {}: {}", r.name, fix));
        }
    }

    if !fixes.is_empty() {
        out.push_str("\n建议:\n");
        for fix in fixes {
            out.push_str(&format!("{}\n", fix));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diag_status_display() {
        assert_eq!(DiagStatus::Pass.to_string(), "✅");
        assert_eq!(DiagStatus::Warn.to_string(), "⚠️");
        assert_eq!(DiagStatus::Fail.to_string(), "❌");
    }

    #[test]
    fn test_format_diagnostics_all_pass() {
        let results = vec![
            DiagnosticResult {
                name: "API Key".into(),
                status: DiagStatus::Pass,
                message: "configured".into(),
                fix_hint: None,
            },
        ];
        let output = format_diagnostics(&results);
        assert!(output.contains("Maix-Agent Doctor"));
        assert!(output.contains("API Key"));
        assert!(!output.contains("建议"));
    }

    #[test]
    fn test_format_diagnostics_with_fixes() {
        let results = vec![
            DiagnosticResult {
                name: "Database".into(),
                status: DiagStatus::Fail,
                message: "not found".into(),
                fix_hint: Some("run `maix init`".into()),
            },
        ];
        let output = format_diagnostics(&results);
        assert!(output.contains("建议"));
        assert!(output.contains("run `maix init`"));
    }

    #[test]
    fn test_check_api_key_empty() {
        let config = maix_core::Config::minimal();
        let result = check_api_key(&config);
        assert!(matches!(result.status, DiagStatus::Fail));
        assert!(result.message.contains("未配置"));
    }

    #[test]
    fn test_check_api_key_long() {
        let mut config = maix_core::Config::minimal();
        config.api_key = "sk-1234567890abcdef".into();
        let result = check_api_key(&config);
        assert!(matches!(result.status, DiagStatus::Pass));
        assert!(result.message.contains("sk-1"));
        assert!(!result.message.contains("sk-1234567890abcdef"));
    }

    #[test]
    fn test_check_api_key_short() {
        let mut config = maix_core::Config::minimal();
        config.api_key = "short".into();
        let result = check_api_key(&config);
        assert!(matches!(result.status, DiagStatus::Pass));
        // "short" has len 5 > 4, so shows first 4 + "..."
        assert!(result.message.contains("shor"));
    }

    #[test]
    fn test_format_diagnostics_empty() {
        let results: Vec<DiagnosticResult> = vec![];
        let output = format_diagnostics(&results);
        assert!(output.contains("Maix-Agent Doctor"));
        assert!(!output.contains("建议"));
    }

    #[test]
    fn test_format_diagnostics_mixed() {
        let results = vec![
            DiagnosticResult {
                name: "API Key".into(),
                status: DiagStatus::Pass,
                message: "configured".into(),
                fix_hint: None,
            },
            DiagnosticResult {
                name: "Database".into(),
                status: DiagStatus::Warn,
                message: "slow".into(),
                fix_hint: None,
            },
            DiagnosticResult {
                name: "Network".into(),
                status: DiagStatus::Fail,
                message: "timeout".into(),
                fix_hint: Some("check firewall".into()),
            },
        ];
        let output = format_diagnostics(&results);
        assert!(output.contains("API Key"));
        assert!(output.contains("Database"));
        assert!(output.contains("Network"));
        assert!(output.contains("建议"));
        assert!(output.contains("check firewall"));
    }

    #[test]
    fn test_check_api_key_exact_8() {
        let mut config = maix_core::Config::minimal();
        config.api_key = "12345678".into();
        let result = check_api_key(&config);
        assert!(matches!(result.status, DiagStatus::Pass));
        // len == 8, not > 4, so shows first 4 + "..."
        assert!(result.message.contains("1234"));
    }

    #[test]
    fn test_check_api_key_exact_9() {
        let mut config = maix_core::Config::minimal();
        config.api_key = "123456789".into();
        let result = check_api_key(&config);
        assert!(matches!(result.status, DiagStatus::Pass));
        // len == 9, > 4, so shows first 4 + "..."
        assert!(result.message.contains("1234"));
        assert!(!result.message.contains("6789"));
    }
}
