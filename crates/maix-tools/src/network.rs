//! Network tools — HTTP requests, web fetching, web search.
//!
//! WebFetchTool supports an optional `prompt` parameter: when provided, the
//! fetched HTML is converted to markdown and combined with the prompt for the
//! agent's LLM to process.

use crate::{RiskLevel, Tool, ToolCtx, ToolDef};
use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::Value;

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; MaixAgent/1.0)")
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_fetch".into(),
            description: "Fetch content from a URL. Converts HTML to markdown. Optionally include a prompt for the agent to process the content.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" },
                    "prompt": { "type": "string", "description": "Optional prompt to process the fetched content (e.g., \"summarize this page\", \"extract all API endpoints\")" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default: 30, max: 120)" }
                },
                "required": ["url"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let url = args["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Err(maix_core::MaixError::Tool("web_fetch: url is required".into()));
        }
        if url.len() > 4096 {
            return Err(maix_core::MaixError::Tool("web_fetch: url too long (max 4KB)".into()));
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(maix_core::MaixError::Tool("web_fetch: url must start with http:// or https://".into()));
        }

        let prompt = args["prompt"].as_str();
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30).min(120);

        let resp = self.client
            .get(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_fetch: {e}")))?;

        let status = resp.status();

        // Check for redirect
        if status.is_redirection() {
            if let Some(location) = resp.headers().get("location") {
                let loc = location.to_str().unwrap_or("unknown");
                return Ok(format!("HTTP {status} — Redirect to: {loc}\n\nRe-fetch at the new URL."));
            }
        }

        let content_type = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_fetch read: {e}")))?;

        // Convert HTML to markdown-like text
        let markdown = if content_type.contains("html") {
            html_to_markdown(&body)
        } else {
            body.clone()
        };

        // Truncate to reasonable size
        let max_chars = 16000;
        let truncated = if markdown.len() > max_chars {
            format!("{}...\n\n(truncated, {} total chars)", &markdown[..max_chars], markdown.len())
        } else {
            markdown
        };

        // Format output
        let mut output = format!("HTTP {status} ({})\nURL: {url}\n", content_type);

        if let Some(p) = prompt {
            output.push_str(&format!("\nPrompt: {p}\n\nContent:\n{truncated}"));
        } else {
            output.push_str(&format!("\n{truncated}"));
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// HTML to Markdown conversion
// ---------------------------------------------------------------------------

/// Convert HTML to a readable markdown-like format.
fn html_to_markdown(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_buffer = String::new();
    let mut text_buffer = String::new();

    for ch in html.chars() {
        match ch {
            '<' => {
                // Flush text buffer
                if !text_buffer.is_empty() && !in_script && !in_style {
                    let text = text_buffer.trim();
                    if !text.is_empty() {
                        result.push_str(text);
                        result.push(' ');
                    }
                }
                text_buffer.clear();
                in_tag = true;
                tag_buffer.clear();
            }
            '>' => {
                in_tag = false;
                let tag = tag_buffer.trim().to_lowercase();

                // Handle block-level tags
                if tag.starts_with("script") { in_script = true; }
                else if tag.starts_with("/script") { in_script = false; }
                else if tag.starts_with("style") { in_style = true; }
                else if tag.starts_with("/style") { in_style = false; }
                else if tag.starts_with("br") || tag.starts_with("br/") {
                    result.push('\n');
                }
                else if tag.starts_with("p") || tag.starts_with("div") || tag.starts_with("section")
                    || tag.starts_with("article") || tag.starts_with("header") || tag.starts_with("footer")
                    || tag.starts_with("main")
                    || tag.starts_with("/p") || tag.starts_with("/div") || tag.starts_with("/section")
                    || tag.starts_with("/article") || tag.starts_with("/header") || tag.starts_with("/footer")
                    || tag.starts_with("/main") {
                    result.push_str("\n\n");
                }
                else if tag.starts_with("h1") { result.push_str("\n\n# "); }
                else if tag.starts_with("h2") { result.push_str("\n\n## "); }
                else if tag.starts_with("h3") { result.push_str("\n\n### "); }
                else if tag.starts_with("h4") { result.push_str("\n\n#### "); }
                else if tag.starts_with("h5") { result.push_str("\n\n##### "); }
                else if tag.starts_with("h6") { result.push_str("\n\n###### "); }
                else if tag.starts_with("li") { result.push_str("\n- "); }
                else if tag.starts_with("tr") { result.push('\n'); }
                else if tag.starts_with("td") || tag.starts_with("th") { result.push_str(" | "); }
                else if tag.starts_with("code") || tag.starts_with("/code") { result.push('`'); }
                else if tag.starts_with("pre") || tag.starts_with("/pre") { result.push_str("\n```\n"); }
                else if tag.len() > 1 && tag.starts_with('a') && (tag.len() == 1 || tag.as_bytes()[1] == b' ' || tag.as_bytes()[1] == b'\t') {
                    result.push('[');
                }
                else if tag == "/a" {
                    result.push(']');
                }
                else if tag.starts_with("img") {
                    if let Some(alt) = extract_alt(&tag_buffer) {
                        result.push_str(&format!("[{}]", alt));
                    }
                }

                tag_buffer.clear();
            }
            _ if in_tag => tag_buffer.push(ch),
            _ if !in_script && !in_style => text_buffer.push(ch),
            _ => {}
        }
    }

    // Flush remaining text
    if !text_buffer.is_empty() && !in_script && !in_style {
        let text = text_buffer.trim();
        if !text.is_empty() {
            result.push_str(text);
        }
    }

    // Clean up excessive whitespace
    let mut cleaned = String::new();
    let mut prev_was_newline = false;
    let mut consecutive_newlines = 0;

    for ch in result.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                cleaned.push(ch);
            }
            prev_was_newline = true;
        } else if ch.is_whitespace() && prev_was_newline {
            // Skip leading whitespace after newline
        } else {
            consecutive_newlines = 0;
            prev_was_newline = false;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

fn extract_alt(tag: &str) -> Option<String> {
    let start = tag.find("alt=\"")? + 5;
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

// ---------------------------------------------------------------------------
// WebSearchTool — DuckDuckGo HTML search
// ---------------------------------------------------------------------------

pub struct WebSearchTool {
    client: reqwest::Client,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_search".into(),
            description: "Search the web using DuckDuckGo. Returns search result titles, URLs, and snippets."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results to return (default: 5)" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default: 15, max: 60)" }
                },
                "required": ["query"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let query = args["query"].as_str().unwrap_or("");
        let max_results = args["max_results"].as_u64().unwrap_or(5) as usize;

        if query.is_empty() {
            return Err(maix_core::MaixError::Tool("web_search: query is required".into()));
        }
        if query.len() > 1000 {
            return Err(maix_core::MaixError::Tool("web_search: query too long (max 1KB)".into()));
        }
        if max_results == 0 || max_results > 20 {
            return Err(maix_core::MaixError::Tool("web_search: max_results must be 1-20".into()));
        }

        let timeout_secs = args["timeout"].as_u64().unwrap_or(15).min(60);

        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let resp = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_search: {e}")))?;

        let html = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_search read: {e}")))?;

        let results = parse_ddg_results(&html, max_results);

        if results.is_empty() {
            Ok("No search results found.".to_string())
        } else {
            let mut output = String::new();
            for (i, (title, url, snippet)) in results.iter().enumerate() {
                output.push_str(&format!("{}. {}\n   {}\n", i + 1, title, url));
                if !snippet.is_empty() {
                    output.push_str(&format!("   {}\n", snippet));
                }
                output.push('\n');
            }
            Ok(output)
        }
    }
}

/// Parse DuckDuckGo HTML search results.
fn parse_ddg_results(html: &str, max: usize) -> Vec<(String, String, String)> {
    let mut results = Vec::new();
    let mut pos = 0;

    while results.len() < max {
        let block_start = match html[pos..].find("class=\"result") {
            Some(i) => pos + i,
            None => break,
        };

        let block = &html[block_start..];
        let title = if let Some(a_start) = block.find("class=\"result__a\"") {
            let after_a = &block[a_start..];
            let href = extract_attr_from_html(after_a, "href").unwrap_or_default();
            let title_text = if let Some(gt) = after_a.find('>') {
                let after_gt = &after_a[gt + 1..];
                if let Some(end) = after_gt.find("</a") {
                    strip_html_tags(&after_gt[..end]).trim().to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            (title_text, href)
        } else {
            pos = block_start + 10;
            continue;
        };

        let snippet = if let Some(s_start) = block.find("class=\"result__snippet\"") {
            let after_s = &block[s_start..];
            if let Some(gt) = after_s.find('>') {
                let after_gt = &after_s[gt + 1..];
                if let Some(end) = after_gt.find("</a") {
                    strip_html_tags(&after_gt[..end]).trim().to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if !title.0.is_empty() && !title.1.is_empty() {
            results.push((title.0, title.1, snippet));
        }

        pos = block_start + 100;
    }

    results
}

fn extract_attr_from_html(html: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = html.find(&pattern)? + pattern.len();
    let end = html[start..].find('"')?;
    Some(html[start..start + end].to_string())
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

// ---------------------------------------------------------------------------
// HttpRequestTool
// ---------------------------------------------------------------------------

pub struct HttpRequestTool {
    client: reqwest::Client,
}

impl Default for HttpRequestTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpRequestTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for HttpRequestTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "http_request".into(),
            description: "Send an HTTP request with custom method and body".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Target URL" },
                    "method": { "type": "string", "description": "HTTP method (GET, POST, PUT, DELETE, PATCH)" },
                    "body": { "type": "string", "description": "Request body (optional)" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default: 30, max: 300)" }
                },
                "required": ["url"]
            }),
            risk_level: RiskLevel::Network,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> MaixResult<String> {
        let url = args["url"].as_str().unwrap_or("");
        let method = args["method"].as_str().unwrap_or("GET");
        let body = args["body"].as_str();

        if url.is_empty() {
            return Err(maix_core::MaixError::Tool("http_request: url is required".into()));
        }
        if url.len() > 4096 {
            return Err(maix_core::MaixError::Tool("http_request: url too long (max 4KB)".into()));
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(maix_core::MaixError::Tool("http_request: url must start with http:// or https://".into()));
        }
        if let Some(b) = body {
            if b.len() > 1_000_000 {
                return Err(maix_core::MaixError::Tool("http_request: body too large (max 1MB)".into()));
            }
        }

        let timeout_secs = args["timeout"].as_u64().unwrap_or(30).min(300);

        let mut req = match method.to_uppercase().as_str() {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            "PATCH" => self.client.patch(url),
            _ => return Err(maix_core::MaixError::Tool(format!("unsupported method: {method}"))),
        };

        if let Some(b) = body {
            req = req.body(b.to_string());
        }

        req = req.timeout(std::time::Duration::from_secs(timeout_secs));

        let resp = req.send().await
            .map_err(|e| maix_core::MaixError::Tool(format!("http_request: {e}")))?;

        let status = resp.status();
        let text = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("http_request read: {e}")))?;

        let truncated = &text[..text.len().min(4000)];
        Ok(format!("HTTP {status}\n\n{truncated}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_markdown_basic() {
        let html = "<html><body><h1>Title</h1><p>Paragraph text</p></body></html>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("Paragraph text"));
    }

    #[test]
    fn test_html_to_markdown_links() {
        let html = r#"<a href="https://example.com">Click here</a>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Click here"), "expected '[Click here' in: '{}'", md);
    }

    #[test]
    fn test_html_to_markdown_lists() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let md = html_to_markdown(html);
        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<b>bold</b>"), "bold");
        assert_eq!(strip_html_tags("no tags"), "no tags");
    }

    #[test]
    fn test_parse_ddg_empty() {
        let results = parse_ddg_results("<html>no results</html>", 5);
        assert!(results.is_empty());
    }
}
