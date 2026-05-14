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
        Commands::Session { action } => cmd_session(&client, action).await,
        Commands::Task { action } => cmd_task(&client, action).await,
        Commands::Tool { action } => cmd_tool(&client, action).await,
        Commands::Health => cmd_health(&client).await,
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
            let _ = source;
            eprintln!("Skill install not yet available via CLI");
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
