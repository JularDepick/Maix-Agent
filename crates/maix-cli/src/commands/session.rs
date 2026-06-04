//! Session management commands.

use crate::cli::SessionAction;
use super::truncate_str;
use maix_core::client::MaixClient;

pub async fn cmd_session(client: &MaixClient, action: SessionAction) {
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
                None => String::new(),
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
