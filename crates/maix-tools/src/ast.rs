//! AST-aware editing — syntax-level code modifications and refactoring.
//!
//! Uses regex-based symbol detection for rename, extract, and reference finding.
//! Supports Rust, TypeScript/JavaScript, Python, and Go.

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    Go,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "ts" | "tsx" | "js" | "jsx" => Some(Self::TypeScript),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }

    fn identifier_chars(&self) -> &str {
        "a-zA-Z0-9_"
    }
}

/// A location in source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// A range in source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// A reference to a symbol.
#[derive(Debug, Clone)]
pub struct SymbolReference {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
    pub is_definition: bool,
}

/// A file change produced by a refactoring operation.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub file: PathBuf,
    pub old_text: String,
    pub new_text: String,
    pub line: usize,
}

/// Type of AST edit operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstEditType {
    Rename,
    ExtractFunction,
    InlineVariable,
    AddImport,
    RemoveImport,
}

/// AST editor for syntax-aware code modifications.
pub struct AstEditor {
    language: Language,
}

impl AstEditor {
    pub fn new(language: Language) -> Self {
        Self { language }
    }

    /// Detect language from file path.
    pub fn for_file(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        Language::from_extension(ext).map(Self::new)
    }

    /// Find all references to a symbol in a file.
    pub fn find_references(&self, file: &Path, symbol: &str) -> std::io::Result<Vec<SymbolReference>> {
        let content = std::fs::read_to_string(file)?;
        Ok(self.find_references_in_source(&content, file, symbol))
    }

    /// Find all references to a symbol in source text.
    pub fn find_references_in_source(
        &self,
        source: &str,
        file: &Path,
        symbol: &str,
    ) -> Vec<SymbolReference> {
        let mut refs = Vec::new();
        let pattern = self.symbol_pattern(symbol);

        for (line_num, line) in source.lines().enumerate() {
            for mat in pattern.find_iter(line) {
                let is_def = self.is_definition_context(line, mat.start(), symbol);
                refs.push(SymbolReference {
                    file: file.to_path_buf(),
                    line: line_num + 1,
                    column: mat.start() + 1,
                    text: mat.as_str().to_string(),
                    is_definition: is_def,
                });
            }
        }

        refs
    }

    /// Rename a symbol in a file, returning the number of changes.
    pub fn rename_in_file(&self, file: &Path, old_name: &str, new_name: &str) -> std::io::Result<Vec<FileChange>> {
        let content = std::fs::read_to_string(file)?;
        let changes = self.rename_in_source(&content, file, old_name, new_name);

        if !changes.is_empty() {
            let pattern = self.symbol_pattern(old_name);
            let new_content = pattern.replace_all(&content, new_name);
            std::fs::write(file, new_content.as_bytes())?;
        }

        Ok(changes)
    }

    /// Compute rename changes in source text (without writing).
    pub fn rename_in_source(
        &self,
        source: &str,
        file: &Path,
        old_name: &str,
        new_name: &str,
    ) -> Vec<FileChange> {
        let pattern = self.symbol_pattern(old_name);
        let mut changes = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            for _mat in pattern.find_iter(line) {
                changes.push(FileChange {
                    file: file.to_path_buf(),
                    old_text: old_name.to_string(),
                    new_text: new_name.to_string(),
                    line: line_num + 1,
                });
            }
        }

