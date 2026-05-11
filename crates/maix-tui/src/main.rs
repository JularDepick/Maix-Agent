mod app;
mod config_wizard;
mod input;
mod ui;

use app::App;
use clap::Parser;
use config_wizard::ConfigWizard;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "maix-tui", version, about = "Maix-Agent Terminal UI")]
struct Cli {
    #[arg(short, long, default_value = "deepseek")]
    model: String,

    #[arg(short, long, default_value = ".")]
    workdir: std::path::PathBuf,

    #[arg(long, default_value = "agent")]
    mode: String,

    #[arg(long, default_value = "127.0.0.1:26506")]
    server: String,
}

fn config_path() -> PathBuf {
    std::env::var("MAIX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = if let Ok(home) = std::env::var("USERPROFILE") {
                PathBuf::from(home)
            } else if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home)
            } else {
                PathBuf::from(".")
            };
            p.push(".maix");
            p
        })
        .join("config.toml")
}

fn is_first_launch() -> bool {
    std::env::args().len() <= 1 && !config_path().exists()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    maix_core::init_console_utf8();
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .try_init();

    let cli = if is_first_launch() {
        let mut wizard = ConfigWizard::new();
        enable_raw_mode()?;
        {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(stdout);
            let terminal = ratatui::Terminal::new(backend)?;
            wizard.run(terminal)?;
        }
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        if !wizard.api_key.is_empty() {
            if let Ok(path) = wizard.save_config() {
                tracing::info!("Config saved to {}", path.display());
            }
        }
        Cli {
            model: wizard.model,
            workdir: PathBuf::from("."),
            mode: "agent".into(),
            server: "127.0.0.1:26506".into(),
        }
    } else {
        Cli::parse()
    };

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
    let mut app = App::new(cli.model, cli.workdir, mode, cli.server).await;

    let result = app.run(terminal).await;

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

    if let Err(e) = result {
        eprintln!("TUI error: {e}");
    }

    Ok(())
}
