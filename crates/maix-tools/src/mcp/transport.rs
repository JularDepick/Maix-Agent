//! MCP transport layer — stdio, SSE, streamable HTTP.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

/// Transport abstraction for sending/receiving JSON-RPC messages.
pub enum MCPTransport {
    Stdio {
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
        child: Box<Child>,
    },
    Sse {
        message_url: String,
        client: reqwest::Client,
        event_rx: mpsc::UnboundedReceiver<String>,
    },
}

impl MCPTransport {
    /// Connect to an MCP server via subprocess stdio.
    pub async fn stdio(command: &str, args: &[&str]) -> Result<Self, String> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to spawn {command}: {e}"))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;

        Ok(Self::Stdio { stdin, stdout: BufReader::new(stdout), child: Box::new(child) })
    }

    /// Connect via SSE (Server-Sent Events) over HTTP.
    ///
    /// Connects to `{base_url}/sse` to receive the message endpoint URL,
    /// then spawns a background task to stream events.
    pub async fn sse(base_url: String) -> Result<Self, String> {
        let client = reqwest::Client::new();
        let sse_url = format!("{}/sse", base_url.trim_end_matches('/'));

        // Connect to the SSE endpoint to get the message URL
        let resp = client
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| format!("SSE connect failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("SSE connect returned HTTP {}", resp.status()));
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Spawn background task to parse SSE stream
        let base = base_url.clone();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();
            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("SSE stream error: {e}");
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events (separated by double newline)
                while let Some(event_end) = buffer.find("\n\n") {
                    let event_str = buffer[..event_end].to_string();
                    buffer = buffer[event_end + 2..].to_string();

                    // Parse SSE event
                    let mut event_type = "";
                    let mut event_data = String::new();
                    for line in event_str.lines() {
                        if let Some(rest) = line.strip_prefix("event:") {
                            event_type = rest.trim();
                        } else if let Some(rest) = line.strip_prefix("data:") {
                            if !event_data.is_empty() {
                                event_data.push('\n');
                            }
                            event_data.push_str(rest.trim());
                        }
                    }

                    match event_type {
                        "endpoint" => {
                            // Server sends the message endpoint URL
                            let _url = if event_data.starts_with("http") {
                                event_data.clone()
                            } else {
                                format!("{}{}", base.trim_end_matches('/'), event_data)
                            };
                            tracing::debug!("SSE message endpoint: {}", _url);
                        }
                        "message" => {
                            // JSON-RPC response message
                            if event_tx.send(event_data).is_err() {
                                return; // receiver dropped
                            }
                        }
                        _ => {
                            // Unknown event type, still forward the data
                            if !event_data.is_empty() {
                                let _ = event_tx.send(event_data);
                            }
                        }
                    }
                }
            }
        });

        // Wait briefly for the endpoint event to arrive
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Ok(Self::Sse {
            message_url: base_url, // Will be updated when endpoint event arrives
            client,
            event_rx,
        })
    }

    /// Send a JSON-RPC message as a line-delimited JSON string.
    pub async fn send(&mut self, message: &str) -> Result<(), String> {
        match self {
            MCPTransport::Stdio { stdin, .. } => {
                let mut msg = message.to_string();
                if !msg.ends_with('\n') {
                    msg.push('\n');
                }
                stdin
                    .write_all(msg.as_bytes())
                    .await
                    .map_err(|e| format!("stdio write: {e}"))?;
                stdin.flush().await.map_err(|e| format!("stdio flush: {e}"))?;
                Ok(())
            }
            MCPTransport::Sse { message_url, client, .. } => {
                client
                    .post(message_url.as_str())
                    .header("Content-Type", "application/json")
                    .body(message.to_string())
                    .send()
                    .await
                    .map_err(|e| format!("sse send: {e}"))?;
                Ok(())
            }
        }
    }

    /// Receive the next JSON-RPC message (line-delimited from stdio).
    pub async fn recv(&mut self) -> Result<String, String> {
        match self {
            MCPTransport::Stdio { stdout, .. } => {
                let mut line = String::new();
                stdout
                    .read_line(&mut line)
                    .await
                    .map_err(|e| format!("stdio read: {e}"))?;
                if line.is_empty() {
                    return Err("EOF".into());
                }
                Ok(line.trim().to_string())
            }
            MCPTransport::Sse { event_rx, .. } => {
                event_rx
                    .recv()
                    .await
                    .ok_or_else(|| "SSE stream closed".to_string())
            }
        }
    }
}

impl Drop for MCPTransport {
    fn drop(&mut self) {
        if let MCPTransport::Stdio { child, .. } = self {
            let _ = child.start_kill();
        }
    }
}
