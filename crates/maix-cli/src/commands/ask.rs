//! Chat with the agent.

use crate::cli::AskArgs;
use super::{parse_agent_mode, truncate_str};
use maix_core::client::{start_chat, MaixClient};
use maix_core::proto::maix::core::v1 as pb;
use std::io::Write;

pub async fn cmd_ask(client: &MaixClient, args: AskArgs) {
    let mut prompt = if args.question.is_empty() {
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