        changes
    }

    /// Extract a range of lines into a new function.
    pub fn extract_function(
        &self,
        source: &str,
        start_line: usize,
        end_line: usize,
        fn_name: &str,
    ) -> Result<String, String> {
        let lines: Vec<&str> = source.lines().collect();
        if start_line == 0 || end_line > lines.len() || start_line > end_line {
            return Err(format!(
                "invalid range: {}-{} (file has {} lines)",
                start_line,
                end_line,
                lines.len()
            ));
        }

        let extracted: Vec<&str> = lines[(start_line - 1)..end_line].to_vec();
        let extracted_code = extracted.join("\n");

        // Detect variables used in extracted code that are defined before it
        let captures = self.detect_captures(&lines, start_line, end_line, &extracted_code);

        // Build function signature
        let params = if captures.is_empty() {
            String::new()
        } else {
            captures.join(": &str, ") + ": &str"
        };

        let new_fn = match self.language {
            Language::Rust => format!("fn {}({}) {{\n{}\n}}", fn_name, params, indent(&extracted_code, "    ")),
            Language::TypeScript => format!("function {}({}) {{\n{}\n}}", fn_name, params, indent(&extracted_code, "    ")),
            Language::Python => format!("def {}({}):\n{}", fn_name, captures.join(", "), indent(&extracted_code, "    ")),
            Language::Go => format!("func {}({}) {{\n{}\n}}", fn_name, params, indent(&extracted_code, "\t")),
        };

        // Build call
        let call = match self.language {
            Language::Python => format!("{}({})", fn_name, captures.join(", ")),
            _ => format!("{}({});", fn_name, captures.join(", ")),
        };

        // Reconstruct file
        let mut result = String::new();
        // Lines before extracted range
        for line in &lines[..start_line - 1] {
            result.push_str(line);
            result.push('\n');
        }
        // Function call
        result.push_str(&call);
        result.push('\n');
        // Lines after extracted range
        for line in &lines[end_line..] {
            result.push_str(line);
            result.push('\n');
        }
        // New function definition at end
        result.push('\n');
        result.push_str(&new_fn);
        result.push('\n');

        Ok(result)
    }

    /// Find function/method definitions in a file.
    pub fn find_definitions(&self, file: &Path) -> std::io::Result<Vec<SymbolReference>> {
        let content = std::fs::read_to_string(file)?;
        Ok(self.find_definitions_in_source(&content, file))
    }

    /// Find definitions in source text.
    pub fn find_definitions_in_source(&self, source: &str, file: &Path) -> Vec<SymbolReference> {
        let mut defs = Vec::new();
        let patterns = self.definition_patterns();

        for (line_num, line) in source.lines().enumerate() {
            for pattern in &patterns {
                if let Some(mat) = pattern.find(line) {
                    // Extract the symbol name (last identifier in the match)
                    let matched = mat.as_str();
                    if let Some(name) = extract_last_identifier(matched) {
                        defs.push(SymbolReference {
                            file: file.to_path_buf(),
                            line: line_num + 1,
                            column: mat.start() + 1,
                            text: name.to_string(),
                            is_definition: true,
                        });
                    }
                }
            }
        }

        defs
    }

    // -- Internal helpers --

    fn symbol_pattern(&self, symbol: &str) -> regex::Regex {
        let escaped = regex::escape(symbol);
        regex::Regex::new(&format!(r"\b{}\b", escaped))
            .unwrap_or_else(|_| regex::Regex::new(&escaped).unwrap())
    }

    fn is_definition_context(&self, line: &str, pos: usize, _symbol: &str) -> bool {
        let before = &line[..pos];
        match self.language {
            Language::Rust => {
                before.contains("fn ") || before.contains("let ")
                    || before.contains("struct ") || before.contains("enum ")
                    || before.contains("trait ") || before.contains("impl ")
                    || before.contains("const ") || before.contains("static ")
                    || before.contains("type ")
            }
            Language::TypeScript => {
                before.contains("function ") || before.contains("const ")
                    || before.contains("let ") || before.contains("var ")
                    || before.contains("class ") || before.contains("interface ")
                    || before.contains("type ")
            }
            Language::Python => before.contains("def ") || before.contains("class "),
            Language::Go => {
                before.contains("func ") || before.contains("var ")
                    || before.contains("const ") || before.contains("type ")
            }
        }
    }

    fn definition_patterns(&self) -> Vec<regex::Regex> {
        match self.language {
            Language::Rust => vec![
                regex::Regex::new(r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:pub\s+)?struct\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:pub\s+)?enum\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:pub\s+)?trait\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:pub\s+)?type\s+(\w+)").unwrap(),
            ],
            Language::TypeScript => vec![
                regex::Regex::new(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=").unwrap(),
                regex::Regex::new(r"(?:export\s+)?class\s+(\w+)").unwrap(),
                regex::Regex::new(r"(?:export\s+)?interface\s+(\w+)").unwrap(),
            ],
            Language::Python => vec![
                regex::Regex::new(r"def\s+(\w+)").unwrap(),
                regex::Regex::new(r"class\s+(\w+)").unwrap(),
            ],
            Language::Go => vec![
                regex::Regex::new(r"func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)").unwrap(),
                regex::Regex::new(r"type\s+(\w+)").unwrap(),
                regex::Regex::new(r"var\s+(\w+)").unwrap(),
            ],
        }
    }

    fn detect_captures(&self, lines: &[&str], start: usize, end: usize, _extracted: &str) -> Vec<String> {
        // Simple heuristic: find identifiers used in extracted range that are defined before it
        let id_chars = self.language.identifier_chars();
        let id_re = regex::Regex::new(&format!(r#"[{}]+"#, id_chars)).unwrap();
        let keywords = self.keywords();

        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in &lines[(start - 1)..end] {
            for mat in id_re.find_iter(line) {
                let word = mat.as_str();
                if !keywords.contains(&word) && word.len() > 1 {
                    used.insert(word.to_string());
                }
            }
        }

        // Check which are defined before the extracted range
        let mut captures: Vec<String> = Vec::new();
        for word in &used {
            for line in &lines[..start - 1] {
                if line.contains(word) && self.is_definition_line(line, word) {
                    captures.push(word.clone());
                    break;
                }
            }
        }

        captures.sort();
        captures
    }

    fn is_definition_line(&self, line: &str, word: &str) -> bool {
        match self.language {
            Language::Rust => line.contains(&format!("let {}", word)) || line.contains(&format!("let mut {}", word)),
            Language::TypeScript => line.contains(&format!("const {}", word)) || line.contains(&format!("let {}", word)),
            Language::Python => line.contains(&format!("{} =", word)),
            Language::Go => line.contains(&format!("{} :=", word)) || line.contains(&format!("var {}", word)),
        }
    }

    fn keywords(&self) -> Vec<&str> {
        match self.language {
            Language::Rust => vec![
                "fn", "let", "mut", "pub", "struct", "enum", "trait", "impl",
                "use", "mod", "const", "static", "if", "else", "match", "for",
                "while", "loop", "return", "self", "Self", "true", "false",
                "async", "await", "move", "ref", "where", "as", "in", "break",
            ],
            Language::TypeScript => vec![
                "function", "const", "let", "var", "class", "interface", "type",
                "if", "else", "for", "while", "return", "import", "export",
                "from", "async", "await", "new", "this", "true", "false",
                "null", "undefined", "void", "string", "number", "boolean",
            ],
            Language::Python => vec![
                "def", "class", "if", "else", "elif", "for", "while", "return",
                "import", "from", "as", "with", "try", "except", "finally",
                "raise", "pass", "break", "continue", "and", "or", "not",
                "True", "False", "None", "self", "lambda", "yield", "async", "await",
            ],
            Language::Go => vec![
                "func", "var", "const", "type", "struct", "interface", "if",
                "else", "for", "range", "return", "import", "package", "go",
                "defer", "select", "case", "switch", "default", "break",
                "continue", "fallthrough", "true", "false", "nil", "make", "new",
            ],
        }
    }
}

fn indent(code: &str, prefix: &str) -> String {
    code.lines()
        .map(|line| format!("{}{}", prefix, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_last_identifier(text: &str) -> Option<&str> {
    // Find the last word that looks like an identifier
    text.split_whitespace()
        .rev()
        .find(|s| s.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false))
        .map(|s| s.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_'))
}

// ---------------------------------------------------------------------------
// Tool wrappers
// ---------------------------------------------------------------------------

/// Tool: rename a symbol in a file.
pub struct AstRenameTool;

#[async_trait]
impl Tool for AstRenameTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ast_rename".into(),
            description: "Rename a symbol across a file using syntax-aware analysis. Finds all occurrences respecting word boundaries.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "old_name": { "type": "string", "description": "Current symbol name" },
                    "new_name": { "type": "string", "description": "New symbol name" }
                },
                "required": ["file", "old_name", "new_name"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> crate::MaixResult<String> {
        let file = PathBuf::from(args["file"].as_str().unwrap_or_default());
        let old_name = args["old_name"].as_str().unwrap_or_default();
        let new_name = args["new_name"].as_str().unwrap_or_default();

        let editor = AstEditor::for_file(&file)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("unsupported file type: {}", file.display())))?;

        let changes = editor.rename_in_file(&file, old_name, new_name)?;
        Ok(format!(
            "Renamed {} occurrence(s) of '{}' to '{}' in {}",
            changes.len(),
            old_name,
            new_name,
            file.display()
        ))
    }
}

/// Tool: find all references to a symbol in a file.
pub struct AstFindRefsTool;

#[async_trait]
impl Tool for AstFindRefsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ast_find_refs".into(),
            description: "Find all references to a symbol in a file, distinguishing definitions from usages.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "symbol": { "type": "string", "description": "Symbol name to search for" }
                },
                "required": ["file", "symbol"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> crate::MaixResult<String> {
        let file = PathBuf::from(args["file"].as_str().unwrap_or_default());
        let symbol = args["symbol"].as_str().unwrap_or_default();

        let editor = AstEditor::for_file(&file)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("unsupported file type: {}", file.display())))?;

        let refs = editor.find_references(&file, symbol)?;
        if refs.is_empty() {
            return Ok(format!("No references to '{}' found in {}", symbol, file.display()));
        }

        let mut lines = vec![format!("References to '{}' in {}:", symbol, file.display())];
        for r in &refs {
            let kind = if r.is_definition { "def" } else { "ref" };
            lines.push(format!("  {}:{} [{}]", r.line, r.column, kind));
        }
        Ok(lines.join("\n"))
    }
}

