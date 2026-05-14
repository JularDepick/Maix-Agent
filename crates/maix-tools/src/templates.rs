//! Code templates — insert common code patterns and snippets.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::HashMap;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// A code template with variable placeholders.
#[derive(Debug, Clone)]
pub struct CodeTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub language: String,
    pub content: String,
    pub variables: Vec<TemplateVariable>,
}

/// A variable in a template.
#[derive(Debug, Clone)]
pub struct TemplateVariable {
    pub name: String,
    pub description: String,
    pub default: Option<String>,
}

/// Template manager with built-in templates.
pub struct TemplateManager {
    templates: Vec<CodeTemplate>,
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            templates: Vec::new(),
        };
        mgr.load_builtins();
        mgr
    }

    fn load_builtins(&mut self) {
        // Rust function
        self.templates.push(CodeTemplate {
            id: "rust_fn".into(),
            name: "Rust Function".into(),
            description: "Create a new Rust function".into(),
            category: "Rust".into(),
            language: "rust".into(),
            content: "/// ${description}\npub fn ${name}(${params}) -> ${return_type} {\n    ${body}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Function description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Function name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Parameters".into(), default: Some("".into()) },
                TemplateVariable { name: "return_type".into(), description: "Return type".into(), default: Some("()".into()) },
                TemplateVariable { name: "body".into(), description: "Body".into(), default: Some("todo!()".into()) },
            ],
        });

        // Rust struct
        self.templates.push(CodeTemplate {
            id: "rust_struct".into(),
            name: "Rust Struct".into(),
            description: "Create a new Rust struct with derives".into(),
            category: "Rust".into(),
            language: "rust".into(),
            content: "/// ${description}\n#[derive(Debug, Clone)]\npub struct ${name} {\n    ${fields}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Struct description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Struct name".into(), default: None },
                TemplateVariable { name: "fields".into(), description: "Fields".into(), default: None },
            ],
        });

        // Rust test
        self.templates.push(CodeTemplate {
            id: "rust_test".into(),
            name: "Rust Test".into(),
            description: "Create a Rust test function".into(),
            category: "Rust".into(),
            language: "rust".into(),
            content: "#[test]\nfn ${name}() {\n    // Arrange\n    ${arrange}\n\n    // Act\n    ${act}\n\n    // Assert\n    ${assert}\n}".into(),
            variables: vec![
                TemplateVariable { name: "name".into(), description: "Test name".into(), default: None },
                TemplateVariable { name: "arrange".into(), description: "Setup".into(), default: Some("".into()) },
                TemplateVariable { name: "act".into(), description: "Action".into(), default: Some("".into()) },
                TemplateVariable { name: "assert".into(), description: "Assertion".into(), default: Some("assert!(true);".into()) },
            ],
        });

        // Rust impl block
        self.templates.push(CodeTemplate {
            id: "rust_impl".into(),
            name: "Rust Impl Block".into(),
            description: "Create an impl block".into(),
            category: "Rust".into(),
            language: "rust".into(),
            content: "impl ${name} {\n    pub fn new(${params}) -> Self {\n        Self {\n            ${init}\n        }\n    }\n}".into(),
            variables: vec![
                TemplateVariable { name: "name".into(), description: "Type name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Constructor params".into(), default: Some("".into()) },
                TemplateVariable { name: "init".into(), description: "Field initialization".into(), default: None },
            ],
        });

        // TypeScript interface
        self.templates.push(CodeTemplate {
            id: "ts_interface".into(),
            name: "TypeScript Interface".into(),
            description: "Create a TypeScript interface".into(),
            category: "TypeScript".into(),
            language: "typescript".into(),
            content: "/**\n * ${description}\n */\nexport interface ${name} {\n    ${fields}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Interface description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Interface name".into(), default: None },
                TemplateVariable { name: "fields".into(), description: "Fields".into(), default: None },
            ],
        });

        // TypeScript function
        self.templates.push(CodeTemplate {
            id: "ts_function".into(),
            name: "TypeScript Function".into(),
            description: "Create a TypeScript function".into(),
            category: "TypeScript".into(),
            language: "typescript".into(),
            content: "/**\n * ${description}\n */\nexport function ${name}(${params}): ${return_type} {\n    ${body}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Function description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Function name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Parameters".into(), default: Some("".into()) },
                TemplateVariable { name: "return_type".into(), description: "Return type".into(), default: Some("void".into()) },
                TemplateVariable { name: "body".into(), description: "Body".into(), default: Some("// TODO".into()) },
            ],
        });

        // Python function
        self.templates.push(CodeTemplate {
            id: "py_function".into(),
            name: "Python Function".into(),
            description: "Create a Python function with docstring".into(),
            category: "Python".into(),
            language: "python".into(),
            content: "def ${name}(${params}) -> ${return_type}:\n    \"\"\"${description}\"\"\"\n    ${body}".into(),
            variables: vec![
                TemplateVariable { name: "name".into(), description: "Function name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Parameters".into(), default: Some("".into()) },
                TemplateVariable { name: "return_type".into(), description: "Return type".into(), default: Some("None".into()) },
                TemplateVariable { name: "description".into(), description: "Docstring".into(), default: None },
                TemplateVariable { name: "body".into(), description: "Body".into(), default: Some("pass".into()) },
            ],
        });

        // Python class
        self.templates.push(CodeTemplate {
            id: "py_class".into(),
            name: "Python Class".into(),
            description: "Create a Python class".into(),
            category: "Python".into(),
            language: "python".into(),
            content: "class ${name}:\n    \"\"\"${description}\"\"\"\n\n    def __init__(self${params}):\n        ${init}\n\n    ${methods}".into(),
            variables: vec![
                TemplateVariable { name: "name".into(), description: "Class name".into(), default: None },
                TemplateVariable { name: "description".into(), description: "Docstring".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Init params".into(), default: Some("".into()) },
                TemplateVariable { name: "init".into(), description: "Init body".into(), default: Some("pass".into()) },
                TemplateVariable { name: "methods".into(), description: "Methods".into(), default: Some("pass".into()) },
            ],
        });

        // Go function
        self.templates.push(CodeTemplate {
            id: "go_function".into(),
            name: "Go Function".into(),
            description: "Create a Go function".into(),
            category: "Go".into(),
            language: "go".into(),
            content: "// ${description}\nfunc ${name}(${params}) ${return_type} {\n    ${body}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Function description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Function name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Parameters".into(), default: Some("".into()) },
                TemplateVariable { name: "return_type".into(), description: "Return type".into(), default: Some("".into()) },
                TemplateVariable { name: "body".into(), description: "Body".into(), default: Some("// TODO".into()) },
            ],
        });

        // HTTP handler
        self.templates.push(CodeTemplate {
            id: "http_handler".into(),
            name: "HTTP Handler".into(),
            description: "Create an HTTP request handler (Rust axum)".into(),
            category: "Web".into(),
            language: "rust".into(),
            content: "/// ${description}\npub async fn ${name}(\n    ${params}\n) -> impl IntoResponse {\n    ${body}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "Handler description".into(), default: None },
                TemplateVariable { name: "name".into(), description: "Handler name".into(), default: None },
                TemplateVariable { name: "params".into(), description: "Parameters".into(), default: Some("State(state): State<AppState>".into()) },
                TemplateVariable { name: "body".into(), description: "Body".into(), default: Some("Json(json!({\"ok\": true}))".into()) },
            ],
        });

        // CLI argument parser
        self.templates.push(CodeTemplate {
            id: "rust_cli".into(),
            name: "Rust CLI with clap".into(),
            description: "Create a CLI argument parser with clap".into(),
            category: "Rust".into(),
            language: "rust".into(),
            content: "use clap::Parser;\n\n/// ${description}\n#[derive(Parser, Debug)]\n#[command(author, version, about)]\nstruct Args {\n    /// ${arg_description}\n    #[arg(short, long)]\n    ${arg_name}: ${arg_type},\n}\n\nfn main() {\n    let args = Args::parse();\n    ${body}\n}".into(),
            variables: vec![
                TemplateVariable { name: "description".into(), description: "CLI description".into(), default: None },
                TemplateVariable { name: "arg_description".into(), description: "First arg description".into(), default: None },
                TemplateVariable { name: "arg_name".into(), description: "First arg name".into(), default: Some("input".into()) },
                TemplateVariable { name: "arg_type".into(), description: "First arg type".into(), default: Some("String".into()) },
                TemplateVariable { name: "body".into(), description: "Main body".into(), default: Some("println!(\"{:?}\", args);".into()) },
            ],
        });
    }

    /// List all templates.
    pub fn list(&self) -> &[CodeTemplate] {
        &self.templates
    }

    /// Get a template by ID.
    pub fn get(&self, id: &str) -> Option<&CodeTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Search templates by query.
    pub fn search(&self, query: &str) -> Vec<&CodeTemplate> {
        let q = query.to_lowercase();
        self.templates
            .iter()
            .filter(|t| {
                t.id.to_lowercase().contains(&q)
                    || t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.category.to_lowercase().contains(&q)
                    || t.language.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Expand a template with given variables.
    pub fn expand(
        &self,
        id: &str,
        vars: &HashMap<String, String>,
    ) -> MaixResult<String> {
        let template = self
            .get(id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("Template not found: {id}")))?;

        let mut result = template.content.clone();

        for var in &template.variables {
            let value = vars
                .get(&var.name)
                .or(var.default.as_ref())
                .cloned()
                .unwrap_or_else(|| format!("${{{}}}", var.name));

            result = result.replace(&format!("${{{}}}", var.name), &value);
        }

        Ok(result)
    }

    /// Format template list for display.
    pub fn format_list(&self) -> String {
        let mut lines = vec!["Available templates:".to_string()];

        let mut categories: Vec<&str> = self
            .templates
            .iter()
            .map(|t| t.category.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        categories.sort();

        for cat in &categories {
            lines.push(format!("\n  {}:", cat));
            for t in self.templates.iter().filter(|t| t.category == *cat) {
                lines.push(format!("    {:<20} {}", t.id, t.description));
            }
        }
        lines.join("\n")
    }

    /// Format template details for display.
    pub fn format_detail(&self, id: &str) -> MaixResult<String> {
        let template = self
            .get(id)
            .ok_or_else(|| maix_core::MaixError::Tool(format!("Template not found: {id}")))?;

        let mut lines = vec![
            format!("Template: {} ({})", template.name, template.id),
            format!("Category: {} | Language: {}", template.category, template.language),
            format!("Description: {}", template.description),
            "".to_string(),
            "Variables:".to_string(),
        ];

        for var in &template.variables {
            let default = var
                .default
                .as_deref()
                .map(|d| format!(" (default: '{}')", d))
                .unwrap_or_default();
            lines.push(format!("  ${{{}}} - {}{}", var.name, var.description, default));
        }

        lines.push("".to_string());
        lines.push("Preview:".to_string());
        for line in template.content.lines() {
            lines.push(format!("  {}", line));
        }

        Ok(lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// List or search code templates.
pub struct TemplateListTool;

#[async_trait]
impl Tool for TemplateListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "template_list".into(),
            description: "List available code templates, optionally filtered by query.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (optional)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let mgr = TemplateManager::new();
        if let Some(query) = args["query"].as_str() {
            let results = mgr.search(query);
            if results.is_empty() {
                return Ok(format!("No templates matching '{}'", query));
            }
            let mut lines = vec![format!("Templates matching '{}':", query)];
            for t in results {
                lines.push(format!("  {:<20} {} [{}]", t.id, t.description, t.language));
            }
            Ok(lines.join("\n"))
        } else {
            Ok(mgr.format_list())
        }
    }
}

/// Show template details and expand with variables.
pub struct TemplateExpandTool;

#[async_trait]
impl Tool for TemplateExpandTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "template_expand".into(),
            description: "Expand a code template with given variables. Returns the filled-in code.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "template_id": { "type": "string", "description": "Template ID (e.g. 'rust_fn')" },
                    "variables": { "type": "object", "description": "Variable values as key-value pairs" }
                },
                "required": ["template_id"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let template_id = args["template_id"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'template_id'".into()))?;

        let mgr = TemplateManager::new();

        // If no variables provided, show template detail
        if args["variables"].as_object().map(|o| o.is_empty()).unwrap_or(true) && args["variables"].as_object().is_none() {
            return mgr.format_detail(template_id);
        }

        let mut vars = HashMap::new();
        if let Some(obj) = args["variables"].as_object() {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    vars.insert(k.clone(), s.to_string());
                }
            }
        }

        mgr.expand(template_id, &vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_templates() {
        let mgr = TemplateManager::new();
        assert!(!mgr.list().is_empty());
        assert!(mgr.get("rust_fn").is_some());
    }

    #[test]
    fn test_search() {
        let mgr = TemplateManager::new();
        let results = mgr.search("rust");
        assert!(!results.is_empty());
        assert!(results.iter().all(|t| t.id.to_lowercase().contains("rust")
            || t.name.to_lowercase().contains("rust")
            || t.category.to_lowercase().contains("rust")
            || t.language.to_lowercase().contains("rust")
            || t.description.to_lowercase().contains("rust")));
    }

    #[test]
    fn test_expand() {
        let mgr = TemplateManager::new();
        let mut vars = HashMap::new();
        vars.insert("description".into(), "Add two numbers".into());
        vars.insert("name".into(), "add".into());
        vars.insert("params".into(), "a: i32, b: i32".into());
        vars.insert("return_type".into(), "i32".into());
        vars.insert("body".into(), "a + b".into());

        let result = mgr.expand("rust_fn", &vars).unwrap();
        assert!(result.contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(result.contains("a + b"));
    }
}
