//! MCP client — connect to external MCP servers, discover tools, and call them.

use super::transport::MCPTransport;
use super::types::*;
use serde_json::Value;

/// An MCP client connected to one MCP server.
pub struct MCPClient {
    transport: MCPTransport,
    next_id: u64,
    server_info: Option<ImplementationInfo>,
    server_caps: Option<ServerCapabilities>,
}

impl MCPClient {
    /// Connect to an MCP server via stdio subprocess.
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self, String> {
        let transport = MCPTransport::stdio(command, args).await?;
        let mut client = Self { transport, next_id: 1, server_info: None, server_caps: None };
        client.initialize().await?;
        Ok(client)
    }

    /// Initialize the MCP session (handshake).
    async fn initialize(&mut self) -> Result<(), String> {
        let params = serde_json::to_value(InitializeParams {
            protocol_version: MCP_VERSION.into(),
            capabilities: ClientCapabilities { roots: None, sampling: None },
            client_info: ImplementationInfo {
                name: "maix-agent".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        })
        .map_err(|e| format!("serialize: {e}"))?;

        let result = self.request("initialize", Some(params)).await?;
        let init: InitializeResult =
            serde_json::from_value(result).map_err(|e| format!("parse init: {e}"))?;

        // Send initialized notification
        let notif = JsonRpcNotification::new("notifications/initialized", None);
        let msg = serde_json::to_string(&notif).map_err(|e| format!("serialize: {e}"))?;
        self.transport.send(&msg).await?;

        self.server_info = Some(init.server_info);
        self.server_caps = Some(init.capabilities);
        Ok(())
    }

    /// Send a request and wait for the response.
    pub async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest::new(id, method, params);
        let msg = serde_json::to_string(&req).map_err(|e| format!("serialize: {e}"))?;
        self.transport.send(&msg).await?;

        let line = self.transport.recv().await?;
        let resp: JsonRpcResponse =
            serde_json::from_str(&line).map_err(|e| format!("parse response: {e}"))?;

        if let Some(err) = resp.error {
            return Err(format!("MCP error {}: {}", err.code, err.message));
        }
        resp.result.ok_or_else(|| "no result".into())
    }

    /// List tools exposed by the server.
    pub async fn list_tools(&mut self) -> Result<Vec<MCPTool>, String> {
        let result = self.request("tools/list", None).await?;
        let list: ListToolsResult =
            serde_json::from_value(result).map_err(|e| format!("parse tools: {e}"))?;
        Ok(list.tools)
    }

    /// Call a tool on the server.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, String> {
        let params = serde_json::to_value(CallToolParams {
            name: name.into(),
            arguments,
        })
        .map_err(|e| format!("serialize: {e}"))?;

        let result = self.request("tools/call", Some(params)).await?;
        serde_json::from_value(result).map_err(|e| format!("parse tool result: {e}"))
    }

    /// List resources exposed by the server.
    pub async fn list_resources(&mut self) -> Result<Vec<MCPResource>, String> {
        let result = self.request("resources/list", None).await?;
        let list: ListResourcesResult =
            serde_json::from_value(result).map_err(|e| format!("parse resources: {e}"))?;
        Ok(list.resources)
    }

    /// List prompts exposed by the server.
    pub async fn list_prompts(&mut self) -> Result<Vec<MCPPrompt>, String> {
        let result = self.request("prompts/list", None).await?;
        let list: ListPromptsResult =
            serde_json::from_value(result).map_err(|e| format!("parse prompts: {e}"))?;
        Ok(list.prompts)
    }

    pub fn server_name(&self) -> &str {
        self.server_info
            .as_ref()
            .map(|i| i.name.as_str())
            .unwrap_or("unknown")
    }
}
