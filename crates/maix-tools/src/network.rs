//! Network tools — HTTP requests, web fetching, web search.

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
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_fetch".into(),
            description: "Fetch content from a URL. Returns raw HTML/text.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" }
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

        let resp = self.client
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_fetch: {e}")))?;

        let status = resp.status();
        let body = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_fetch read: {e}")))?;

        let truncated = &body[..body.len().min(8000)];
        Ok(format!("HTTP {status}\n\n{truncated}"))
    }
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
                    "max_results": { "type": "integer", "description": "Max results to return (default: 5)" }
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

        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let resp = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_search: {e}")))?;

        let html = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("web_search read: {e}")))?;

        // Parse DuckDuckGo HTML results
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

    // DuckDuckGo HTML results are in <div class="result ..."> blocks
    // Each has: <a class="result__a" href="...">title</a>
    //           <a class="result__snippet" href="...">snippet</a>

    let mut pos = 0;
    while results.len() < max {
        // Find next result block
        let block_start = match html[pos..].find("class=\"result") {
            Some(i) => pos + i,
            None => break,
        };

        // Find the result__a link (title + URL)
        let block = &html[block_start..];
        let title = if let Some(a_start) = block.find("class=\"result__a\"") {
            let after_a = &block[a_start..];
            // Find href
            let href = extract_attr(after_a, "href").unwrap_or_default();
            // Find title text between > and </a>
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

        // Find snippet
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

fn extract_attr(html: &str, attr: &str) -> Option<String> {
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
                    "body": { "type": "string", "description": "Request body (optional)" }
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

        let resp = req.send().await
            .map_err(|e| maix_core::MaixError::Tool(format!("http_request: {e}")))?;

        let status = resp.status();
        let text = resp.text().await
            .map_err(|e| maix_core::MaixError::Tool(format!("http_request read: {e}")))?;

        let truncated = &text[..text.len().min(4000)];
        Ok(format!("HTTP {status}\n\n{truncated}"))
    }
}
