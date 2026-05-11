use clap::{Parser, Subcommand};
use maix_core::client::{start_chat, MaixClient};
use maix_core::proto::maix::core::v1 as pb;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "maix-cli", version, about = "Multi-modal AI Agent for 1M-context LLMs")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, default_value = "deepseek", global = true)]
    model: String,

    #[arg(short, long, default_value = ".")]
    workdir: PathBuf,

    #[arg(long, default_value = "agent")]
    mode: String,

    #[arg(long, default_value = "127.0.0.1:26506", global = true)]
    server: String,
}

#[derive(Subcommand)]
enum Commands {
    Chat,
    Ask {
        question: Vec<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    Memory { #[command(subcommand)] action: MemoryAction },
    Config { #[command(subcommand)] action: ConfigAction },
    Identity { #[command(subcommand)] action: IdentityAction },
    Architecture { #[command(subcommand)] action: ArchitectureAction },
    Skill { #[command(subcommand)] action: SkillAction },
}

#[derive(Subcommand)]
enum IdentityAction { List, Use { name: String } }

#[derive(Subcommand)]
enum MemoryAction {
    List,
    Search { query: Vec<String> },
    Forget { id: String },
}

#[derive(Subcommand)]
enum ArchitectureAction {
    List,
    Show { name: String },
    Run { name: String, input: Vec<String> },
}

#[derive(Subcommand)]
enum SkillAction {
    Install { path: PathBuf },
    List,
    Enable { name: String },
    Disable { name: String },
}

#[derive(Subcommand)]
enum ConfigAction { Show }

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

    let client = MaixClient::connect(&cli.server).await.unwrap_or_else(|e| {
        eprintln!("Failed to connect to maix at {}: {e}", cli.server);
        eprintln!("Is maix running? Try: maix --foreground");
        std::process::exit(1);
    });

    match client.health_check().await {
        Ok(h) => {
            tracing::debug!(
                "Connected to maix v{} (uptime {}s)",
                h.version,
                h.uptime_secs
            );
        }
        Err(e) => {
            eprintln!("Server health check failed: {e}");
            std::process::exit(1);
        }
    }

    let mode = agent_mode_from_str(&cli.mode);

    match cli.command.unwrap_or(Commands::Chat) {
        Commands::Chat => {
            println!(
                "Maix-Agent v{} | server: {} | mode: {:?}",
                env!("CARGO_PKG_VERSION"),
                cli.server,
                cli.mode,
            );
            println!("Type /help for commands, /exit to quit.\n");
            run_repl(&client, mode).await;
        }
        Commands::Ask { question, file } => {
            let mut prompt = question.join(" ");
            if let Some(f) = &file {
                if let Ok(content) = tokio::fs::read_to_string(f).await {
                    prompt.push_str(&format!("\n\nFile {}:\n{}", f.display(), content));
                }
            }
            match start_chat(&client, &prompt, None).await {
                Ok(mut handle) => {
                    let mut had_delta = false;
                    let mut had_reasoning = false;
                    loop {
                        match handle.recv().await {
                            Some(Ok(msg)) => {
                                if let Some(out) = msg.output {
                                    match out {
                                        pb::chat_output::Output::TextDelta(d) => {
                                            if had_reasoning {
                                                println!("\x1b[0m");
                                                had_reasoning = false;
                                            }
                                            print!("{}", d.text);
                                            let _ = std::io::stdout().flush();
                                            had_delta = true;
                                        }
                                        pb::chat_output::Output::ReasoningDelta(d) => {
                                            print!("\x1b[2m{}\x1b[0m", d.text);
                                            let _ = std::io::stdout().flush();
                                            had_reasoning = true;
                                        }
                                        pb::chat_output::Output::Complete(c) => {
                                            if !had_delta && !c.text.is_empty() {
                                                println!("{}", c.text);
                                            } else if had_delta {
                                                println!();
                                            }
                                            break;
                                        }
                                        pb::chat_output::Output::Error(e) => {
                                            eprintln!("\nError: {}", e.message);
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                eprintln!("Stream error: {e}");
                                break;
                            }
                            None => break,
                        }
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        Commands::Memory { action } => handle_memory(&client, action).await,
        Commands::Config { action } => handle_config(action),
        Commands::Identity { action } => handle_identity(&client, action).await,
        Commands::Architecture { action } => handle_architecture(&client, action).await,
        Commands::Skill { action } => handle_skill(&client, action).await,
    }
}

async fn run_repl(client: &MaixClient, _mode: i32) {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut input = String::new();

    loop {
        input.clear();
        print!("> ");
        let _ = stdout.flush();
        let n = stdin.lock().read_line(&mut input).unwrap_or(0);
        if n == 0 {
            break; // EOF
        }
        let line = input.trim().to_string();
        if line.is_empty() {
            continue;
        }

        match line.as_str() {
            "/exit" | "/quit" => break,
            "/help" => {
                println!("Commands:");
                println!("  /exit       Quit");
                println!("  /help       Show this help");
                println!("  /memory     Show memories");
                println!("  /mode <m>   Switch mode (plan/agent/yolo)");
                continue;
            }
            "/memory" => {
                match client.search_memory("", 50).await {
                    Ok(entries) => {
                        if entries.is_empty() {
                            println!("(no memories)");
                        } else {
                            for e in entries.iter().take(10) {
                                println!(
                                    "  [{}] kind={}: {}",
                                    &e.id[..e.id.len().min(8)],
                                    e.kind,
                                    truncate_str(&e.content, 100)
                                );
                            }
                            if entries.len() > 10 {
                                println!("  ... and {} more", entries.len() - 10);
                            }
                        }
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
                continue;
            }
            cmd if cmd.starts_with("/mode ") => {
                let _mode_str = &cmd[6..];
                println!("Mode switching via gRPC not yet implemented in server");
                continue;
            }
            _ => {}
        }

        match start_chat(client, &line, None).await {
            Ok(mut handle) => {
                let mut had_delta = false;
                let mut had_reasoning = false;
                loop {
                    match handle.recv().await {
                        Some(Ok(msg)) => {
                            if let Some(out) = msg.output {
                                match out {
                                    pb::chat_output::Output::TextDelta(d) => {
                                        if had_reasoning {
                                            println!("\x1b[0m");
                                            had_reasoning = false;
                                        }
                                        print!("{}", d.text);
                                        let _ = stdout.flush();
                                        had_delta = true;
                                    }
                                    pb::chat_output::Output::ReasoningDelta(d) => {
                                        print!("\x1b[2m{}\x1b[0m", d.text);
                                        let _ = stdout.flush();
                                        had_reasoning = true;
                                    }
                                    pb::chat_output::Output::ToolCall(tc) => {
                                        println!(
                                            "\n[tool: {}({:?})]",
                                            tc.tool_name, tc.arguments
                                        );
                                    }
                                    pb::chat_output::Output::ToolResult(tr) => {
                                        println!(
                                            "[result: {}]",
                                            truncate_str(&tr.result, 200)
                                        );
                                    }
                                    pb::chat_output::Output::Complete(c) => {
                                        if let Some(u) = c.usage {
                                            println!(
                                                "\n---\n[tokens: {} in / {} out | total: {}]",
                                                u.prompt_tokens,
                                                u.completion_tokens,
                                                u.total_tokens
                                            );
                                        }
                                        if had_delta {
                                            println!();
                                        }
                                        break;
                                    }
                                    pb::chat_output::Output::Error(e) => {
                                        eprintln!("\nError: {}", e.message);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(Err(e)) => {
                            eprintln!("\nStream error: {e}");
                            break;
                        }
                        None => break,
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        }
    }
    println!("Goodbye!");
}

async fn handle_memory(client: &MaixClient, action: MemoryAction) {
    match action {
        MemoryAction::List => {
            match client.search_memory("", 100).await {
                Ok(entries) => {
                    for e in entries {
                        println!(
                            "[{}] kind={}: {}",
                            &e.id[..e.id.len().min(12)],
                            e.kind,
                            truncate_str(&e.content, 150)
                        );
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        MemoryAction::Search { query } => {
            match client.search_memory(&query.join(" "), 10).await {
                Ok(entries) => {
                    for e in entries {
                        println!("[{}] kind={}: {}", e.id, e.kind, e.content);
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        MemoryAction::Forget { id } => {
            match client.forget_memory(&id).await {
                Ok(true) => println!("Forgot: {id}"),
                Ok(false) => eprintln!("Not found: {id}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}

fn handle_config(_action: ConfigAction) {
    let cfg = maix_core::Config::load().unwrap_or_else(|_| maix_core::Config::minimal());
    println!("{:#?}", cfg);
}

async fn handle_identity(client: &MaixClient, action: IdentityAction) {
    match action {
        IdentityAction::List => {
            match client.list_agents().await {
                Ok(resp) => {
                    println!("Available identities:");
                    for agent in &resp.agents {
                        let active = if resp.active.as_deref() == Some(&agent.name) {
                            " *"
                        } else {
                            ""
                        };
                        println!(
                            "  - {}{} | {} | tone: {}",
                            agent.name, active, agent.description, agent.tone
                        );
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        IdentityAction::Use { name } => {
            match client.activate_agent(&name).await {
                Ok(resp) => {
                    if resp.activated {
                        println!("Activated identity: {name}");
                    } else {
                        eprintln!("Error: {}", resp.system_prompt);
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}

async fn handle_architecture(client: &MaixClient, action: ArchitectureAction) {
    match action {
        ArchitectureAction::List => {
            match client.list_architectures().await {
                Ok(archs) => {
                    println!("Available architectures:\n");
                    for a in &archs {
                        println!(
                            "  {} — {} ({} nodes, {} flows, topology: {})",
                            a.name,
                            a.description.as_deref().unwrap_or(""),
                            a.node_count,
                            a.flow_count,
                            a.topology
                        );
                    }
                    println!();
                    println!("Use `maix architecture show <name>` for details.");
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        ArchitectureAction::Show { name } => {
            match client.list_architectures().await {
                Ok(archs) => {
                    if let Some(a) = archs.iter().find(|a| a.name == name) {
                        println!("Name: {}", a.name);
                        println!("Description: {}", a.description.as_deref().unwrap_or(""));
                        println!("Topology: {}", a.topology);
                        println!("Nodes: {}, Flows: {}", a.node_count, a.flow_count);
                        println!("ID: {}", a.id);
                    } else {
                        eprintln!("Unknown: {name}");
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        ArchitectureAction::Run { name, input } => {
            match client.run_architecture(&name, &input.join(" ")).await {
                Ok(resp) => {
                    let mut stream = resp.into_inner();
                    use tokio_stream::StreamExt;
                    while let Some(Ok(msg)) = stream.next().await {
                        if let Some(out) = msg.output {
                            match out {
                                pb::run_architecture_output::Output::Complete(s) => {
                                    println!("{s}");
                                }
                                pb::run_architecture_output::Output::Error(e) => {
                                    eprintln!("Error: {e}");
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}

async fn handle_skill(client: &MaixClient, action: SkillAction) {
    match action {
        SkillAction::Install { path } => {
            match client.install_skill(&path.to_string_lossy()).await {
                Ok(resp) => println!("Installed: {} v{}", resp.name, resp.version),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        SkillAction::List => {
            match client.list_skills().await {
                Ok(skills) => {
                    if skills.is_empty() {
                        println!("No skills installed.");
                    } else {
                        for s in skills {
                            let status = if s.enabled { "enabled" } else { "disabled" };
                            println!("  - {} v{} [{}] runtime={}", s.name, s.version, status, s.runtime);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        SkillAction::Enable { name } => {
            match client.enable_skill(&name).await {
                Ok(true) => println!("Enabled: {name}"),
                Ok(false) => eprintln!("Not found: {name}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        SkillAction::Disable { name } => {
            match client.disable_skill(&name).await {
                Ok(true) => println!("Disabled: {name}"),
                Ok(false) => eprintln!("Not found: {name}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}
