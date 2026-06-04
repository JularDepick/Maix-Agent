//! Memory management commands.

use crate::cli::MemoryAction;
use super::truncate_str;
use maix_core::client::MaixClient;

pub async fn cmd_memory(client: &MaixClient, action: MemoryAction) {
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
