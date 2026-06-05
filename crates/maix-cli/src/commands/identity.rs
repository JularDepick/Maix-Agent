//! Identity and architecture management commands.

use crate::cli::{ArchitectureAction, IdentityAction};
use maix_core::client::MaixClient;
use maix_core::proto::maix::core::v1 as pb;
use std::io::Write;

pub async fn cmd_identity(client: &MaixClient, action: IdentityAction) {
    match action {
        IdentityAction::List => match client.list_agents().await {
            Ok(resp) => {
                for a in &resp.agents {
                    println!("  {}", a.name);
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        IdentityAction::Activate { name } => match client.activate_agent(&name).await {
            Ok(_) => println!("Activated: {name}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
    }
}

pub async fn cmd_architecture(client: &MaixClient, action: ArchitectureAction) {
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
