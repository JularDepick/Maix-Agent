//! LSP client — connects to language servers via stdio and performs LSP requests.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// LSP position (0-based line and column).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// LSP range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// LSP location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// LSP hover result.
#[derive(Debug, Clone, Deserialize)]
pub struct HoverResult {
    pub contents: HoverContents,
    #[serde(default)]
    pub range: Option<Range>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    String(String),
    MarkedString(MarkedString),
    Array(Vec<MarkedString>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    String(String),
    LanguageString { language: String, value: String },
}

impl std::fmt::Display for HoverContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HoverContents::String(s) => write!(f, "{}", s),
            HoverContents::MarkedString(ms) => match ms {
                MarkedString::String(s) => write!(f, "{}", s),
                MarkedString::LanguageString { language, value } => write!(f, "```{}\n{}\n```", language, value),
            },
            HoverContents::Array(arr) => {
                for (i, ms) in arr.iter().enumerate() {
                    if i > 0 { writeln!(f)?; }
                    match ms {
                        MarkedString::String(s) => write!(f, "{}", s)?,
                        MarkedString::LanguageString { language, value } => write!(f, "```{}\n{}\n```", language, value)?,
                    }
                }
                Ok(())
            }
        }
    }
}

/// LSP symbol information.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: u32,
    #[serde(default)]
    pub location: Option<Location>,
    #[serde(default)]
    #[serde(rename = "containerName")]
    pub container_name: Option<String>,
}

/// LSP document symbol (hierarchical).
#[derive(Debug, Clone, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u32,
    #[serde(default)]
    pub detail: Option<String>,
    pub range: Range,
    #[serde(default)]
    pub children: Option<Vec<DocumentSymbol>>,
}

impl DocumentSymbol {
    pub fn kind_name(kind: u32) -> &'static str {
        match kind {
            1 => "File", 2 => "Module", 3 => "Namespace", 4 => "Package",
            5 => "Class", 6 => "Method", 7 => "Property", 8 => "Field",
            9 => "Constructor", 10 => "Enum", 11 => "Interface", 12 => "Function",
            13 => "Variable", 14 => "Constant", 15 => "String", 16 => "Number",
            17 => "Boolean", 18 => "Array", 19 => "Object", 20 => "Key",
            21 => "Null", 22 => "EnumMember", 23 => "Struct", 24 => "Event",
            25 => "Operator", 26 => "TypeParameter",
            _ => "Unknown",
        }
    }

    pub fn display(&self, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        let kind = Self::kind_name(self.kind);
        let detail = self.detail.as_deref().unwrap_or("");
        let detail_str = if detail.is_empty() { String::new() } else { format!("({})", detail) };
        let mut result = format!("{}{}: {} {}", prefix, kind, self.name, detail_str);
        if let Some(children) = &self.children {
            for child in children {
                result.push('\n');
                result.push_str(&child.display(indent + 1));
            }
        }
        result
    }
}

/// JSON-RPC request.
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: i64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC notification.
#[derive(Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response.
#[derive(Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcErrorResponse>,
}

#[derive(Deserialize)]
struct JsonRpcErrorResponse {
    code: i64,
    message: String,
}

impl std::fmt::Display for JsonRpcErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LSP error {}: {}", self.code, self.message)
    }
}

/// LSP client connected to a language server via stdio.
pub struct LspClient {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    child: Child,
    request_id: AtomicI64,
    root_uri: String,
    initialized: bool,
}

impl LspClient {
    /// Spawn a language server and connect via stdio.
    pub async fn connect(command: &str, args: &[&str], root_uri: &str) -> Result<Self, String> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {}", command, e))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;