/// Tool: extract a range of lines into a new function.
pub struct AstExtractTool;

#[async_trait]
impl Tool for AstExtractTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ast_extract".into(),
            description: "Extract a range of lines into a new function. Detects captured variables automatically.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "start_line": { "type": "integer", "description": "Start line (1-based)" },
                    "end_line": { "type": "integer", "description": "End line (1-based inclusive)" },
                    "fn_name": { "type": "string", "description": "Name for the new function" }
                },
                "required": ["file", "start_line", "end_line", "fn_name"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> crate::MaixResult<String> {
        let file = PathBuf::from(args["file"].as_str().unwrap_or_default());
        let start_line = args["start_line"].as_u64().unwrap_or(1) as usize;
        let end_line = args["end_line"].as_u64().unwrap_or(1) as usize;
        let fn_name = args["fn_name"].as_str().unwrap_or_default();

        let editor = AstEditor::for_file(&file)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("unsupported file type: {}", file.display())))?;

        let source = std::fs::read_to_string(&file)?;
        let result = editor.extract_function(&source, start_line, end_line, fn_name)
            .map_err(maix_core::MaixError::Tool)?;
        std::fs::write(&file, &result)?;

        Ok(format!(
            "Extracted lines {}-{} into function '{}' in {}",
            start_line,
            end_line,
            fn_name,
            file.display()
        ))
    }
}

