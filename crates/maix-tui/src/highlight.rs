//! Lightweight syntax highlighting for code blocks.
//!
//! Token-based lexer for Rust, Python, TypeScript, Go with keyword/string/comment coloring.

/// Token types for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Operator,
    Punctuation,
    Plain,
}

#[allow(dead_code)]
impl TokenKind {
    /// ANSI color code for this token kind.
    pub fn color(&self) -> &'static str {
        match self {
            Self::Keyword => "\x1b[34m",      // blue
            Self::String => "\x1b[32m",       // green
            Self::Comment => "\x1b[90m",      // gray
            Self::Number => "\x1b[36m",       // cyan
            Self::Function => "\x1b[33m",     // yellow
            Self::Type => "\x1b[35m",         // magenta
            Self::Operator => "\x1b[37m",     // white
            Self::Punctuation => "\x1b[37m",  // white
            Self::Plain => "\x1b[0m",         // reset
        }
    }

    pub fn reset(&self) -> &'static str {
        "\x1b[0m"
    }
}

/// A highlighted token.
#[derive(Debug, Clone)]
pub struct HighlightedToken {
    pub text: String,
    pub kind: TokenKind,
}

#[allow(dead_code)]
impl HighlightedToken {
    pub fn new(text: &str, kind: TokenKind) -> Self {
        Self {
            text: text.to_string(),
            kind,
        }
    }

    pub fn render_ansi(&self) -> String {
        format!("{}{}{}", self.kind.color(), self.text, self.kind.reset())
    }
}

