//! # Maix-CLI
//!
//! Command-line interface for Maix-Agent — manage sessions, memory, tools,
//! and interact with the Maix server.
//!
//! ## Usage
//!
//! ```bash
//! # Start a chat session
//! maix ask "What is Rust?"
//!
//! # Manage memory
//! maix memory search "rust"
//! maix memory list
//!
//! # Check system health
//! maix health
//! maix doctor
//!
//! # Manage sessions
//! maix session list
//! maix session show <id>
//! ```
//!
//! ## Commands
//!
//! - `ask` — Send a message to the agent
//! - `memory` — Search, list, forget memories
//! - `session` — List, show, fork, delete sessions
//! - `skill` — Install, list, enable, disable skills
//! - `tool` — List, call tools
//! - `config` — View, set configuration
//! - `health` — Check system health
//! - `doctor` — Run diagnostics
//! - `cost` — View cost statistics
//! - `update` — Check for updates

mod cli;
mod commands;
mod doctor;
mod update;

use clap::Parser;
use cli::*;
use maix_core::client::MaixClient;

#[tokio::main]
async fn main() {
    maix_core::init_console_utf8();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("MAIX_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();

    // Auto-launch if requested
    if cli.launch {
        if let Err(e) = maix_core::ensure_server_running(&cli.server).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

    // Connect to daemon
    let client = MaixClient::connect(&cli.server).await.unwrap_or_else(|e| {
        eprintln!("Cannot connect to maix at {}. Is maix running?", cli.server);
        eprintln!("  Hint: maix --foreground  (or use --launch to auto-start)");
        tracing::debug!("connect error: {e}");
        std::process::exit(1);
    });

    match cli.command {
        Commands::Ask(args) => commands::cmd_ask(&client, args).await,
        Commands::Memory { action } => commands::cmd_memory(&client, action).await,
        Commands::Config { action } => commands::cmd_config(&client, action).await,
        Commands::Identity { action } => commands::cmd_identity(&client, action).await,
        Commands::Architecture { action } => commands::cmd_architecture(&client, action).await,
        Commands::Skill { action } => commands::cmd_skill(&client, action).await,
        Commands::Server { action } => commands::cmd_server(action).await,
        Commands::Session { action } => commands::cmd_session(&client, action).await,
        Commands::Task { action } => commands::cmd_task(&client, action).await,
        Commands::Tool { action } => commands::cmd_tool(&client, action).await,
        Commands::Health => commands::cmd_health(&client).await,
        Commands::Update { check } => commands::cmd_update(check).await,
        Commands::Cost => commands::cmd_cost(&client).await,
        Commands::Doctor => commands::cmd_doctor(&client).await,
        Commands::Init { force } => commands::cmd_init(force).await,
    }
}