/// Tool: find all function/struct/enum definitions in a file.
pub struct AstDefinitionsTool;

#[async_trait]
impl Tool for AstDefinitionsTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "ast_definitions".into(),
            description: "Find all function, struct, class, and type definitions in a file.".into(),
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

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> crate::MaixResult<String> {
        let file = PathBuf::from(args["file"].as_str().unwrap_or_default());

        let editor = AstEditor::for_file(&file)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("unsupported file type: {}", file.display())))?;

        let defs = editor.find_definitions(&file)?;
        if defs.is_empty() {
            return Ok(format!("No definitions found in {}", file.display()));
        }

        let mut lines = vec![format!("Definitions in {}:", file.display())];
        for d in &defs {
            lines.push(format!("  {} (line {})", d.text, d.line));
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("java"), None);
    }

    #[test]
    fn test_find_references_rust() {
        let editor = AstEditor::new(Language::Rust);
        let source = r#"fn main() {
    let x = 10;
    let y = x + 5;
    println!("{}", x);
}"#;
        let refs = editor.find_references_in_source(source, Path::new("test.rs"), "x");
        assert_eq!(refs.len(), 3); // definition + 2 uses
        assert!(refs.iter().any(|r| r.is_definition));
    }

    #[test]
    fn test_find_references_no_match() {
        let editor = AstEditor::new(Language::Rust);
        let source = "fn main() { let x = 10; }";
        let refs = editor.find_references_in_source(source, Path::new("test.rs"), "z");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_rename_in_source() {
        let editor = AstEditor::new(Language::Rust);
        let source = r#"fn main() {
    let count = 0;
    let total = count + 1;
}"#;
        let changes = editor.rename_in_source(source, Path::new("test.rs"), "count", "num");
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn test_rename_word_boundary() {
        let editor = AstEditor::new(Language::Rust);
        let source = "let counter = 0;\nlet count = counter + 1;";
        let changes = editor.rename_in_source(source, Path::new("test.rs"), "count", "num");
        // Should only match "count", not "counter"
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn test_find_definitions_rust() {
        let editor = AstEditor::new(Language::Rust);
        let source = r#"pub fn hello() {}
struct Foo {}
enum Bar {}
trait Baz {}
"#;
        let defs = editor.find_definitions_in_source(source, Path::new("test.rs"));
        assert_eq!(defs.len(), 4);
        assert_eq!(defs[0].text, "hello");
        assert_eq!(defs[1].text, "Foo");
        assert_eq!(defs[2].text, "Bar");
        assert_eq!(defs[3].text, "Baz");
    }

    #[test]
    fn test_find_definitions_typescript() {
        let editor = AstEditor::new(Language::TypeScript);
        let source = r#"function greet(name: string) {}
const PI = 3.14;
class User {}
interface Config {}
"#;
        let defs = editor.find_definitions_in_source(source, Path::new("test.ts"));
        assert_eq!(defs.len(), 4);
    }

    #[test]
    fn test_find_definitions_python() {
        let editor = AstEditor::new(Language::Python);
        let source = "def hello():\n    pass\nclass Foo:\n    pass\n";
        let defs = editor.find_definitions_in_source(source, Path::new("test.py"));
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].text, "hello");
        assert_eq!(defs[1].text, "Foo");
    }

    #[test]
    fn test_extract_function() {
        let editor = AstEditor::new(Language::Rust);
        let source = r#"fn main() {
    let x = 10;
    let y = x + 5;
    println!("{}", y);
}"#;
        let result = editor.extract_function(source, 2, 3, "compute").unwrap();
        assert!(result.contains("compute("));
        assert!(result.contains("fn compute("));
    }

    #[test]
    fn test_extract_function_invalid_range() {
        let editor = AstEditor::new(Language::Rust);
        let source = "fn main() {}";
        assert!(editor.extract_function(source, 0, 1, "test").is_err());
        assert!(editor.extract_function(source, 1, 5, "test").is_err());
    }

    #[test]
    fn test_for_file() {
        assert!(AstEditor::for_file(Path::new("test.rs")).is_some());
        assert!(AstEditor::for_file(Path::new("test.py")).is_some());
        assert!(AstEditor::for_file(Path::new("test.xyz")).is_none());
    }

    #[test]
    fn test_indent() {
        assert_eq!(indent("line1\nline2", "    "), "    line1\n    line2");
    }

    #[test]
    fn test_extract_last_identifier() {
        assert_eq!(extract_last_identifier("fn hello_world"), Some("hello_world"));
        assert_eq!(extract_last_identifier("pub struct Foo {"), Some("Foo"));
    }
}
