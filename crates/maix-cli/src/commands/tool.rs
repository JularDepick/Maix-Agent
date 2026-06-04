//! Tool management commands.

use crate::cli::ToolAction;
use maix_core::client::MaixClient;

pub async fn cmd_tool(client: &MaixClient, action: ToolAction) {
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
