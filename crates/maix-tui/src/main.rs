mod app;
mod clipboard;
mod desk;
mod diff_view;
mod highlight;
mod input;
mod layout;
mod notify;
mod palette;
mod pane;
mod status_bar;
mod stream_renderer;
mod ui;
mod vim;

use app::App;
use clap::Parser;
use std::path::PathBuf;

fn dirs_home() -> PathBuf {
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
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use maix_core::client::MaixClient;
use ratatui::backend::CrosstermBackend;
use std::io;

#[derive(Parser)]
#[command(
    name = "maix-tui",
    version,
    about = "Maix-Agent Terminal UI — interactive multi-round assistant"
)]
struct Cli {
    /// Agent mode: agent, plan, yolo
    #[arg(long, default_value = "agent")]
    mode: String,

    /// maix daemon address
    #[arg(long, default_value = "127.0.0.1:26506")]
    server: String,

    /// Resume a saved session by ID
    #[arg(long)]
    resume: Option<String>,

    /// Auto-launch maix daemon if not running
    #[arg(long)]
    launch: bool,

    /// Restore last session on startup
    #[arg(long)]
    restore: bool,

    /// UI theme: dark, light, solarized, dracula
    #[arg(long, default_value = "dark")]
    theme: String,

    /// Layout preset: standard, compact, relaxed, focus
    #[arg(long, default_value = "standard")]
    layout: String,
}

/// Clean up log files older than max_days (105-018).
fn cleanup_old_logs(log_dir: &std::path::Path, max_days: u64) -> io::Result<()> {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(max_days * 86400);
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "log") {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    maix_core::init_console_utf8();

    // Set up panic handler to save crash logs and restore terminal (105-011)
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Try to restore terminal state before showing panic
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );

        // Try to save panic info to log file
        let log_path = dirs_home().join(".maix").join("crash.log");
        let _ = std::fs::create_dir_all(log_path.parent().unwrap());
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "=== Crash at {} ===", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
            let _ = writeln!(file, "{}", panic_info);
            let _ = writeln!(file, "Location: {:?}", panic_info.location());
            let _ = writeln!(file, "");
        }

        // Print user-friendly error message
        eprintln!("\n╔══════════════════════════════════════════════════╗");
        eprintln!("║  Maix TUI 遇到意外错误并已退出                   ║");
        eprintln!("║  错误详情已保存到 ~/.maix/crash.log               ║");
        eprintln!("║  请报告此问题: https://github.com/anthropics/... ║");
        eprintln!("╚══════════════════════════════════════════════════╝");

        // Call original hook
        orig_hook(panic_info);
    }));

    // Set up structured logging with file output (105-018)
    let log_dir = dirs_home().join(".maix").join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // Configure log level from environment or default to warn
    let log_level = std::env::var("MAIX_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());

    // Log to file with daily rotation
    let log_file = log_dir.join(format!("maix-{}.log", chrono::Local::now().format("%Y-%m-%d")));
    if let Ok(log_file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        let _ = tracing_subscriber::fmt()
            .with_writer(log_file)
            .with_env_filter(&log_level)
            .with_ansi(false)
            .try_init();
    } else {
        // Fallback to stderr
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(&log_level)
            .try_init();
    }

    // Clean up old log files (keep last 7 days)
    let _ = cleanup_old_logs(&log_dir, 7);

    let cli = Cli::parse();

    // --- Phase 1: Connect to daemon (normal terminal, errors visible) ---

    if cli.launch {
        if let Err(e) = maix_core::ensure_server_running(&cli.server).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

    let client = match MaixClient::connect(&cli.server).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: failed to connect to maix at {}: {e}", cli.server);
            std::process::exit(1);
        }
    };

    if let Err(e) = client.health_check().await {
        eprintln!("Error: server health check failed: {e}");
        std::process::exit(1);
    }

    let session_id = match client.create_session().await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error: failed to create session: {e}");
            std::process::exit(1);
        }
    };

    // --- Phase 2: Enter TUI (raw mode + alternate screen) ---

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::new(backend)?;

    let mode = match cli.mode.as_str() {
        "plan" => app::MODE_PLAN,
        "yolo" => app::MODE_YOLO,
        _ => app::MODE_AGENT,
    };

    let mut app = App::new(client, session_id, mode, cli.server, cli.resume).await;
    app.theme = crate::ui::Theme::from_name(&cli.theme);

    // Apply layout preset
    match cli.layout.as_str() {
        "compact" => {
            app.panel_width = 20;
            app.show_dividers = false;
            app.show_timestamps = false;
            app.layout_preset = "compact".to_string();
        }
        "relaxed" => {
            app.panel_width = 40;
            app.show_dividers = true;
            app.show_timestamps = true;
            app.layout_preset = "relaxed".to_string();
        }
        "focus" => {
            app.panel_width = 15;
            app.show_dividers = false;
            app.show_timestamps = false;
            app.fullscreen = true;
            app.layout_preset = "focus".to_string();
        }
        _ => {
            app.layout_preset = "standard".to_string();
        }
    }

    let result = app.run(terminal).await;

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

    if let Err(e) = result {
        eprintln!("TUI error: {e}");
    }

    Ok(())
}
