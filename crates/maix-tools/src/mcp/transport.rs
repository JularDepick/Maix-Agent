//! MCP transport layer — stdio, SSE, streamable HTTP.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Transport abstraction for sending/receiving JSON-RPC messages.
pub enum MCPTransport {
    Stdio {
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
        child: Child,
    },
    Sse {
        base_url: String,
        client: reqwest::Client,
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

        Ok(Self::Stdio { stdin, stdout: BufReader::new(stdout), child })
    }

    /// Connect via SSE (Server-Sent Events) over HTTP.
    pub fn sse(base_url: String) -> Self {
        Self::Sse { base_url, client: reqwest::Client::new() }
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
            MCPTransport::Sse { base_url, client } => {
                client
                    .post(base_url.as_str())
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
            MCPTransport::Sse { .. } => {
                Err("SSE recv not yet implemented — use request/response mode".into())
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