/// Supported languages for highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    Go,
    Java,
    Cpp,
    Ruby,
    Shell,
    C,
    CSharp,
    Json,
    Yaml,
    Toml,
    Sql,
    Lua,
    Php,
    Swift,
    Kotlin,
    Unknown,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Self::Rust,
            "py" | "pyw" => Self::Python,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Self::TypeScript,
            "go" => Self::Go,
            "java" => Self::Java,
            "cpp" | "cc" | "cxx" | "h" | "hpp" | "hh" => Self::Cpp,
            "rb" | "erb" => Self::Ruby,
            "sh" | "bash" | "zsh" | "fish" => Self::Shell,
            "c" => Self::C,
            "cs" => Self::CSharp,
            "json" | "jsonc" | "json5" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "sql" | "mysql" | "pgsql" | "sqlite" => Self::Sql,
            "lua" => Self::Lua,
            "php" => Self::Php,
            "swift" => Self::Swift,
            "kt" | "kts" => Self::Kotlin,
            _ => Self::Unknown,
        }
    }

    fn keywords(&self) -> &[&str] {
        match self {
            Self::Rust => &[
                "fn", "let", "mut", "pub", "struct", "enum", "trait", "impl",
                "use", "mod", "const", "static", "if", "else", "match", "for",
                "while", "loop", "return", "self", "Self", "true", "false",
                "async", "await", "move", "ref", "where", "as", "in", "break",
                "continue", "crate", "super", "type", "where", "unsafe", "extern",
            ],
            Self::Python => &[
                "def", "class", "if", "else", "elif", "for", "while", "return",
                "import", "from", "as", "with", "try", "except", "finally",
                "raise", "pass", "break", "continue", "and", "or", "not",
                "True", "False", "None", "self", "lambda", "yield", "async", "await",
            ],
            Self::TypeScript => &[
                "function", "const", "let", "var", "class", "interface", "type",
                "if", "else", "for", "while", "return", "import", "export",
                "from", "async", "await", "new", "this", "true", "false",
                "null", "undefined", "void", "string", "number", "boolean",
                "extends", "implements", "enum", "namespace", "module",
            ],
            Self::Go => &[
                "func", "var", "const", "type", "struct", "interface", "if",
                "else", "for", "range", "return", "import", "package", "go",
                "defer", "select", "case", "switch", "default", "break",
                "continue", "fallthrough", "true", "false", "nil", "make", "new",
                "map", "chan", "select",
            ],
            Self::Java => &[
                "class", "interface", "extends", "implements", "if", "else", "for",
                "while", "return", "import", "package", "new", "this", "super",
                "true", "false", "null", "void", "int", "long", "double", "float",
                "boolean", "char", "string", "public", "private", "protected",
                "static", "final", "abstract", "synchronized", "try", "catch",
                "finally", "throw", "throws", "break", "continue", "switch",
                "case", "default", "do", "instanceof", "enum", "var",
            ],
            Self::Cpp | Self::C => &[
                "int", "long", "double", "float", "char", "void", "bool",
                "if", "else", "for", "while", "return", "include", "define",
                "typedef", "struct", "class", "public", "private", "protected",
                "virtual", "override", "new", "delete", "nullptr", "true", "false",
                "break", "continue", "switch", "case", "default", "do", "goto",
                "const", "static", "extern", "auto", "sizeof", "namespace",
                "using", "template", "typename", "try", "catch", "throw",
            ],
            Self::Ruby => &[
                "def", "class", "module", "if", "elsif", "else", "unless", "for",
                "while", "until", "return", "yield", "begin", "rescue", "ensure",
                "end", "true", "false", "nil", "self", "super", "and", "or", "not",
                "do", "break", "next", "redo", "retry", "in", "case", "when",
                "require", "include", "extend", "attr_accessor", "attr_reader",
            ],
            Self::Shell => &[
                "if", "then", "else", "elif", "fi", "for", "while", "do", "done",
                "case", "esac", "function", "return", "exit", "export", "source",
                "local", "readonly", "declare", "typeset", "unset", "shift",
                "echo", "printf", "read", "test", "true", "false",
            ],
            Self::CSharp => &[
                "class", "interface", "struct", "enum", "if", "else", "for",
                "foreach", "while", "return", "using", "namespace", "new", "this",
                "base", "true", "false", "null", "void", "int", "long", "double",
                "float", "bool", "char", "string", "var", "public", "private",
                "protected", "internal", "static", "virtual", "override", "abstract",
                "sealed", "try", "catch", "finally", "throw", "break", "continue",
                "switch", "case", "default", "do", "is", "as", "async", "await",
                "get", "set", "init", "record",
            ],
            Self::Json => &["true", "false", "null"],
            Self::Yaml => &["true", "false", "null", "yes", "no", "on", "off"],
            Self::Toml => &["true", "false"],
            Self::Sql => &[
                "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "CREATE",
                "DROP", "ALTER", "TABLE", "INDEX", "VIEW", "JOIN", "LEFT", "RIGHT",
                "INNER", "OUTER", "ON", "AND", "OR", "NOT", "IN", "BETWEEN", "LIKE",
                "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "AS", "SET",
                "VALUES", "INTO", "NULL", "IS", "DISTINCT", "UNION", "ALL", "EXISTS",
                "CASE", "WHEN", "THEN", "ELSE", "END", "BEGIN", "COMMIT", "ROLLBACK",
                "GRANT", "REVOKE", "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "CHECK",
                "DEFAULT", "AUTO_INCREMENT", "SERIAL", "VARCHAR", "INTEGER", "TEXT",
                "BOOLEAN", "DATE", "TIMESTAMP", "select", "from", "where", "insert",
                "update", "delete", "create", "drop", "alter", "table", "join",
                "on", "and", "or", "not", "in", "order", "by", "group", "having",
                "limit", "as", "set", "values", "into", "null", "is", "distinct",
            ],
            Self::Lua => &[
                "and", "break", "do", "else", "elseif", "end", "false", "for",
                "function", "if", "in", "local", "nil", "not", "or", "repeat",
                "return", "then", "true", "until", "while", "goto",
            ],
            Self::Php => &[
                "abstract", "and", "array", "as", "break", "callable", "case",
                "catch", "class", "clone", "const", "continue", "declare", "default",
                "die", "do", "echo", "else", "elseif", "empty", "enddeclare",
                "endfor", "endforeach", "endif", "endswitch", "endwhile", "eval",
                "exit", "extends", "final", "finally", "fn", "for", "foreach",
                "function", "global", "goto", "if", "implements", "include",
                "include_once", "instanceof", "insteadof", "interface", "isset",
                "list", "match", "namespace", "new", "or", "print", "private",
                "protected", "public", "readonly", "require", "require_once",
                "return", "static", "switch", "throw", "trait", "try", "unset",
                "use", "var", "while", "xor", "yield",
            ],
            Self::Swift => &[
                "associatedtype", "class", "deinit", "enum", "extension", "func",
                "import", "init", "inout", "internal", "let", "operator", "private",
                "protocol", "public", "static", "struct", "subscript", "typealias",
                "var", "break", "case", "continue", "default", "defer", "do",
                "else", "fallthrough", "for", "guard", "if", "in", "repeat",
                "return", "switch", "where", "while", "as", "catch", "false", "is",
                "nil", "rethrows", "super", "self", "Self", "throw", "throws",
                "true", "try", "async", "await", "actor", "some", "any",
            ],
            Self::Kotlin => &[
                "abstract", "actual", "annotation", "as", "break", "by", "catch",
                "class", "companion", "const", "constructor", "continue", "crossinline",
                "data", "delegate", "do", "dynamic", "else", "enum", "expect",
                "external", "false", "final", "finally", "for", "fun", "get",
                "if", "import", "in", "infix", "init", "inline", "inner",
                "interface", "internal", "is", "it", "lateinit", "lazy", "noinline",
                "null", "object", "open", "operator", "out", "override", "package",
                "private", "protected", "public", "reified", "return", "sealed",
                "set", "super", "suspend", "tailrec", "this", "throw", "true",
                "try", "typealias", "val", "var", "vararg", "when", "where", "while",
            ],
            Self::Unknown => &[],
        }
    }

    fn type_keywords(&self) -> &[&str] {
        match self {
            Self::Rust => &["String", "Vec", "Option", "Result", "Box", "Arc", "Mutex", "HashMap", "HashSet", "usize", "u8", "u16", "u32", "u64", "i32", "i64", "f32", "f64", "bool", "str"],
            Self::Python => &["int", "float", "str", "list", "dict", "tuple", "set", "bool", "bytes", "None"],
            Self::TypeScript => &["string", "number", "boolean", "any", "void", "never", "unknown", "object", "Array", "Promise", "Record", "Partial", "Required"],
            Self::Go => &["string", "int", "int32", "int64", "float32", "float64", "bool", "byte", "error", "any"],
            Self::Java => &["String", "Integer", "Long", "Double", "Float", "Boolean", "Character", "Object", "List", "Map", "Set", "ArrayList", "HashMap", "HashSet"],
            Self::Cpp | Self::C => &["string", "vector", "map", "set", "list", "queue", "stack", "pair", "shared_ptr", "unique_ptr", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "size_t"],
            Self::Ruby => &["String", "Integer", "Float", "Array", "Hash", "Symbol", "NilClass", "TrueClass", "FalseClass"],
            Self::Shell => &[],
            Self::CSharp => &["string", "int", "long", "double", "float", "bool", "char", "object", "dynamic", "List", "Dictionary", "HashSet", "Task", "Action", "Func"],
            Self::Json => &[],
            Self::Yaml => &[],
            Self::Toml => &[],
            Self::Sql => &["INT", "INTEGER", "BIGINT", "SMALLINT", "TINYINT", "FLOAT", "DOUBLE", "DECIMAL", "NUMERIC", "VARCHAR", "CHAR", "TEXT", "BLOB", "CLOB", "DATE", "TIME", "TIMESTAMP", "DATETIME", "BOOLEAN", "BOOL", "UUID", "SERIAL", "JSON", "JSONB", "ARRAY"],
            Self::Lua => &[],
            Self::Php => &["int", "float", "string", "bool", "array", "object", "callable", "iterable", "void", "null", "mixed", "never", "self", "static", "parent"],
            Self::Swift => &["Int", "Int8", "Int16", "Int32", "Int64", "UInt", "UInt8", "UInt16", "UInt32", "UInt64", "Float", "Double", "Bool", "String", "Character", "Array", "Dictionary", "Set", "Optional", "Any", "AnyObject", "Error", "Result"],
            Self::Kotlin => &["Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "String", "Array", "List", "Map", "Set", "Pair", "Triple", "Any", "Unit", "Nothing"],
            Self::Unknown => &[],
        }
    }
}

