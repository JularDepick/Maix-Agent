//! Code indexer — regex-based symbol extraction and codebase navigation.
//!
//! Provides lightweight symbol indexing without external parser dependencies.
//! Uses regex patterns to extract function, struct, class, and other definitions.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// Type of code symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Interface,
    TypeAlias,
    Constant,
    Static,
    Module,
    Variable,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "function"),
            Self::Method => write!(f, "method"),
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Trait => write!(f, "trait"),
            Self::Impl => write!(f, "impl"),
            Self::Class => write!(f, "class"),
            Self::Interface => write!(f, "interface"),
            Self::TypeAlias => write!(f, "type"),
            Self::Constant => write!(f, "const"),
            Self::Static => write!(f, "static"),
            Self::Module => write!(f, "module"),
            Self::Variable => write!(f, "variable"),
        }
    }
}

/// A symbol extracted from source code.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub line: usize,
    pub signature: String,
    pub visibility: Option<String>,
}

/// Information about a source file.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub language: String,
    pub symbol_count: usize,
    pub line_count: usize,
}

/// Project summary statistics.
#[derive(Debug)]
pub struct ProjectSummary {
    pub total_files: usize,
    pub total_symbols: usize,
    pub by_language: HashMap<String, usize>,
    pub by_kind: HashMap<String, usize>,
}

impl ProjectSummary {
    pub fn format(&self) -> String {
        let mut lines = vec![
            format!("Project Summary:"),
            format!("  Files indexed: {}", self.total_files),
            format!("  Total symbols: {}", self.total_symbols),
            String::new(),
        ];

        lines.push("By language:".to_string());
        let mut langs: Vec<_> = self.by_language.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in langs {
            lines.push(format!("  {}: {} files", lang, count));
        }

        lines.push(String::new());
        lines.push("By symbol kind:".to_string());
        let mut kinds: Vec<_> = self.by_kind.iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(a.1));
        for (kind, count) in kinds {
            lines.push(format!("  {}: {}", kind, count));
        }

        lines.join("\n")
    }
}

/// Lightweight code indexer using regex-based symbol extraction.
pub struct CodeIndex {
    symbols: Vec<Symbol>,
    files: Vec<FileInfo>,
}

impl Default for CodeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeIndex {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Index a directory recursively.
    pub async fn index_directory(&mut self, root: &Path) -> MaixResult<()> {
        let skip_dirs = [".git", "node_modules", "target", ".venv", "__pycache__", "dist", "build"];
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut entry_stream = entries;
            while let Some(entry) = entry_stream.next_entry().await.unwrap_or(None) {
                let ft = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if ft.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }

                if !ft.is_file() {
                    continue;
                }

                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let language = match ext {
                    "rs" => "rust",
                    "ts" | "tsx" => "typescript",
                    "js" | "jsx" => "javascript",
                    "py" => "python",
                    "go" => "go",
                    _ => continue,
                };

                let content = match tokio::fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let symbols = extract_symbols(&content, language, &path);
                let line_count = content.lines().count();

                self.files.push(FileInfo {
                    path: path.clone(),
                    language: language.to_string(),
                    symbol_count: symbols.len(),
                    line_count,
                });
                self.symbols.extend(symbols);
            }
        }

        Ok(())
    }

    /// Find a symbol by exact name.
    pub fn find_symbol(&self, name: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    /// Search symbols by fuzzy substring match.
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<&Symbol> {
        let query_lower = query.to_lowercase();
        self.symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect()
    }

    /// Get all symbols in a specific file.
    pub fn file_symbols(&self, path: &str) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.file.to_string_lossy().contains(path))
            .collect()
    }

    /// Get a project summary.
    pub fn summary(&self) -> ProjectSummary {
        let mut by_language: HashMap<String, usize> = HashMap::new();
        let mut by_kind: HashMap<String, usize> = HashMap::new();

        for f in &self.files {
            *by_language.entry(f.language.clone()).or_insert(0) += 1;
        }
        for s in &self.symbols {
            *by_kind.entry(s.kind.to_string()).or_insert(0) += 1;
        }

        ProjectSummary {
            total_files: self.files.len(),
            total_symbols: self.symbols.len(),
            by_language,
            by_kind,
        }
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Extract symbols from source code using regex patterns.
fn extract_symbols(content: &str, language: &str, path: &Path) -> Vec<Symbol> {
    match language {
        "rust" => extract_rust_symbols(content, path),
        "typescript" | "javascript" => extract_js_symbols(content, path),
        "python" => extract_python_symbols(content, path),
        "go" => extract_go_symbols(content, path),
        _ => Vec::new(),
    }
}

fn extract_rust_symbols(content: &str, path: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Functions: pub fn name(...) or fn name(...)
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?(?:async\s+)?fn\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Structs
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?struct\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Struct,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Enums
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?enum\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Enum,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Traits
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?(?:async\s+)?trait\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Trait,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Type aliases
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?type\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::TypeAlias,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Constants
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?const\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Constant,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Statics
        if let Some(caps) = regex_match(r"(?:(pub(?:\([^)]*\))?)\s+)?static\s+(\w+)", trimmed) {
            let vis = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(2).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Static,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
        }

        // impl blocks
        if let Some(caps) = regex_match(r"impl(?:<[^>]*>)?\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Impl,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: None,
            });
        }
    }

    symbols
}

