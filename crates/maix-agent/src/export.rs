//! Session export/import for conversation portability.

use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Export format for sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Readable Markdown format.
    Markdown,
    /// Full JSON data.
    Json,
    /// Plain text.
    PlainText,
}

/// A serializable message for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ExportToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// A serializable tool call for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportToolCall {
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// A complete session export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExport {
    pub session_id: String,
    pub model: String,
    pub created_at: String,
    pub messages: Vec<ExportMessage>,
    pub metadata: ExportMetadata,
}

/// Metadata about the exported session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub total_tokens: u64,
    pub total_cost: f64,
    pub message_count: usize,
    pub export_format: String,
    pub exported_at: String,
}

/// Export a session to a file.
pub fn export_to_file(
    export: &SessionExport,
    path: &Path,
    format: ExportFormat,
) -> MaixResult<()> {
    let content = match format {
        ExportFormat::Json => serde_json::to_string_pretty(export)
            .map_err(maix_core::MaixError::Json)?,
        ExportFormat::Markdown => format_markdown(export),
        ExportFormat::PlainText => format_plain_text(export),
    };

    std::fs::write(path, content).map_err(maix_core::MaixError::Io)?;
    Ok(())
}

/// Import a session from a JSON file.
pub fn import_from_file(path: &Path) -> MaixResult<SessionExport> {
    let content = std::fs::read_to_string(path).map_err(maix_core::MaixError::Io)?;
    serde_json::from_str(&content).map_err(maix_core::MaixError::Json)
}

/// Format session as Markdown.
fn format_markdown(export: &SessionExport) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Session: {}\n\n", export.session_id));
    md.push_str(&format!("- **Model**: {}\n", export.model));
    md.push_str(&format!("- **Created**: {}\n", export.created_at));
    md.push_str(&format!("- **Messages**: {}\n", export.metadata.message_count));
    md.push_str(&format!("- **Tokens**: {}\n", export.metadata.total_tokens));
    md.push_str(&format!("- **Cost**: ¥{:.4}\n\n", export.metadata.total_cost));
    md.push_str("---\n\n");

    for msg in &export.messages {
        match msg.role.as_str() {
            "user" => {
                md.push_str(&format!("## User\n\n{}\n\n", msg.content));
            }
            "assistant" => {
                if let Some(reasoning) = &msg.reasoning {
                    md.push_str(&format!("> *Reasoning: {}*\n\n", reasoning));
                }
                md.push_str(&format!("## Assistant\n\n{}\n\n", msg.content));

                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        md.push_str(&format!("### Tool: `{}`\n", tc.name));
                        md.push_str(&format!("```json\n{}\n```\n", tc.arguments));
                        if let Some(result) = &tc.result {
                            md.push_str(&format!("**Result:**\n```\n{}\n```\n\n", result));
                        }
                    }
                }
            }
            "system" => {
                md.push_str(&format!("> **System**: {}\n\n", msg.content));
            }
            _ => {}
        }
    }

    md
}

/// Format session as plain text.
fn format_plain_text(export: &SessionExport) -> String {
    let mut text = String::new();

    text.push_str(&format!("Session: {}\n", export.session_id));
    text.push_str(&format!("Model: {}\n", export.model));
    text.push_str(&format!("Messages: {}\n\n", export.metadata.message_count));

    for msg in &export.messages {
        let role = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" => "System",
            other => other,
        };
        text.push_str(&format!("{}: {}\n\n", role, msg.content));

        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                text.push_str(&format!("[Tool: {}]\n", tc.name));
                if let Some(result) = &tc.result {
                    text.push_str(&format!("Result: {}\n\n", result));
                }
            }
        }
    }

    text
}

/// Detect export format from file extension.
pub fn detect_format(path: &Path) -> ExportFormat {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => ExportFormat::Json,
        Some("md") | Some("markdown") => ExportFormat::Markdown,
        _ => ExportFormat::PlainText,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_export() -> SessionExport {
        SessionExport {
            session_id: "test-123".into(),
            model: "gpt-4o".into(),
            created_at: "2026-05-14T00:00:00Z".into(),
            messages: vec![
                ExportMessage {
                    role: "user".into(),
                    content: "Hello".into(),
                    tool_calls: None,
                    reasoning: None,
                    timestamp: None,
                },
                ExportMessage {
                    role: "assistant".into(),
                    content: "Hi there!".into(),
                    tool_calls: None,
                    reasoning: None,
                    timestamp: None,
                },
            ],
            metadata: ExportMetadata {
                total_tokens: 100,
                total_cost: 0.001,
                message_count: 2,
                export_format: "json".into(),
                exported_at: "2026-05-14T00:00:00Z".into(),
            },
        }
    }

    #[test]
    fn test_format_markdown() {
        let export = sample_export();
        let md = format_markdown(&export);
        assert!(md.contains("# Session: test-123"));
        assert!(md.contains("## User"));
        assert!(md.contains("Hello"));
        assert!(md.contains("## Assistant"));
        assert!(md.contains("Hi there!"));
    }

    #[test]
    fn test_format_plain_text() {
        let export = sample_export();
        let text = format_plain_text(&export);
        assert!(text.contains("User: Hello"));
        assert!(text.contains("Assistant: Hi there!"));
    }

    #[test]
    fn test_json_roundtrip() {
        let export = sample_export();
        let json = serde_json::to_string_pretty(&export).unwrap();
        let imported: SessionExport = serde_json::from_str(&json).unwrap();
        assert_eq!(imported.session_id, export.session_id);
        assert_eq!(imported.messages.len(), export.messages.len());
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(detect_format(Path::new("session.json")), ExportFormat::Json);
        assert_eq!(detect_format(Path::new("session.md")), ExportFormat::Markdown);
        assert_eq!(detect_format(Path::new("session.txt")), ExportFormat::PlainText);
    }
}