/// Simple syntax highlighter.
pub struct SimpleHighlighter {
    language: Language,
}

#[allow(dead_code)]
impl SimpleHighlighter {
    pub fn new(language: Language) -> Self {
        Self { language }
    }

    pub fn from_extension(ext: &str) -> Self {
        Self::new(Language::from_extension(ext))
    }

    /// Tokenize a line of code into highlighted tokens.
    pub fn highlight_line(&self, line: &str) -> Vec<HighlightedToken> {
        if self.language == Language::Unknown {
            return vec![HighlightedToken::new(line, TokenKind::Plain)];
        }

        let mut tokens = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Skip whitespace
            if chars[i].is_whitespace() {
                let start = i;
                while i < len && chars[i].is_whitespace() {
                    i += 1;
                }
                tokens.push(HighlightedToken::new(&line[start..i], TokenKind::Plain));
                continue;
            }

            // Comments
            if self.is_comment_start(&chars, i) {
                tokens.push(HighlightedToken::new(&line[i..], TokenKind::Comment));
                break;
            }

            // Strings
            if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
                let start = i;
                let quote = chars[i];
                i += 1;
                while i < len && chars[i] != quote {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 1;
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                tokens.push(HighlightedToken::new(&line[start..i], TokenKind::String));
                continue;
            }

            // Numbers
            if chars[i].is_ascii_digit() {
                let start = i;
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_' || chars[i] == 'x' || chars[i] == 'b') {
                    i += 1;
                }
                tokens.push(HighlightedToken::new(&line[start..i], TokenKind::Number));
                continue;
            }

