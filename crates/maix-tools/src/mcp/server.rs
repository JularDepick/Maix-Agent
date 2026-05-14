//! MCP server — expose this agent's tools as MCP resources.

use super::types::*;
use crate::{ToolCtx, ToolRegistry};

/// An MCP server that wraps a ToolRegistry.
pub struct MCPServer {
    pub name: String,
    pub version: String,
}

impl MCPServer {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    /// Handle an incoming JSON-RPC request string, return response string.
    pub async fn handle_request(&self, request_str: &str, tools: &ToolRegistry, ctx: &ToolCtx) -> String {
        let req: Result<JsonRpcRequest, _> = serde_json::from_str(request_str);
        match req {
            Ok(req) => self.handle_method(&req.method, req.id, &req.params, tools, ctx).await,
            Err(_) => {
                // Maybe it's a notification (no id field)
                let notif: Result<JsonRpcNotification, _> = serde_json::from_str(request_str);
                if let Ok(n) = notif {
                    return self.handle_notification(&n.method, &n.params, tools);
                }
                serde_json::to_string(&JsonRpcResponse::err(0, -32600, "Invalid Request"))
                    .unwrap_or_default()
            }
        }
    }

    async fn handle_method(
        &self,
        method: &str,
        id: u64,
        params: &Option<serde_json::Value>,
        tools: &ToolRegistry,
        ctx: &ToolCtx,
    ) -> String {
        let result = match method {
            "initialize" => {
                let init_params: Option<InitializeParams> = params
                    .as_ref()
                    .and_then(|p| serde_json::from_value(p.clone()).ok());
                let _ = init_params;
                serde_json::to_value(InitializeResult {
                    protocol_version: MCP_VERSION.into(),
                    capabilities: ServerCapabilities {
                        tools: Some(serde_json::json!({})),
                        resources: None,
                        prompts: None,
                    },
                    server_info: ImplementationInfo {
                        name: self.name.clone(),
                        version: self.version.clone(),
                    },
                    instructions: None,
                })
                .ok()
            }
            "tools/list" => {
                let mcp_tools: Vec<MCPTool> = tools
                    .get_defs()
                    .into_iter()
                    .map(|td| MCPTool {
                        name: td.name,
                        description: td.description,
                        input_schema: td.parameters,
                    })
                    .collect();
                serde_json::to_value(ListToolsResult { tools: mcp_tools }).ok()
            }
            "tools/call" => {
                let call: CallToolParams = match params
                    .as_ref()
                    .and_then(|p| serde_json::from_value(p.clone()).ok())
                {
                    Some(c) => c,
                    None => {
                        return serde_json::to_string(&JsonRpcResponse::err(
                            id,
                            -32602,
                            "Invalid params",
                        ))
                        .unwrap_or_default();
                    }
                };

                match tools.execute(&call.name, ctx, call.arguments).await {
                    Ok(result) => serde_json::to_value(CallToolResult {
                        content: vec![ToolContent::Text { text: result }],
                        is_error: None,
                    })
                    .ok(),
                    Err(e) => serde_json::to_value(CallToolResult {
                        content: vec![ToolContent::Text { text: format!("Error: {e}") }],
                        is_error: Some(true),
                    })
                    .ok(),
                }
            }
            _ => None,
        };

        match result {
            Some(r) => {
                serde_json::to_string(&JsonRpcResponse::ok(id, r)).unwrap_or_default()
            }
            None => serde_json::to_string(&JsonRpcResponse::err(
                id,
                -32601,
                &format!("Method not found: {method}"),
            ))
            .unwrap_or_default(),
        }
    }

    fn handle_notification(
        &self,
        method: &str,
        _params: &Option<serde_json::Value>,
        _tools: &ToolRegistry,
    ) -> String {
        match method {
            "notifications/initialized" => String::new(),
            _ => String::new(),
        }
    }
}
