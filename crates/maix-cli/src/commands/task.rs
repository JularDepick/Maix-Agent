//! Task management commands.

use crate::cli::TaskAction;
use maix_core::client::MaixClient;

pub async fn cmd_task(client: &MaixClient, action: TaskAction) {
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
