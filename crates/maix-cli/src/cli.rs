//! CLI argument definitions.

use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "maix-cli",
    version,
    about = "Maix-Agent CLI — stateless single-round AI assistant",
    long_about = "A git-style stateless CLI for Maix-Agent.\nEach invocation connects to the daemon, executes one command, and exits."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// maix daemon address
    #[arg(long, default_value = "127.0.0.1:26506", global = true)]
    pub server: String,

    /// Auto-launch maix daemon if not running
    #[arg(long, global = true)]
    pub launch: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Ask a question (single-round conversation)
    #[command(alias = "q")]
    Ask(AskArgs),

    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },

    /// Show or update config
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Identity (agent profile) operations
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },

    /// Architecture operations
    Architecture {
        #[command(subcommand)]
        action: ArchitectureAction,
    },

    /// Skill operations
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    /// Server (daemon) operations
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },

    /// Session operations
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Task queue operations
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },

    /// Tool operations
    Tool {
        #[command(subcommand)]
        action: ToolAction,
    },

    /// Show daemon health status
    #[command(alias = "status")]
    Health,

    /// Check for updates and update maix
    Update {
        /// Check only, don't install
        #[arg(long)]
        check: bool,
    },

    /// Show token usage and cost
    Cost,

    /// Run environment diagnostics
    Doctor,

    /// Initialize project with MAIX.md
    Init {
        /// Force overwrite existing MAIX.md
        #[arg(long)]
        force: bool,
    },
}

#[derive(Args)]
pub struct AskArgs {
    /// Question to ask (reads from stdin if not provided)
    #[arg(trailing_var_arg = true)]
    pub question: Vec<String>,

    /// Model to use (default from config)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Working directory
    #[arg(short, long)]
    pub workdir: Option<PathBuf>,

    /// Agent mode: agent, plan, yolo
    #[arg(long)]
    pub mode: Option<String>,

    /// Read additional prompt from file
    #[arg(short, long)]
    pub file: Option<PathBuf>,

    /// Show reasoning tokens
    #[arg(long)]
    pub verbose: bool,

    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    pub output_format: String,

    /// Maximum agent tool-calling rounds
    #[arg(long)]
    pub max_turns: Option<usize>,

    /// Print mode: non-interactive, output and exit (same as -p)
    #[arg(short = 'p', long)]
    pub print: bool,

    /// Continue a previous session
    #[arg(short, long)]
    pub r#continue: bool,
}

#[derive(Subcommand)]
pub enum MemoryAction {
    /// List all memories
    List,
    /// Search memories by keyword
    Search {
        /// Search query
        query: Vec<String>,
    },
    /// Forget a memory by ID
    Forget {
        /// Memory ID
        id: String,
    },
}

#[derive(Subcommand)]
pub enum IdentityAction {
    /// List available agent profiles
    List,
    /// Activate an agent profile
    Activate {
        /// Profile name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ArchitectureAction {
    /// List architectures
    List,
    /// Show architecture details
    Show {
        /// Architecture name
        name: String,
    },
    /// Run an architecture
    Run {
        /// Architecture name
        name: String,
        /// Input for the architecture
        input: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum SkillAction {
    /// Install a skill from local path or GitHub URL
    Install {
        /// Path or URL
        source: String,
    },
    /// List installed skills
    List,
    /// Enable a skill
    Enable {
        /// Skill name
        name: String,
    },
    /// Disable a skill
    Disable {
        /// Skill name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ServerAction {
    /// Install maix as a Windows Service (or systemd unit on Linux)
    Install,
    /// Uninstall the maix service
    Uninstall,
    /// Start the maix service
    Start,
    /// Stop the maix service
    Stop,
    /// Show service status
    Status,
}

#[derive(Subcommand)]
pub enum SessionAction {
    /// List all saved sessions
    List,
    /// Show messages in a session
    Show {
        /// Session ID
        id: String,
    },
    /// Delete a session
    Delete {
        /// Session ID
        id: String,
    },
    /// Fork a session (create a branch from a message point)
    Fork {
        /// Session ID to fork
        id: String,
        /// Message index to fork from (0-based, optional - forks from end if omitted)
        #[arg(long)]
        from: Option<usize>,
        /// Name for the new session
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TaskAction {
    /// Submit a new task
    Submit {
        /// Task description
        description: String,
        /// Task input
        #[arg(trailing_var_arg = true)]
        input: Vec<String>,
        /// Priority (higher = more important)
        #[arg(short, long, default_value = "0")]
        priority: u32,
    },
    /// List all tasks
    List,
    /// Cancel a task
    Cancel {
        /// Task ID
        id: String,
    },
    /// Suspend a task
    Suspend {
        /// Task ID
        id: String,
    },
    /// Resume a suspended task
    Resume {
        /// Task ID
        id: String,
    },
    /// Change task priority
    Repriority {
        /// Task ID
        id: String,
        /// New priority
        priority: u32,
    },
}

#[derive(Subcommand)]
pub enum ToolAction {
    /// List available tools
    List,
    /// Call a tool
    Call {
        /// Tool name
        name: String,
        /// Tool arguments as JSON
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current config
    Show,
    /// Set a config value
    Set {
        /// Config key (e.g., "model", "provider")
        key: String,
        /// Config value
        value: String,
    },
    /// Validate config
    Validate,
    /// Export config (API key masked)
    Export,
    /// Import config from file
    Import {
        /// Path to config JSON file
        file: PathBuf,
    },
    /// Show diff from defaults
    Diff,
}
