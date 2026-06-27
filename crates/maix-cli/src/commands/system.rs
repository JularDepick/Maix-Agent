//! System commands: health, doctor, cost, update, init.

use super::{download_file, format_number};
use maix_core::client::MaixClient;
use std::path::PathBuf;

pub async fn cmd_health(client: &MaixClient) {
    match client.health_check().await {
        Ok(h) => {
            println!("Status:   {}", h.status);
            println!("Version:  {}", h.version);
            println!("Uptime:   {}s", h.uptime_secs);
            println!("Sessions: {}", h.active_sessions);
            println!("Queue:    {}", h.queue_depth);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn cmd_doctor(client: &MaixClient) {
    let config = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
    let results = crate::doctor::run_diagnostics(&config, client).await;
    println!("{}", crate::doctor::format_diagnostics(&results));
}

pub async fn cmd_cost(client: &MaixClient) {
    match client.get_work_status().await {
        Ok(status) => {
            let pricing = maix_core::types::Pricing::default();
            let total_tokens = status.total_tokens;
            let total_cost = status.total_cost;

            println!("Maix-Agent Cost Report");
            println!("{}", "─".repeat(40));
            println!("Active agents:    {}", status.active_agents);
            println!("Idle agents:      {}", status.idle_agents);
            println!("Queue depth:      {}", status.queue_depth);
            println!("Tasks completed:  {}", status.tasks_completed);
            println!("Tasks failed:     {}", status.tasks_failed);
            println!("Uptime:           {}s", status.uptime_secs);
            println!();
            println!("Token Usage");
            println!("{}", "─".repeat(40));
            println!("Total tokens:     {}", format_number(total_tokens));

            if !status.agents.is_empty() {
                println!();
                println!("Per-Agent Breakdown");
                println!("{}", "─".repeat(40));
                for agent in &status.agents {
                    println!(
                        "  {} ({}): {} tokens, {} tool calls",
                        agent.agent_id,
                        agent.state,
                        format_number(agent.total_tokens),
                        agent.tool_calls
                    );
                }
            }

            println!();
            println!("Cost Estimate");
            println!("{}", "─".repeat(40));
            let (input_cost, output_cost) = estimate_cost(total_tokens, pricing.input_per_million, pricing.output_per_million);

            if total_cost > 0.0 {
                println!("Server tracked:   ¥{:.4}", total_cost);
            }
            println!("Estimated ({} tok):", format_number(total_tokens));
            println!("  Input ({}%):     ¥{:.4}", 70, input_cost);
            println!("  Output ({}%):    ¥{:.4}", 30, output_cost);
            println!("  Total:           ¥{:.4}", input_cost + output_cost);
        }
        Err(e) => {
            eprintln!("Error getting work status: {e}");
            eprintln!("Is the maix server running?");
            std::process::exit(1);
        }
    }
}

pub async fn cmd_update(check_only: bool) {
    let current = env!("CARGO_PKG_VERSION");
    println!("Current version: v{}", current);
    println!("Checking for updates...");

    let url = "https://api.github.com/repos/JularDepick/Maix-Agent/releases/latest";
    let http_client = match reqwest::Client::builder()
        .user_agent(format!("maix-cli/{}", current))
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let resp = match http_client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error checking for updates: {e}");
            std::process::exit(1);
        }
    };

    if !resp.status().is_success() {
        eprintln!("GitHub API returned status {}", resp.status());
        std::process::exit(1);
    }

    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading response: {e}");
            std::process::exit(1);
        }
    };

    let checker = crate::update::UpdateChecker::new(current);
    match checker.parse_release_json(&body) {
        Some(info) => {
            println!("\nNew version available: v{} -> v{}", info.current, info.latest);
            if !info.release_notes.is_empty() {
                println!("\nRelease notes:");
                let notes = truncate_notes(&info.release_notes, 500);
                println!("{}", notes);
            }
            println!("\nDownload: {}", info.download_url);

            if check_only {
                return;
            }

            println!("\nDownloading v{}...", info.latest);
            let temp_dir = std::env::temp_dir();
            let ext = if cfg!(target_os = "windows") { "zip" } else { "tar.gz" };
            let archive_path = temp_dir.join(format!("maix-update.{}", ext));

            match download_file(&info.download_url, &archive_path).await {
                Ok(_) => {
                    println!("Downloaded to {}", archive_path.display());
                    println!("\nTo install:");
                    if cfg!(target_os = "windows") {
                        println!("  1. Stop the maix service: maix server stop");
                        println!("  2. Extract {} to your maix directory", archive_path.display());
                        println!("  3. Start the maix service: maix server start");
                    } else {
                        println!("  1. Stop the maix service: maix server stop");
                        println!("  2. tar xzf {} -C /usr/local/bin/", archive_path.display());
                        println!("  3. Start the maix service: maix server start");
                    }
                }
                Err(e) => {
                    eprintln!("Download failed: {e}");
                    eprintln!("Please download manually from: {}", info.download_url);
                    std::process::exit(1);
                }
            }
        }
        None => {
            println!("You are running the latest version (v{}).", current);
        }
    }
}

