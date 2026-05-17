mod doctor;

use clap::{Parser, Subcommand, Args};
use maix_core::client::{start_chat, MaixClient};
use maix_core::proto::maix::core::v1 as pb;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "maix-cli",
    version,
    about = "Maix-Agent CLI — stateless single-round AI assistant",
    long_about = "A git-style stateless CLI for Maix-Agent.\nEach invocation connects to the daemon, executes one command, and exits."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// maix daemon address
    #[arg(long, default_value = "127.0.0.1:26506", global = true)]
    server: String,

    /// Auto-launch maix daemon if not running
    #[arg(long, global = true)]
    launch: bool,
}

#[derive(Subcommand)]
enum Commands {
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
struct AskArgs {
    /// Question to ask (reads from stdin if not provided)
    #[arg(trailing_var_arg = true)]
    question: Vec<String>,

    /// Model to use (default from config)
    #[arg(short, long)]
    model: Option<String>,

    /// Working directory
    #[arg(short, long)]
    workdir: Option<PathBuf>,

    /// Agent mode: agent, plan, yolo
    #[arg(long)]
    mode: Option<String>,

    /// Read additional prompt from file
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Show reasoning tokens
    #[arg(long)]
    verbose: bool,

    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    output_format: String,

    /// Maximum agent tool-calling rounds
    #[arg(long)]
    max_turns: Option<usize>,

    /// Print mode: non-interactive, output and exit (same as -p)
    #[arg(short = 'p', long)]
    print: bool,

    /// Continue a previous session
    #[arg(short, long)]
    r#continue: bool,
}

#[derive(Subcommand)]
enum MemoryAction {
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
enum IdentityAction {
    /// List available agent profiles
    List,
    /// Activate an agent profile
    Activate {
        /// Profile name
        name: String,
    },
}

#[derive(Subcommand)]
enum ArchitectureAction {
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
enum SkillAction {
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
enum ServerAction {
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
enum SessionAction {
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
enum TaskAction {
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
enum ToolAction {
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
enum ConfigAction {
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

fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn dirs_next() -> Option<std::path::PathBuf> {
    home::home_dir()
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
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

#[allow(dead_code)]
fn agent_mode_from_str(s: &str) -> i32 {
    match s {
        "plan" => pb::AgentMode::Plan.into(),
        "yolo" => pb::AgentMode::Yolo.into(),
        _ => pb::AgentMode::Agent.into(),
    }
}

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
        Commands::Ask(args) => cmd_ask(&client, args).await,
        Commands::Memory { action } => cmd_memory(&client, action).await,
        Commands::Config { action } => cmd_config(&client, action).await,
        Commands::Identity { action } => cmd_identity(&client, action).await,
        Commands::Architecture { action } => cmd_architecture(&client, action).await,
        Commands::Skill { action } => cmd_skill(&client, action).await,
        Commands::Server { action } => cmd_server(action).await,
        Commands::Session { action } => cmd_session(&client, action).await,
        Commands::Task { action } => cmd_task(&client, action).await,
        Commands::Tool { action } => cmd_tool(&client, action).await,
        Commands::Health => cmd_health(&client).await,
        Commands::Update { check } => cmd_update(check).await,
        Commands::Cost => cmd_cost(&client).await,
        Commands::Doctor => cmd_doctor(&client).await,
        Commands::Init { force } => cmd_init(force).await,
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn parse_agent_mode(mode: &str) -> pb::AgentMode {
    match mode.to_lowercase().as_str() {
        "plan" => pb::AgentMode::Plan,
        "agent" => pb::AgentMode::Agent,
        "yolo" => pb::AgentMode::Yolo,
        _ => pb::AgentMode::Unspecified,
    }
}

async fn cmd_ask(client: &MaixClient, args: AskArgs) {
    let mut prompt = if args.question.is_empty() {
        // Read from stdin
        use std::io::Read;
        let mut buf = String::new();
        if atty::is(atty::Stream::Stdin) {
            eprintln!("Error: no question provided. Usage: maix ask <question> or pipe input via stdin");
            std::process::exit(1);
        }
        std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
            eprintln!("Error reading stdin: {e}");
            std::process::exit(1);
        });
        buf
    } else {
        args.question.join(" ")
    };

