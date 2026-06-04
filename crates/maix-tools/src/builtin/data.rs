//! Data tools: json_parse, toml_parse, text_transform.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};

// ---------------------------------------------------------------------------
// json_parse
// ---------------------------------------------------------------------------

pub struct JsonParseTool;

impl Default for JsonParseTool {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonParseTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for JsonParseTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "json_parse".into(),
            description: "Parse and pretty-print a JSON string".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "JSON string to parse" }
                },
                "required": ["input"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::data::json_parse(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// toml_parse
// ---------------------------------------------------------------------------

pub struct TomlParseTool;

impl Default for TomlParseTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TomlParseTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TomlParseTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "toml_parse".into(),
            description: "Parse and pretty-print a TOML string".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "TOML string to parse" }
                },
                "required": ["input"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::data::toml_parse(ctx, args).await)
    }
}

// ---------------------------------------------------------------------------
// text_transform
// ---------------------------------------------------------------------------

pub struct TextTransformTool;

impl Default for TextTransformTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TextTransformTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TextTransformTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "text_transform".into(),
            description: "Transform text (uppercase, lowercase, trim, count lines/words)".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Text to transform" },
                    "operation": { "type": "string", "description": "Operation: uppercase, lowercase, trim, lines, words" }
                },
                "required": ["input", "operation"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        Ok(crate::data::text_transform(ctx, args).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolCtx {
        ToolCtx {
            session_id: "test".into(),
            working_dir: ".".into(),
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn test_json_parse_valid() {
        let tool = JsonParseTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"input": "{\"key\": \"value\"}"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[tokio::test]
    async fn test_json_parse_invalid() {
        let tool = JsonParseTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"input": "not json"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("error"));
    }

    #[tokio::test]
    async fn test_toml_parse_valid() {
        let tool = TomlParseTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"input": "[package]\nname = \"test\""});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("package"));
        assert!(result.contains("test"));
    }

    #[tokio::test]
    async fn test_text_transform_uppercase() {
        let tool = TextTransformTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"input": "hello", "operation": "uppercase"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("HELLO"));
    }

    #[tokio::test]
    async fn test_text_transform_word_count() {
        let tool = TextTransformTool::new();
        let ctx = test_ctx();
        let args = serde_json::json!({"input": "hello world foo", "operation": "words"});
        let result = tool.execute(&ctx, args).await.unwrap();
        assert!(result.contains("3"));
    }
}
