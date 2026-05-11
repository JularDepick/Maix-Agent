//! Data processing tools — JSON, TOML, text transformation.

use crate::{ToolCtx, ToolResult};
use serde_json::Value;

pub async fn json_parse(_ctx: &ToolCtx, args: Value) -> ToolResult {
    let input = args["input"].as_str().unwrap_or("");
    match serde_json::from_str::<Value>(input) {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("format error: {e}")),
        Err(e) => format!("json parse error: {e}"),
    }
}

pub async fn toml_parse(_ctx: &ToolCtx, args: Value) -> ToolResult {
    let input = args["input"].as_str().unwrap_or("");
    match toml::from_str::<toml::Value>(input) {
        Ok(v) => format!("{v:#?}"),
        Err(e) => format!("toml parse error: {e}"),
    }
}

pub async fn text_transform(_ctx: &ToolCtx, args: Value) -> ToolResult {
    let input = args["input"].as_str().unwrap_or("");
    let op = args["operation"].as_str().unwrap_or("lowercase");
    match op {
        "uppercase" => input.to_uppercase(),
        "lowercase" => input.to_lowercase(),
        "trim" => input.trim().to_string(),
        "lines" => format!("{} lines", input.lines().count()),
        "words" => format!("{} words", input.split_whitespace().count()),
        _ => format!("unknown operation: {op}. Supported: uppercase, lowercase, trim, lines, words"),
    }
}