    if let Some(f) = &args.file {
        match tokio::fs::read_to_string(f).await {
            Ok(content) => prompt.push_str(&format!("\n\nFile {}:\n{}", f.display(), content)),
            Err(e) => {
                eprintln!("Error reading {}: {e}", f.display());
                std::process::exit(1);
            }
        }
    }

    if prompt.trim().is_empty() {
        eprintln!("Error: empty prompt");
        std::process::exit(1);
    }

    let is_json = args.output_format == "json";
    let mut json_output = serde_json::json!({
        "response": "",
        "tool_calls": [],
        "tokens": { "prompt": 0, "completion": 0, "total": 0 },
    });

    match start_chat(client, &prompt, None).await {
        Ok(mut handle) => {
            // Set agent mode if specified
            if let Some(mode_str) = &args.mode {
                let mode = parse_agent_mode(mode_str);
                if let Err(e) = handle.send_set_mode(mode).await {
                    eprintln!("Warning: failed to set mode: {e}");
                }
            }

            let mut full_text = String::new();
            let mut had_delta = false;
            let mut had_reasoning = false;
            let mut tool_calls_log: Vec<serde_json::Value> = Vec::new();

            loop {
                match handle.recv().await {
                    Some(Ok(msg)) => {
                        if let Some(out) = msg.output {
                            match out {
                                pb::chat_output::Output::TextDelta(d) => {
                                    full_text.push_str(&d.text);
                                    if is_json {
                                        // Collect only, don't print
                                    } else {
                                        if had_reasoning {
                                            print!("\x1b[0m");
                                            had_reasoning = false;
                                        }
                                        print!("{}", d.text);
                                        let _ = std::io::stdout().flush();
                                    }
                                    had_delta = true;
                                }
                                pb::chat_output::Output::ReasoningDelta(d)
                                    if !is_json && args.verbose => {
                                        print!("\x1b[2m{}\x1b[0m", d.text);
                                        let _ = std::io::stdout().flush();
                                        had_reasoning = true;
                                    }
                                pb::chat_output::Output::ToolCall(tc) => {
                                    tool_calls_log.push(serde_json::json!({
                                        "tool": tc.tool_name,
                                        "arguments": format!("{:?}", tc.arguments),
                                    }));
                                    if !is_json {
                                        eprintln!(
                                            "\n[tool: {}({})]",
                                            tc.tool_name,
                                            truncate_str(&format!("{:?}", tc.arguments), 120)
                                        );
                                    }
                                }
                                pb::chat_output::Output::ToolResult(tr)
                                    if !is_json => {
                                        eprintln!("[result: {}]", truncate_str(&tr.result, 200));
                                    }
                                pb::chat_output::Output::Complete(c) => {
                                    if is_json {
                                        json_output["response"] = serde_json::Value::String(full_text.clone());
                                        json_output["tool_calls"] = serde_json::Value::Array(tool_calls_log.clone());
                                        if let Some(u) = c.usage.as_ref() {
                                            json_output["tokens"] = serde_json::json!({
                                                "prompt": u.prompt_tokens,
                                                "completion": u.completion_tokens,
                                                "total": u.total_tokens,
                                            });
                                        }
                                        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                                    } else {
                                        if let Some(u) = c.usage {
                                            eprintln!(
                                                "\n[tokens: {} in / {} out | total: {}]",
                                                u.prompt_tokens, u.completion_tokens, u.total_tokens
                                            );
                                        }
                                        if !had_delta && !c.text.is_empty() {
                                            println!("{}", c.text);
                                        } else if had_delta {
                                            println!();
                                        }
                                    }
                                    break;
                                }
                                pb::chat_output::Output::Error(e) => {
                                    if is_json {
                                        json_output["error"] = serde_json::Value::String(e.message.clone());
                                        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                                    } else {
                                        eprintln!("\nError: {}", e.message);
                                    }
                                    std::process::exit(1);
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Err(e)) => {
                        if is_json {
                            json_output["error"] = serde_json::Value::String(e.to_string());
                            println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                        } else {
                            eprintln!("\nStream error: {e}");
                        }
                        std::process::exit(1);
                    }
                    None => break,
                }
            }
        }
        Err(e) => {
            if is_json {
                json_output["error"] = serde_json::Value::String(e.to_string());
                println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
            } else {
                eprintln!("Error: {e}");
            }
            std::process::exit(1);
        }
    }
}

async fn cmd_memory(client: &MaixClient, action: MemoryAction) {
    match action {
        MemoryAction::List => match client.search_memory("", 50).await {
            Ok(entries) => {
                if entries.is_empty() {
                    println!("(no memories)");
                } else {
                    for e in &entries {
                        println!(
                            "[{}] kind={}: {}",
                            &e.id[..e.id.len().min(8)],
                            e.kind,
                            truncate_str(&e.content, 120)
                        );
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        MemoryAction::Search { query } => {
            let q = query.join(" ");
            match client.search_memory(&q, 10).await {
                Ok(entries) => {
                    for e in &entries {
                        println!(
                            "[{}] kind={}: {}",
                            &e.id[..e.id.len().min(8)],
                            e.kind,
                            truncate_str(&e.content, 120)
                        );
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        MemoryAction::Forget { id } => match client.forget_memory(&id).await {
            Ok(_) => println!("Forgot: {id}"),
            Err(e) => eprintln!("Error: {e}"),
        },
    }
}

async fn cmd_identity(client: &MaixClient, action: IdentityAction) {
    match action {
        IdentityAction::List => match client.list_agents().await {
            Ok(resp) => {
                for a in &resp.agents {
                    println!("  {}", a.name);
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        IdentityAction::Activate { name } => match client.activate_agent(&name).await {
            Ok(_) => println!("Activated: {name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
    }
}

async fn cmd_architecture(client: &MaixClient, action: ArchitectureAction) {
    match action {
        ArchitectureAction::List => match client.list_architectures().await {
            Ok(archs) => {
                if archs.is_empty() {
                    println!("No architectures found.");
                } else {
                    for a in &archs {
                        println!("{}: {} (nodes={}, flows={})", a.name, a.description.as_deref().unwrap_or(""), a.node_count, a.flow_count);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        ArchitectureAction::Show { name } => match client.list_architectures().await {
            Ok(archs) => {
                if let Some(a) = archs.iter().find(|a| a.name == name) {
                    println!("Name: {}", a.name);
                    println!("ID: {}", a.id);
                    if let Some(desc) = &a.description {
                        println!("Description: {desc}");
                    }
                    println!("Topology: {}", a.topology);
                    println!("Nodes: {}", a.node_count);
                    println!("Flows: {}", a.flow_count);
                } else {
                    eprintln!("Architecture '{name}' not found");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        ArchitectureAction::Run { name, input } => {
            let input_str = input.join(" ");
            match client.run_architecture(&name, &input_str).await {
                Ok(resp) => {
                    let mut stream = resp.into_inner();
                    use tokio_stream::StreamExt;
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(output) => {
                                if let Some(out) = output.output {
                                    match out {
                                        pb::run_architecture_output::Output::TextDelta(text) => {
                                            print!("{}", text);
                                            let _ = std::io::stdout().flush();
                                        }
                                        pb::run_architecture_output::Output::Complete(text) => {
                                            if !text.is_empty() {
                                                println!("{}", text);
                                            }
                                            println!();
                                        }
                                        pb::run_architecture_output::Output::Error(err) => {
                                            eprintln!("\nError: {err}");
                                            std::process::exit(1);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("\nStream error: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

async fn cmd_skill(client: &MaixClient, action: SkillAction) {
    match action {
        SkillAction::Install { source } => {
            // Install skill from local path or URL
            let source_path = std::path::Path::new(&source);
            let skills_dir = dirs_next()
                .map(|h| h.join(".maix").join("skills"))
                .unwrap_or_else(|| std::path::PathBuf::from(".maix/skills"));

            if source_path.exists() {
                // Local path install
                let skill_name = source_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".into());
                let dest = skills_dir.join(&skill_name);

                if dest.exists() {
                    eprintln!("Skill '{}' already exists at {}", skill_name, dest.display());
                    eprintln!("Remove it first or use a different name.");
                    std::process::exit(1);
                }

                // Copy directory
                if source_path.is_dir() {
                    match copy_dir_recursive(source_path, &dest) {
                        Ok(_) => {
                            // Validate manifest
                            match maix_skills::manifest::SkillManifest::from_dir(&dest) {
                                Ok(manifest) => {
                                    println!("Installed skill '{}' v{} from {}", manifest.skill.name, manifest.skill.version, source);
                                    println!("  Location: {}", dest.display());
                                    if let Some(desc) = &manifest.skill.description {
                                        println!("  Description: {}", desc);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Warning: skill installed but manifest invalid: {e}");
                                    println!("Installed to {}", dest.display());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error copying skill: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Source must be a directory");
                    std::process::exit(1);
                }
            } else if source.starts_with("http://") || source.starts_with("https://") {
                eprintln!("URL install not yet supported. Please download manually and install from local path.");
                std::process::exit(1);
            } else if source.contains(':') && source.contains('/') {
                // GitHub shorthand: user/repo
                eprintln!("GitHub shorthand install not yet supported.");
                eprintln!("Use: git clone https://github.com/{} /tmp/skill && maix skill install /tmp/skill", source);
                std::process::exit(1);
            } else {
                eprintln!("Source not found: {}", source);
                std::process::exit(1);
            }
        }
        SkillAction::List => match client.list_skills().await {
            Ok(list) => {
                for s in &list {
                    println!(
                        "  {} v{} ({})",
                        s.name,
                        s.version,
                        if s.enabled { "enabled" } else { "disabled" }
                    );
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        SkillAction::Enable { name } => match client.enable_skill(&name).await {
            Ok(_) => println!("Enabled: {name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
        SkillAction::Disable { name } => match client.disable_skill(&name).await {
            Ok(_) => println!("Disabled: {name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
    }
}

const SERVICE_NAME: &str = "MaixAgent";
const SERVICE_DISPLAY: &str = "Maix-Agent AI Assistant";

async fn cmd_server(action: ServerAction) {
    match action {
        ServerAction::Install => {
            #[cfg(windows)]
            {
                // Find the maix binary path
                let maix_exe = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("maix.exe")))
                    .filter(|p| p.exists());

                let exe_path = match maix_exe {
                    Some(p) => p,
                    None => {
                        eprintln!("Error: maix.exe not found next to maix-cli.exe");
                        eprintln!("Ensure maix-server is built and in the same directory.");
                        std::process::exit(1);
                    }
                };

                // Use sc.exe to create the service
                let output = std::process::Command::new("sc.exe")
                    .args([
                        "create",
                        SERVICE_NAME,
                        "binPath=",
                        &format!("{} --service", exe_path.display()),
                        "start=",
                        "auto",
                        "DisplayName=",
                        SERVICE_DISPLAY,
                    ])
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' installed successfully.", SERVICE_NAME);
                        println!("  Binary: {}", exe_path.display());
                        println!("  Start:  auto (boot)");
                        println!("  Use 'maix server start' to start now.");
                    }
                    Ok(o) => {
                        eprintln!("sc.exe failed: {}", String::from_utf8_lossy(&o.stderr));
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                // systemd unit file for Linux
                let unit_content = format!(
                    "[Unit]\n\
                     Description=Maix-Agent AI Assistant\n\
                     After=network.target\n\n\
                     [Service]\n\
                     Type=simple\n\
                     ExecStart=/usr/local/bin/maix --foreground\n\
                     Restart=on-failure\n\
                     RestartSec=5\n\n\
                     [Install]\n\
                     WantedBy=multi-user.target\n"
                );
                let unit_path = "/etc/systemd/system/maix-agent.service";
                match std::fs::write(unit_path, unit_content) {
                    Ok(_) => {
                        println!("Service unit file written to {unit_path}");
                        println!("Run: sudo systemctl daemon-reload && sudo systemctl enable maix-agent");
                    }
                    Err(e) => {
                        eprintln!("Error writing unit file: {e}");
                        eprintln!("Try running with sudo.");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Uninstall => {
            #[cfg(windows)]
            {
                // Stop first, then delete
                let _ = std::process::Command::new("sc.exe")
                    .args(["stop", SERVICE_NAME])
                    .output();

                let output = std::process::Command::new("sc.exe")
                    .args(["delete", SERVICE_NAME])
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' uninstalled.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        eprintln!("sc.exe failed: {}", String::from_utf8_lossy(&o.stderr));
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = std::process::Command::new("systemctl")
                    .args(["disable", "maix-agent"])
                    .status();
                match std::fs::remove_file("/etc/systemd/system/maix-agent.service") {
                    Ok(_) => {
                        println!("Service removed. Run: sudo systemctl daemon-reload");
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Start => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["start", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' started.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stderr.contains("1056") || stdout.contains("1056") {
                            println!("Service '{}' is already running.", SERVICE_NAME);
                        } else {
                            eprintln!("sc.exe failed: {stderr}{stdout}");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["start", "maix-agent"])
                    .status()
                {
                    Ok(s) if s.success() => println!("Service started."),
                    Ok(_) => {
                        eprintln!("Failed to start service.");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Stop => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["stop", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' stopped.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stderr.contains("1062") || stdout.contains("1062") {
                            println!("Service '{}' is not running.", SERVICE_NAME);
                        } else {
                            eprintln!("sc.exe failed: {stderr}{stdout}");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["stop", "maix-agent"])
                    .status()
                {
                    Ok(s) if s.success() => println!("Service stopped."),
                    Ok(_) => {
                        eprintln!("Failed to stop service.");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Status => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["query", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stdout.contains("does not exist") || stdout.contains("1060") {
                            println!("Service '{}' is not installed.", SERVICE_NAME);
                        } else {
                            println!("{}", stdout.trim());
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["status", "maix-agent"])
                    .output()
                {
                    Ok(o) => {
                        println!("{}", String::from_utf8_lossy(&o.stdout));
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

async fn cmd_session(client: &MaixClient, action: SessionAction) {
    match action {
        SessionAction::List => match client.list_sessions().await {
            Ok(sessions) => {
                if sessions.is_empty() {
                    println!("(no saved sessions)");
                } else {
                    for s in &sessions {
                        println!(
                            "{} | {} | msgs: {} | {}",
                            &s.id[..s.id.len().min(8)],
                            s.name,
                            s.message_count,
                            s.updated_at
                        );
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        SessionAction::Show { id } => match client.get_session_messages(&id, 100).await {
            Ok(msgs) => {
                if msgs.is_empty() {
                    println!("(no messages in session {id})");
                } else {
                    for m in &msgs {
                        println!("[{}] {}", m.role, truncate_str(&m.content, 200));
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        SessionAction::Delete { id } => match client.delete_session(&id).await {
            Ok(true) => println!("Deleted session: {id}"),
            Ok(false) => println!("Session not found: {id}"),
            Err(e) => eprintln!("Error: {e}"),
        },
        SessionAction::Fork { id, from, name } => {
            // If from is specified, get the message at that index to use as from_message_id
            let from_id = match from {
                Some(idx) => {
                    match client.get_session_messages(&id, 1000).await {
                        Ok(msgs) => {
                            if idx >= msgs.len() {
                                eprintln!("Error: message index {} out of range (session has {} messages)", idx, msgs.len());
                                std::process::exit(1);
                            }
                            msgs[idx].created_at.clone()
                        }
                        Err(e) => {
                            eprintln!("Error getting messages: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                None => String::new(), // Empty = fork from end
            };

            match client.fork_session(&id, &from_id, name.as_deref()).await {
                Ok(resp) => {
                    println!("Forked session: {}", resp.new_session_id);
                    println!("  Copied {} messages", resp.copied_messages);
                }
                Err(e) => {
                    eprintln!("Error forking session: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

async fn cmd_health(client: &MaixClient) {
    match client.health_check().await {
        Ok(h) => {
            println!("Status:   {}", h.status);
            println!("Version:  {}", h.version);
            println!("Uptime:   {}s", h.uptime_secs);
            println!("Sessions: {}", h.active_sessions);
            println!("Queue:    {}", h.queue_depth);
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}

async fn cmd_doctor(client: &MaixClient) {
    let config = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
    let results = doctor::run_diagnostics(&config, client).await;
    println!("{}", doctor::format_diagnostics(&results));
}

async fn cmd_cost(client: &MaixClient) {
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
            // Estimate based on default pricing
            let estimated_input = total_tokens * 70 / 100; // rough estimate: 70% input
            let estimated_output = total_tokens * 30 / 100; // 30% output
            let input_cost = estimated_input as f64 * pricing.input_per_million / 1_000_000.0;
            let output_cost = estimated_output as f64 * pricing.output_per_million / 1_000_000.0;

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

fn format_number(n: u64) -> String {
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

async fn cmd_update(check_only: bool) {
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

    let checker = maix_core::update::UpdateChecker::new(current);
    match checker.parse_release_json(&body) {
        Some(info) => {
            println!("\nNew version available: v{} -> v{}", info.current, info.latest);
            if !info.release_notes.is_empty() {
                println!("\nRelease notes:");
                // Show first 500 chars of release notes
                let notes = if info.release_notes.len() > 500 {
                    format!("{}...", &info.release_notes[..500])
                } else {
                    info.release_notes.clone()
                };
                println!("{}", notes);
            }
            println!("\nDownload: {}", info.download_url);

            if check_only {
                return;
            }

            // Download and install
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

async fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
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

async fn cmd_config(client: &MaixClient, action: Option<ConfigAction>) {
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
            let parts: Vec<&str> = key.splitn(2, '.').collect();
            let (section, config_key) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                ("general", key.as_str())
            };
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

async fn cmd_task(client: &MaixClient, action: TaskAction) {
    match action {
        TaskAction::Submit { description, input, priority } => {
            let input_str = input.join(" ");
            match client.submit_task(&description, &input_str, priority).await {
                Ok(task_id) => println!("Submitted task: {task_id}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        TaskAction::List => match client.list_tasks().await {
            Ok(tasks) => {
                if tasks.is_empty() {
                    println!("No tasks.");
                } else {
                    for t in &tasks {
                        println!("{}: {} [{}] priority={}", t.id, t.description, t.status, t.priority);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        TaskAction::Cancel { id } => match client.cancel_task(&id).await {
            Ok(true) => println!("Cancelled task {id}"),
            Ok(false) => {
                eprintln!("Task {id} not found");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        TaskAction::Suspend { id } => match client.suspend_task(&id).await {
            Ok(true) => println!("Suspended task {id}"),
            Ok(false) => {
                eprintln!("Task {id} not found");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        TaskAction::Resume { id } => match client.resume_task(&id).await {
            Ok(true) => println!("Resumed task {id}"),
            Ok(false) => {
                eprintln!("Task {id} not found");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        TaskAction::Repriority { id, priority } => match client.reprioritize_task(&id, priority).await {
            Ok(true) => println!("Updated task {id} priority to {priority}"),
            Ok(false) => {
                eprintln!("Task {id} not found");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
    }
}

async fn cmd_tool(client: &MaixClient, action: ToolAction) {
    match action {
        ToolAction::List => match client.list_tools().await {
            Ok(tools) => {
                for t in &tools {
                    println!("{}: {} (risk={})", t.name, t.description, t.risk_level);
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        ToolAction::Call { name, args } => {
            let args_str = args.join(" ");
            let args_json: serde_json::Value = if args_str.is_empty() {
                serde_json::json!({})
            } else {
                match serde_json::from_str(&args_str) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Invalid JSON args: {e}");
                        std::process::exit(1);
                    }
                }
            };
            let arguments = maix_core::json_to_prost_struct(args_json);
            let session_id = uuid::Uuid::new_v4().to_string();
            match client.call_tool(&name, Some(arguments), &session_id, ".").await {
                Ok(resp) => {
                    if let Some(err) = resp.error {
                        eprintln!("Tool error: {err}");
                        std::process::exit(1);
                    } else {
                        println!("{}", resp.result);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

async fn cmd_init(force: bool) {
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