fn extract_js_symbols(content: &str, path: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Functions: function name(...) or export function name(...)
        if let Some(caps) = regex_match(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: if trimmed.contains("export") { Some("export".into()) } else { None },
            });
            continue;
        }

        // Arrow functions: const name = (...) => or export const name = (...)
        if let Some(caps) = regex_match(r"(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: if trimmed.contains("export") { Some("export".into()) } else { None },
            });
            continue;
        }

        // Classes
        if let Some(caps) = regex_match(r"(?:export\s+)?(?:abstract\s+)?class\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: if trimmed.contains("export") { Some("export".into()) } else { None },
            });
            continue;
        }

        // Interfaces (TypeScript)
        if let Some(caps) = regex_match(r"(?:export\s+)?interface\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Interface,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: if trimmed.contains("export") { Some("export".into()) } else { None },
            });
            continue;
        }

        // Type aliases (TypeScript)
        if let Some(caps) = regex_match(r"(?:export\s+)?type\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::TypeAlias,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: if trimmed.contains("export") { Some("export".into()) } else { None },
            });
        }
    }

    symbols
}

fn extract_python_symbols(content: &str, path: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Functions: def name(...) or async def name(...)
        if let Some(caps) = regex_match(r"(?:async\s+)?def\s+(\w+)\s*\(", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let vis = if name.starts_with('_') { None } else { Some("public".into()) };
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Classes
        if let Some(caps) = regex_match(r"class\s+(\w+)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Class,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: Some("public".into()),
            });
        }
    }

    symbols
}

fn extract_go_symbols(content: &str, path: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Functions: func name(...) or func (receiver) name(...)
        if let Some(caps) = regex_match(r"func\s+(?:\([^)]+\)\s+)?(\w+)\s*\(", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let kind = if trimmed.contains("func (") {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            let vis = if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                Some("public".into())
            } else {
                None
            };
            symbols.push(Symbol {
                name,
                kind,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: vis,
            });
            continue;
        }

        // Types
        if let Some(caps) = regex_match(r"type\s+(\w+)\s+(struct|interface)", trimmed) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let kind_str = caps.get(2).unwrap().as_str();
            let kind = if kind_str == "struct" { SymbolKind::Struct } else { SymbolKind::Interface };
            symbols.push(Symbol {
                name,
                kind,
                file: path.to_path_buf(),
                line: line_no,
                signature: trimmed.to_string(),
                visibility: None,
            });
        }
    }

    symbols
}

fn regex_match<'a>(pattern: &str, text: &'a str) -> Option<regex::Captures<'a>> {
    regex::Regex::new(pattern).ok()?.captures(text)
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Build a code index for the project.
pub struct IndexBuildTool(pub Arc<Mutex<CodeIndex>>);

#[async_trait]
impl Tool for IndexBuildTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "index_build".into(),
            description: "Build a code index for the project directory. Extracts symbols (functions, structs, classes, etc.) from source files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory to index (default: working directory)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let root = ctx.working_dir.join(path_str);

        let mut index = self.0.lock().await;
        index.index_directory(&root).await?;

        let summary = index.summary();
        Ok(summary.format())
    }
}

/// Search for symbols in the code index.
pub struct SymbolSearchTool(pub Arc<Mutex<CodeIndex>>);

