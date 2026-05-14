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
        let masked = if config.api_key.len() > 8 {
            format!("{}...{}", &config.api_key[..4], &config.api_key[config.api_key.len() - 4..])
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
        Ok(_resp) => {
            let elapsed = start.elapsed().as_millis();
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
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
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
