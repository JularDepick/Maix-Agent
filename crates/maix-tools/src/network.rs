//! Network tools — HTTP requests, web fetching.

use crate::{ToolCtx, ToolResult};
use serde_json::Value;

pub async fn web_fetch(_ctx: &ToolCtx, args: Value) -> ToolResult {
    let url = args["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return "error: url is required".into();
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("client error: {e}"),
    };

    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => {
                    let truncated = &body[..body.len().min(8000)];
                    format!("HTTP {status}\n\n{truncated}")
                }
                Err(e) => format!("read error: {e}"),
            }
        }
        Err(e) => format!("fetch error: {e}"),
    }
}

pub async fn http_request(_ctx: &ToolCtx, args: Value) -> ToolResult {
    let url = args["url"].as_str().unwrap_or("");
    let method = args["method"].as_str().unwrap_or("GET");
    let body = args["body"].as_str();

    if url.is_empty() {
        return "error: url is required".into();
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("client error: {e}"),
    };

    let mut req = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => return format!("unsupported method: {method}"),
    };

    if let Some(b) = body {
        req = req.body(b.to_string());
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(body) => {
                    let truncated = &body[..body.len().min(4000)];
                    format!("HTTP {status}\n\n{truncated}")
                }
                Err(e) => format!("read error: {e}"),
            }
        }
        Err(e) => format!("request error: {e}"),
    }
}