        let mut client = Self {
            stdin,
            stdout: BufReader::new(stdout),
            child,
            request_id: AtomicI64::new(1),
            root_uri: root_uri.to_string(),
            initialized: false,
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Perform the LSP initialize handshake.
    async fn initialize(&mut self) -> Result<(), String> {
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": self.root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                    "synchronization": { "didSave": true }
                },
                "workspace": {
                    "symbol": { "dynamicRegistration": false }
                }
            },
            "workspaceFolders": [{
                "uri": self.root_uri,
                "name": "workspace"
            }]
        });

        let result = self.request("initialize", Some(params)).await?;
        let _caps = result.get("capabilities");
        self.notify("initialized", None).await?;
        self.initialized = true;
        Ok(())
    }

    /// Send a request and wait for the response.
    pub async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let body = serde_json::to_string(&req).map_err(|e| format!("serialize: {e}"))?;
        self.send_message(&body).await?;

        // Read response, skipping notifications
        loop {
            let msg = self.read_message().await?;
            let resp: JsonRpcResponse =
                serde_json::from_str(&msg).map_err(|e| format!("parse response: {e}"))?;

            if resp.id == Some(id) {
                if let Some(err) = resp.error {
                    return Err(format!("{}", err));
                }
                return resp.result.ok_or_else(|| "no result in response".to_string());
            }
        }
    }

    /// Send a notification (no response expected).
    pub async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), String> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };
        let body = serde_json::to_string(&notif).map_err(|e| format!("serialize: {e}"))?;
        self.send_message(&body).await
    }

    /// Send a message with Content-Length header.
    async fn send_message(&mut self, body: &str) -> Result<(), String> {
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).await
            .map_err(|e| format!("write header: {e}"))?;
        self.stdin.write_all(body.as_bytes()).await
            .map_err(|e| format!("write body: {e}"))?;
        self.stdin.flush().await
            .map_err(|e| format!("flush: {e}"))?;
        Ok(())
    }

    /// Read a message with Content-Length header.
    async fn read_message(&mut self) -> Result<String, String> {
        let mut content_length = 0usize;

        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).await
                .map_err(|e| format!("read header: {e}"))?;

            if line.trim().is_empty() {
                break;
            }

            if let Some(val) = line.strip_prefix("Content-Length:") {
                content_length = val.trim().parse()
                    .map_err(|e| format!("parse content-length: {e}"))?;
            }
        }

        if content_length == 0 {
            return Err("missing Content-Length header".into());
        }

        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await
            .map_err(|e| format!("read body: {e}"))?;

        String::from_utf8(body).map_err(|e| format!("invalid utf8: {e}"))
    }

    /// Notify the server that a file was opened.
    pub async fn did_open(&mut self, path: &Path, language_id: &str, text: &str) -> Result<(), String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": text
            }
        });
        self.notify("textDocument/didOpen", Some(params)).await
    }

    /// Notify the server that a file was saved.
    pub async fn did_save(&mut self, path: &Path) -> Result<(), String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri }
        });
        self.notify("textDocument/didSave", Some(params)).await
    }

    /// Go to definition.
    pub async fn goto_definition(&mut self, path: &Path, line: u32, character: u32) -> Result<Vec<Location>, String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        let result = self.request("textDocument/definition", Some(params)).await?;
        if result.is_null() {
            return Ok(vec![]);
        }
        if result.is_array() {
            return serde_json::from_value(result).map_err(|e| format!("parse locations: {e}"));
        }
        let loc: Location = serde_json::from_value(result).map_err(|e| format!("parse location: {e}"))?;
        Ok(vec![loc])
    }

    /// Find references.
    pub async fn find_references(&mut self, path: &Path, line: u32, character: u32, include_declaration: bool) -> Result<Vec<Location>, String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": include_declaration }
        });
        let result = self.request("textDocument/references", Some(params)).await?;
        if result.is_null() {
            return Ok(vec![]);
        }
        serde_json::from_value(result).map_err(|e| format!("parse references: {e}"))
    }

    /// Hover.
    pub async fn hover(&mut self, path: &Path, line: u32, character: u32) -> Result<Option<HoverResult>, String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        let result = self.request("textDocument/hover", Some(params)).await?;
        if result.is_null() {
            return Ok(None);
        }
        serde_json::from_value(result).map_err(|e| format!("parse hover: {e}"))
    }

    /// Document symbols.
    pub async fn document_symbols(&mut self, path: &Path) -> Result<Vec<DocumentSymbol>, String> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri }
        });
        let result = self.request("textDocument/documentSymbol", Some(params)).await?;
        if result.is_null() {
            return Ok(vec![]);
        }
        serde_json::from_value(result).map_err(|e| format!("parse symbols: {e}"))
    }

    /// Workspace symbols.
    pub async fn workspace_symbols(&mut self, query: &str) -> Result<Vec<SymbolInformation>, String> {
        let params = serde_json::json!({ "query": query });
        let result = self.request("workspace/symbol", Some(params)).await?;
        if result.is_null() {
            return Ok(vec![]);
        }
        serde_json::from_value(result).map_err(|e| format!("parse symbols: {e}"))
    }

    /// Shutdown the language server.
    pub async fn shutdown(&mut self) -> Result<(), String> {
        let _ = self.request("shutdown", None).await;
        let _ = self.notify("exit", None).await;
        Ok(())
    }

    /// Get the language ID from file extension.
    pub fn language_id_from_path(path: &Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => "rust",
            Some("ts") | Some("tsx") => "typescript",
            Some("js") | Some("jsx") => "javascript",
            Some("py") => "python",
            Some("go") => "go",
            Some("json") => "json",
            Some("toml") => "toml",
            Some("md") => "markdown",
            _ => "plaintext",
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Convert a file path to a file:// URI.
pub fn path_to_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let path_str = absolute.to_string_lossy().replace('\\', "/");
    if path_str.starts_with("//") {
        format!("file:{}", path_str)
    } else {
        format!("file:///{}", path_str)
    }
}

/// Convert a file:// URI to a path.
pub fn uri_to_path(uri: &str) -> Option<&str> {
    uri.strip_prefix("file:///").or_else(|| uri.strip_prefix("file://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_uri() {
        let path = Path::new("/home/user/project/src/main.rs");
        let uri = path_to_uri(path);
        assert!(uri.starts_with("file:///"));
        assert!(uri.ends_with("main.rs"));
    }

    #[test]
    fn test_uri_to_path() {
        assert_eq!(uri_to_path("file:///home/user/main.rs"), Some("home/user/main.rs"));
        assert_eq!(uri_to_path("file://server/share"), Some("server/share"));
        assert_eq!(uri_to_path("https://example.com"), None);
    }

    #[test]
    fn test_language_id() {
        assert_eq!(LspClient::language_id_from_path(Path::new("main.rs")), "rust");
        assert_eq!(LspClient::language_id_from_path(Path::new("app.ts")), "typescript");
        assert_eq!(LspClient::language_id_from_path(Path::new("main.py")), "python");
        assert_eq!(LspClient::language_id_from_path(Path::new("main.go")), "go");
    }

    #[test]
    fn test_symbol_display() {
        let sym = DocumentSymbol {
            name: "main".to_string(),
            kind: 12,
            detail: Some("fn()".to_string()),
            range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 10, character: 0 } },
            children: None,
        };
        let display = sym.display(0);
        assert!(display.contains("Function"));
        assert!(display.contains("main"));
    }
}
