//! LSP tool implementations — registered as Maix tools.

use super::client::{LspClient, uri_to_path};
use super::manager::LspManager;
use async_trait::async_trait;
use maix_core::MaixResult;
use once_cell;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Shared LSP manager that tools can use.
static LSP_MANAGER: once_cell::sync::Lazy<Arc<Mutex<LspManager>>> =
    once_cell::sync::Lazy::new(|| {
        let root = std::env::current_dir().unwrap_or_default();
        Arc::new(Mutex::new(LspManager::new(root)))
    });

// ---------------------------------------------------------------------------
// LspGotoDefinitionTool
// ---------------------------------------------------------------------------

pub struct LspGotoDefinitionTool;

#[async_trait]
impl Tool for LspGotoDefinitionTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "lsp_goto_definition".into(),
            description: "Go to the definition of a symbol at a given position in a file. Uses LSP protocol.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "line": { "type": "integer", "description": "Line number (0-based)" },
                    "character": { "type": "integer", "description": "Character offset (0-based)" }
                },
                "required": ["file", "line", "character"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let file = args["file"].as_str().unwrap_or_default();
        let line = args["line"].as_u64().unwrap_or(0) as u32;
        let character = args["character"].as_u64().unwrap_or(0) as u32;

        let path = Path::new(file);
        let mut mgr = LSP_MANAGER.lock().await;
        let client = mgr.get_or_start(path).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        let lang_id = LspClient::language_id_from_path(path);
        let _ = client.did_open(path, lang_id, "").await;

        let locations = client.goto_definition(path, line, character).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        if locations.is_empty() {
            return Ok("No definition found.".to_string());
        }

        let mut result = String::from("Definitions:\n");
        for loc in &locations {
            let file_path = uri_to_path(&loc.uri).unwrap_or(&loc.uri);
            result.push_str(&format!(
                "  {}:{}:{}\n",
                file_path,
                loc.range.start.line + 1,
                loc.range.start.character + 1
            ));
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// LspFindReferencesTool
// ---------------------------------------------------------------------------

pub struct LspFindReferencesTool;

#[async_trait]
impl Tool for LspFindReferencesTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "lsp_find_references".into(),
            description: "Find all references to a symbol at a given position. Uses LSP protocol.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "line": { "type": "integer", "description": "Line number (0-based)" },
                    "character": { "type": "integer", "description": "Character offset (0-based)" },
                    "include_declaration": { "type": "boolean", "description": "Include the declaration in results (default: true)" }
                },
                "required": ["file", "line", "character"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let file = args["file"].as_str().unwrap_or_default();
        let line = args["line"].as_u64().unwrap_or(0) as u32;
        let character = args["character"].as_u64().unwrap_or(0) as u32;
        let include_declaration = args["include_declaration"].as_bool().unwrap_or(true);

        let path = Path::new(file);
        let mut mgr = LSP_MANAGER.lock().await;
        let client = mgr.get_or_start(path).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        let lang_id = LspClient::language_id_from_path(path);
        let _ = client.did_open(path, lang_id, "").await;

        let locations = client.find_references(path, line, character, include_declaration).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        if locations.is_empty() {
            return Ok("No references found.".to_string());
        }

        let mut result = format!("References ({}):\n", locations.len());
        for loc in &locations {
            let file_path = uri_to_path(&loc.uri).unwrap_or(&loc.uri);
            result.push_str(&format!(
                "  {}:{}:{}\n",
                file_path,
                loc.range.start.line + 1,
                loc.range.start.character + 1
            ));
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// LspHoverTool
// ---------------------------------------------------------------------------

pub struct LspHoverTool;

#[async_trait]
impl Tool for LspHoverTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "lsp_hover".into(),
            description: "Get hover information (documentation, type info) at a position. Uses LSP protocol.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "line": { "type": "integer", "description": "Line number (0-based)" },
                    "character": { "type": "integer", "description": "Character offset (0-based)" }
                },
                "required": ["file", "line", "character"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let file = args["file"].as_str().unwrap_or_default();
        let line = args["line"].as_u64().unwrap_or(0) as u32;
        let character = args["character"].as_u64().unwrap_or(0) as u32;

        let path = Path::new(file);
        let mut mgr = LSP_MANAGER.lock().await;
        let client = mgr.get_or_start(path).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        let lang_id = LspClient::language_id_from_path(path);
        let _ = client.did_open(path, lang_id, "").await;

        let hover = client.hover(path, line, character).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        match hover {
            Some(h) => Ok(format!("Hover:\n{}", h.contents)),
            None => Ok("No hover information available.".to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// LspDocumentSymbolsTool
// ---------------------------------------------------------------------------

pub struct LspDocumentSymbolsTool;

#[async_trait]
impl Tool for LspDocumentSymbolsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "lsp_document_symbols".into(),
            description: "List all symbols (functions, classes, variables, etc.) in a file. Uses LSP protocol.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" }
                },
                "required": ["file"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let file = args["file"].as_str().unwrap_or_default();
        let path = Path::new(file);
        let mut mgr = LSP_MANAGER.lock().await;
        let client = mgr.get_or_start(path).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        let lang_id = LspClient::language_id_from_path(path);
        let _ = client.did_open(path, lang_id, "").await;

        let symbols = client.document_symbols(path).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        if symbols.is_empty() {
            return Ok("No symbols found.".to_string());
        }

        let mut result = format!("Document Symbols ({}):\n", symbols.len());
        for sym in &symbols {
            result.push_str(&sym.display(0));
            result.push('\n');
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// LspWorkspaceSymbolsTool
// ---------------------------------------------------------------------------

pub struct LspWorkspaceSymbolsTool;

#[async_trait]
impl Tool for LspWorkspaceSymbolsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "lsp_workspace_symbols".into(),
            description: "Search for symbols across the workspace by name. Uses LSP protocol.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Symbol name query to search for" }
                },
                "required": ["query"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let query = args["query"].as_str().unwrap_or_default();
        let root = std::env::current_dir().unwrap_or_default();
        let any_rs = root.join("src").join("lib.rs");
        let mut mgr = LSP_MANAGER.lock().await;
        let client = mgr.get_or_start(&any_rs).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        let symbols = client.workspace_symbols(query).await
            .map_err(|e| maix_core::MaixError::Tool(e))?;

        if symbols.is_empty() {
            return Ok(format!("No symbols matching '{}' found.", query));
        }

        let mut result = format!("Workspace Symbols ({}):\n", symbols.len());
        for sym in symbols.iter().take(50) {
            let kind = super::client::DocumentSymbol::kind_name(sym.kind);
            let loc = sym.location.as_ref().map(|l| {
                let file_path = uri_to_path(&l.uri).unwrap_or(&l.uri);
                format!("{}:{}:{}", file_path, l.range.start.line + 1, l.range.start.character + 1)
            }).unwrap_or_default();
            let container = sym.container_name.as_deref().unwrap_or("");
            let container_str = if container.is_empty() { String::new() } else { format!(" [{}]", container) };
            result.push_str(&format!("  {} {}: {} {}\n", kind, sym.name, loc, container_str));
        }
        Ok(result)
    }
}