            // Identifiers and keywords
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word = &line[start..i];

                // Check if followed by '(' => function call
                let is_func = i < len && chars[i] == '(';

                let kind = if self.language.keywords().contains(&word) {
                    TokenKind::Keyword
                } else if self.language.type_keywords().contains(&word) {
                    TokenKind::Type
                } else if is_func {
                    TokenKind::Function
                } else if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) && self.language == Language::Rust {
                    TokenKind::Type
                } else {
                    TokenKind::Plain
                };

                tokens.push(HighlightedToken::new(word, kind));
                continue;
            }

            // Operators
            if "+-*/%=<>!&|^~?".contains(chars[i]) {
                let start = i;
                while i < len && "+-*/%=<>!&|^~?".contains(chars[i]) {
                    i += 1;
                }
                tokens.push(HighlightedToken::new(&line[start..i], TokenKind::Operator));
                continue;
            }

            // Punctuation
            if "{}()[];:,.".contains(chars[i]) {
                tokens.push(HighlightedToken::new(&line[i..i + 1], TokenKind::Punctuation));
                i += 1;
                continue;
            }

            // Default
            tokens.push(HighlightedToken::new(&line[i..i + 1], TokenKind::Plain));
            i += 1;
        }

        tokens
    }

    /// Highlight a full code block.
    pub fn highlight_block(&self, code: &str) -> Vec<Vec<HighlightedToken>> {
        code.lines().map(|line| self.highlight_line(line)).collect()
    }

    /// Render a code block with ANSI colors.
    pub fn render_ansi(&self, code: &str) -> String {
        self.highlight_block(code)
            .iter()
            .map(|tokens| {
                tokens.iter().map(|t| t.render_ansi()).collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn is_comment_start(&self, chars: &[char], pos: usize) -> bool {
        match self.language {
            Language::Rust | Language::TypeScript | Language::Go | Language::Java | Language::Cpp | Language::C | Language::CSharp | Language::Kotlin | Language::Swift | Language::Php => {
                pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/'
            }
            Language::Python | Language::Ruby | Language::Shell | Language::Yaml | Language::Toml => {
                pos < chars.len() && chars[pos] == '#'
            }
            Language::Lua => {
                pos + 1 < chars.len() && chars[pos] == '-' && chars[pos + 1] == '-'
            }
            Language::Sql => {
                pos + 1 < chars.len() && chars[pos] == '-' && chars[pos + 1] == '-'
            }
            Language::Json => false,
            Language::Unknown => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("xyz"), Language::Unknown);
    }

    #[test]
    fn test_highlight_rust_keyword() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line("fn main() {");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "fn"));
    }

    #[test]
    fn test_highlight_rust_string() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line(r#"let s = "hello";"#);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String && t.text == "\"hello\""));
    }

    #[test]
    fn test_highlight_rust_comment() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line("let x = 1; // comment");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_highlight_number() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line("let x = 42;");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Number && t.text == "42"));
    }

    #[test]
    fn test_highlight_function_call() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line("println(\"hello\");");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Function && t.text == "println"));
    }

    #[test]
    fn test_highlight_python_comment() {
        let h = SimpleHighlighter::new(Language::Python);
        let tokens = h.highlight_line("# this is a comment");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_highlight_python_keyword() {
        let h = SimpleHighlighter::new(Language::Python);
        let tokens = h.highlight_line("def hello():");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "def"));
    }

    #[test]
    fn test_highlight_unknown_language() {
        let h = SimpleHighlighter::new(Language::Unknown);
        let tokens = h.highlight_line("anything goes here");
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Plain));
    }

    #[test]
    fn test_highlight_block() {
        let h = SimpleHighlighter::new(Language::Rust);
        let code = "fn main() {\n    println!(\"hi\");\n}";
        let block = h.highlight_block(code);
        assert_eq!(block.len(), 3);
    }

    #[test]
    fn test_render_ansi() {
        let h = SimpleHighlighter::new(Language::Rust);
        let rendered = h.render_ansi("let x = 1;");
        assert!(rendered.contains("\x1b[34m")); // keyword color
        assert!(rendered.contains("\x1b[0m"));   // reset
    }

    #[test]
    fn test_token_ansi_output() {
        let token = HighlightedToken::new("fn", TokenKind::Keyword);
        let ansi = token.render_ansi();
        assert!(ansi.starts_with("\x1b[34m"));
        assert!(ansi.contains("fn"));
        assert!(ansi.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_highlight_type() {
        let h = SimpleHighlighter::new(Language::Rust);
        let tokens = h.highlight_line("let v: Vec<String> = Vec::new();");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Type && t.text == "Vec"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Type && t.text == "String"));
    }

    #[test]
    fn test_highlight_go_keyword() {
        let h = SimpleHighlighter::new(Language::Go);
        let tokens = h.highlight_line("func main() {");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "func"));
    }

    #[test]
    fn test_highlight_json() {
        let h = SimpleHighlighter::new(Language::Json);
        let tokens = h.highlight_line(r#"{"key": true, "value": null}"#);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "true"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "null"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String && t.text.contains("key")));
    }

    #[test]
    fn test_highlight_yaml() {
        let h = SimpleHighlighter::new(Language::Yaml);
        let tokens = h.highlight_line("enabled: true");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "true"));
    }

    #[test]
    fn test_highlight_sql() {
        let h = SimpleHighlighter::new(Language::Sql);
        let tokens = h.highlight_line("SELECT * FROM users WHERE id = 1");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "SELECT"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "FROM"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "WHERE"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Number && t.text == "1"));
    }

    #[test]
    fn test_highlight_lua() {
        let h = SimpleHighlighter::new(Language::Lua);
        let tokens = h.highlight_line("local x = function() return true end");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "local"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "function"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "return"));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "true"));
    }

    #[test]
    fn test_highlight_lua_comment() {
        let h = SimpleHighlighter::new(Language::Lua);
        let tokens = h.highlight_line("-- this is a comment");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_highlight_toml() {
        let h = SimpleHighlighter::new(Language::Toml);
        let tokens = h.highlight_line("debug = true");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword && t.text == "true"));
    }

    #[test]
    fn test_language_extensions() {
        assert_eq!(Language::from_extension("json"), Language::Json);
        assert_eq!(Language::from_extension("yaml"), Language::Yaml);
        assert_eq!(Language::from_extension("yml"), Language::Yaml);
        assert_eq!(Language::from_extension("toml"), Language::Toml);
        assert_eq!(Language::from_extension("sql"), Language::Sql);
        assert_eq!(Language::from_extension("lua"), Language::Lua);
        assert_eq!(Language::from_extension("php"), Language::Php);
        assert_eq!(Language::from_extension("swift"), Language::Swift);
        assert_eq!(Language::from_extension("kt"), Language::Kotlin);
    }
}