pub async fn cmd_config(client: &MaixClient, action: Option<crate::cli::ConfigAction>) {
    use crate::cli::ConfigAction;

    match action.unwrap_or(ConfigAction::Show) {
        ConfigAction::Show => match client.get_config().await {
            Ok(cfg) => {
                println!("Provider: {}", cfg.active_provider);
                println!("Model:    {}", cfg.model);
                if !cfg.api_base.is_empty() {
                    println!("API Base: {}", cfg.api_base);
                }
                println!("Listen:   {}:{}", cfg.listen_addr, cfg.listen_port);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        ConfigAction::Set { key, value } => {
            let (section, config_key) = parse_config_key(&key);
            let mut value_map = serde_json::Map::new();
            value_map.insert(config_key.to_string(), serde_json::Value::String(value.clone()));
            match client.update_config(section, config_key, value_map).await {
                Ok(_) => println!("Updated {key} = {value}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Validate => {
            let config = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
            let errors = maix_core::config::validate_config(&config);
            if errors.is_empty() {
                println!("Config is valid.");
            } else {
                println!("Config validation errors:");
                for e in &errors {
                    println!("  - {e}");
                }
                std::process::exit(1);
            }
        }
        ConfigAction::Export => {
            let config = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
            match maix_core::config::export_config(&config) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("Error exporting config: {e}");
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Import { file } => {
            let content = match std::fs::read_to_string(&file) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading {}: {e}", file.display());
                    std::process::exit(1);
                }
            };
            match maix_core::config::import_config(&content) {
                Ok(settings) => {
                    match maix_core::Config::save_user_settings(&settings) {
                        Ok(path) => println!("Imported config to {}", path.display()),
                        Err(e) => {
                            eprintln!("Error saving config: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error parsing config: {e}");
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Diff => {
            let config = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
            println!("{}", maix_core::config::config_diff(&config));
        }
    }
}

pub async fn cmd_init(force: bool) {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let maix_md_path = root.join("MAIX.md");

    if maix_md_path.exists() && !force {
        eprintln!("MAIX.md already exists. Use --force to overwrite.");
        std::process::exit(1);
    }

    let project_type = maix_agent::init::detect_project_type(&root);
    let dir_tree = maix_agent::init::build_dir_tree(&root);
    let key_files = maix_agent::init::scan_project_files(&root);
    let content = maix_agent::init::generate_maix_md(project_type, &dir_tree, &key_files);

    match std::fs::write(&maix_md_path, &content) {
        Ok(_) => {
            println!("Generated MAIX.md ({project_type} project)");
            println!("Path: {}", maix_md_path.display());
        }
        Err(e) => {
            eprintln!("Failed to generate MAIX.md: {e}");
            std::process::exit(1);
        }
    }
}

/// Split a config key into (section, field). E.g. "provider.model" -> ("provider", "model").
pub(super) fn parse_config_key(key: &str) -> (&str, &str) {
    let parts: Vec<&str> = key.splitn(2, '.').collect();
    if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("general", key)
    }
}

/// Estimate input/output cost from total tokens using a 70/30 split.
pub(super) fn estimate_cost(total_tokens: u64, input_rate: f64, output_rate: f64) -> (f64, f64) {
    let estimated_input = total_tokens * 70 / 100;
    let estimated_output = total_tokens * 30 / 100;
    let input_cost = estimated_input as f64 * input_rate / 1_000_000.0;
    let output_cost = estimated_output as f64 * output_rate / 1_000_000.0;
    (input_cost, output_cost)
}

/// Truncate release notes to max_len bytes, appending "..." if truncated.
#[allow(dead_code)]
pub(super) fn truncate_notes(notes: &str, max_len: usize) -> String {
    if notes.len() > max_len {
        let mut end = max_len;
        while end > 0 && !notes.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &notes[..end])
    } else {
        notes.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_key_simple() {
        assert_eq!(parse_config_key("model"), ("general", "model"));
    }

    #[test]
    fn test_parse_config_key_dotted() {
        assert_eq!(parse_config_key("provider.model"), ("provider", "model"));
    }

    #[test]
    fn test_parse_config_key_multiple_dots() {
        assert_eq!(parse_config_key("a.b.c"), ("a", "b.c"));
    }

    #[test]
    fn test_parse_config_key_empty() {
        assert_eq!(parse_config_key(""), ("general", ""));
    }

    #[test]
    fn test_estimate_cost_zero() {
        let (input, output) = estimate_cost(0, 3.0, 15.0);
        assert_eq!(input, 0.0);
        assert_eq!(output, 0.0);
    }

    #[test]
    fn test_estimate_cost_known() {
        // 1_000_000 tokens, 70% input = 700_000, 30% output = 300_000
        // input cost = 700_000 * 3.0 / 1_000_000 = 2.1
        // output cost = 300_000 * 15.0 / 1_000_000 = 4.5
        let (input, output) = estimate_cost(1_000_000, 3.0, 15.0);
        assert!((input - 2.1).abs() < 0.001);
        assert!((output - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_truncate_notes_short() {
        assert_eq!(truncate_notes("short", 500), "short");
    }

    #[test]
    fn test_truncate_notes_exact() {
        let notes = "a".repeat(500);
        assert_eq!(truncate_notes(&notes, 500), notes);
    }

    #[test]
    fn test_truncate_notes_long() {
        let notes = "a".repeat(501);
        let result = truncate_notes(&notes, 500);
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 503); // 500 + "..."
    }

    #[test]
    fn test_truncate_notes_empty() {
        assert_eq!(truncate_notes("", 500), "");
    }

    #[test]
    fn test_truncate_notes_utf8_boundary() {
        // "你好世界" = 4 chars, each 3 bytes = 12 bytes total
        // max_len=5 falls mid-char at byte 5 (inside second char)
        // should back up to byte 3 (end of first char "你")
        let result = truncate_notes("你好世界", 5);
        assert!(result.ends_with("..."));
        assert_eq!(result, "你...");
    }

    #[test]
    fn test_truncate_notes_utf8_exact_char() {
        // max_len=6 = exactly 2 Chinese chars, but 12 bytes > 6, so truncates + "..."
        let result = truncate_notes("你好世界", 6);
        assert_eq!(result, "你好...");
    }

    #[test]
    fn test_parse_config_key_dot_only() {
        assert_eq!(parse_config_key("."), ("", ""));
    }

    #[test]
    fn test_parse_config_key_leading_dot() {
        assert_eq!(parse_config_key(".model"), ("", "model"));
    }

    #[test]
    fn test_parse_config_key_trailing_dot() {
        assert_eq!(parse_config_key("provider."), ("provider", ""));
    }

    #[test]
    fn test_estimate_cost_small_tokens() {
        // 1 token: 70/100=0, 30/100=0 → both costs are 0
        let (input, output) = estimate_cost(1, 3.0, 15.0);
        assert_eq!(input, 0.0);
        assert_eq!(output, 0.0);
    }

    #[test]
    fn test_estimate_cost_large_tokens() {
        // 10M tokens: 7M input, 3M output
        let (input, output) = estimate_cost(10_000_000, 3.0, 15.0);
        assert!((input - 21.0).abs() < 0.01);
        assert!((output - 45.0).abs() < 0.01);
    }

    #[test]
    fn test_format_number_u64_max() {
        let result = format_number(u64::MAX);
        assert_eq!(result, "18,446,744,073,709,551,615");
    }

    #[test]
    fn test_format_number_999() {
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_million() {
        assert_eq!(format_number(1_000_000), "1,000,000");
    }
}
