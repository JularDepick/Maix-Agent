//! Bridge: wrap MCP server tools as Maix Tool trait implementations.

use super::client::MCPClient;
use super::types::MCPTool;
use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Wraps a single MCP tool as a Maix Tool.
pub struct McpToolBridge {
    server_name: String,
    tool: MCPTool,
    client: Arc<Mutex<MCPClient>>,
}

impl McpToolBridge {
    pub fn new(server_name: String, tool: MCPTool, client: Arc<Mutex<MCPClient>>) -> Self {
        Self {
            server_name,
            tool,
            client,
        }
    }

    fn prefixed_name(&self) -> String {
        format!("mcp_{}_{}", self.server_name, self.tool.name)
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: self.prefixed_name(),
            description: format!("[MCP:{}] {}", self.server_name, self.tool.description),
            parameters: self.tool.input_schema.clone(),
            risk_level: RiskLevel::Network,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let mut client = self.client.lock().await;
        let result = client
            .call_tool(&self.tool.name, args)
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("MCP {}: {e}", self.server_name)))?;

        let text: String = result
            .content
            .into_iter()
            .map(|c| match c {
                super::types::ToolContent::Text { text } => text,
                super::types::ToolContent::Image { data, mime_type } => {
                    format!("[image: {mime_type} ({}) bytes]", data.len())
                }
                super::types::ToolContent::Resource { resource } => {
                    format!("[resource: {resource}]")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error.unwrap_or(false) {
            Ok(format!("MCP error: {text}"))
        } else {
            Ok(text)
        }
    }
}

/// Connect to an MCP server and return bridges for all its tools.
pub async fn connect_mcp_server(
    name: &str,
    command: &str,
    args: &[&str],
    env: &[(String, String)],
) -> MaixResult<Vec<McpToolBridge>> {
    // Set env vars for the MCP server process
    for (key, val) in env {
        std::env::set_var(key, val);
    }

    let mut client = MCPClient::connect_stdio(command, args)
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("MCP connect '{name}': {e}")))?;

    let tools = client
        .list_tools()
        .await
        .map_err(|e| maix_core::MaixError::Tool(format!("MCP list_tools '{name}': {e}")))?;

    let client = Arc::new(Mutex::new(client));
    let bridges: Vec<McpToolBridge> = tools
        .into_iter()
        .map(|t| McpToolBridge::new(name.to_string(), t, client.clone()))
        .collect();

    tracing::info!(
        "MCP server '{}': connected, {} tools discovered",
        name,
        bridges.len()
    );

    Ok(bridges)
}