#[async_trait]
impl Tool for SymbolSearchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "symbol_search".into(),
            description: "Search for symbols (functions, classes, structs, etc.) in the code index by name.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Symbol name or substring to search for" },
                    "limit": { "type": "integer", "description": "Max results (default: 20)" }
                },
                "required": ["query"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'query'".into()))?;
        let limit = args["limit"].as_u64().unwrap_or(20) as usize;

        let index = self.0.lock().await;
        let results = index.search_symbols(query, limit);

        if results.is_empty() {
            return Ok(format!("No symbols found matching '{}'", query));
        }

        let mut lines = vec![format!("Found {} symbols matching '{}':", results.len(), query)];
        for sym in &results {
            let vis = sym.visibility.as_deref().unwrap_or("");
            let vis_str = if vis.is_empty() { String::new() } else { format!("{} ", vis) };
            lines.push(format!(
                "  {} {}{} @ {}:{}",
                sym.kind, vis_str, sym.name,
                sym.file.display(), sym.line + 1
            ));
        }

        Ok(lines.join("\n"))
    }
}

/// List all symbols in a specific file.
pub struct FileSymbolsTool(pub Arc<Mutex<CodeIndex>>);

#[async_trait]
impl Tool for FileSymbolsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "file_symbols".into(),
            description: "List all symbols (functions, classes, structs, etc.) in a specific file.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path (or substring to match)" }
                },
                "required": ["file"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let file = args["file"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'file'".into()))?;

        let index = self.0.lock().await;
        let results = index.file_symbols(file);

        if results.is_empty() {
            return Ok(format!("No symbols found in '{}'", file));
        }

        let mut lines = vec![format!("Symbols in {}:", file)];
        for sym in &results {
            let vis = sym.visibility.as_deref().unwrap_or("");
            let vis_str = if vis.is_empty() { String::new() } else { format!("{} ", vis) };
            lines.push(format!(
                "  L{}: {} {}{} - {}",
                sym.line + 1, sym.kind, vis_str, sym.name, sym.signature
            ));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let content = r#"
pub fn main() {
    println!("hello");
}

struct Config {
    name: String,
}

pub trait Handler: Send + Sync {
    fn handle(&self);
}

enum Color {
    Red,
    Green,
}

const MAX_SIZE: usize = 1024;
"#;
        let symbols = extract_rust_symbols(content, Path::new("test.rs"));
        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == SymbolKind::Struct));
        assert!(symbols.iter().any(|s| s.name == "Handler" && s.kind == SymbolKind::Trait));
        assert!(symbols.iter().any(|s| s.name == "Color" && s.kind == SymbolKind::Enum));
        assert!(symbols.iter().any(|s| s.name == "MAX_SIZE" && s.kind == SymbolKind::Constant));
    }

    #[test]
    fn test_extract_js_symbols() {
        let content = r#"
export function hello() {
    console.log("hi");
}

const add = (a, b) => a + b;

class Calculator {
    compute() {}
}

interface Config {
    name: string;
}
"#;
        let symbols = extract_js_symbols(content, Path::new("test.ts"));
        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "add" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "Calculator" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == SymbolKind::Interface));
    }

    #[test]
    fn test_extract_python_symbols() {
        let content = r#"
def hello():
    print("hi")

async def fetch_data():
    pass

class MyClass:
    pass
"#;
        let symbols = extract_python_symbols(content, Path::new("test.py"));
        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "fetch_data" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class));
    }

    #[test]
    fn test_extract_go_symbols() {
        let content = r#"
func main() {
    fmt.Println("hello")
}

func (s *Server) Start() error {
    return nil
}

type Config struct {
    Name string
}
"#;
        let symbols = extract_go_symbols(content, Path::new("test.go"));
        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "Start" && s.kind == SymbolKind::Method));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == SymbolKind::Struct));
    }

    #[test]
    fn test_search_symbols() {
        let mut index = CodeIndex::new();
        index.symbols.push(Symbol {
            name: "handle_request".into(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/main.rs"),
            line: 10,
            signature: "fn handle_request(req: &Request)".into(),
            visibility: Some("pub".into()),
        });
        index.symbols.push(Symbol {
            name: "Handler".into(),
            kind: SymbolKind::Trait,
            file: PathBuf::from("src/lib.rs"),
            line: 5,
            signature: "trait Handler".into(),
            visibility: Some("pub".into()),
        });

        let results = index.search_symbols("handle", 10);
        assert_eq!(results.len(), 2);

        let results = index.find_symbol("Handler");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, SymbolKind::Trait);
    }
}
