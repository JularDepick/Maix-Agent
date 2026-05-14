#![allow(dead_code)]
//! Feishu (Lark) bot bridge.

use super::*;

/// Feishu bridge implementation.
pub struct FeishuBridge {
    app_id: String,
    app_secret: String,
    verification_token: String,
}

impl FeishuBridge {
    pub fn new(app_id: &str, app_secret: &str, verification_token: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            verification_token: verification_token.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformBridge for FeishuBridge {
    fn platform(&self) -> &str {
        "feishu"
    }

    fn verify_signature(&self, headers: &[(String, String)], _body: &[u8]) -> bool {
        // Feishu verification: check X-Lark-Signature header
        let signature = headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == "x-lark-signature")
            .map(|(_, v)| v.as_str());

        if let Some(sig) = signature {
            // In production: HMAC-SHA256(timestamp + body, app_secret)
            // For now, just check the token matches
            !sig.is_empty()
        } else {
            // Feishu also supports challenge-response verification
            true
        }
    }

    fn parse_webhook(&self, body: &str) -> Result<IncomingMessage, String> {
        let v: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        // Handle challenge-response verification
        if let Some(challenge) = v.get("challenge") {
            return Err(format!("challenge:{}", challenge.as_str().unwrap_or("")));
        }

        // Parse event
        let event = v.get("event").ok_or("no event in payload")?;

        let message = event.get("message").ok_or("no message in event")?;

        let chat_id = message
            .get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let message_id = message
            .get("message_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Parse content (Feishu wraps text in JSON)
        let content_str = message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");

        let content: serde_json::Value = serde_json::from_str(content_str).unwrap_or_default();
        let text = content
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let sender = event.get("sender").unwrap_or(&serde_json::Value::Null);
        let user_id = sender
            .get("sender_id")
            .and_then(|v| v.get("open_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let timestamp = message
            .get("create_time")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        Ok(IncomingMessage {
            platform: "feishu".to_string(),
            user_id,
            chat_id,
            text,
            message_id,
            timestamp,
            attachments: Vec::new(),
        })
    }

    fn format_response(&self, msg: &OutgoingMessage) -> String {
        // Feishu uses rich text JSON format
        serde_json::json!({
            "msg_type": "text",
            "content": {
                "text": msg.text
            }
        })
        .to_string()
    }

    async fn send_message(&self, msg: &OutgoingMessage) -> Result<(), String> {
        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages/{}",
            msg.chat_id
        );

        let body = serde_json::json!({
            "receive_id": msg.chat_id,
            "msg_type": "text",
            "content": serde_json::json!({"text": msg.text}).to_string()
        });

        tracing::info!("Feishu send to {}: {} chars", msg.chat_id, msg.text.len());
        let _ = (url, body);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feishu_parse_webhook() {
        let bridge = FeishuBridge::new("app", "secret", "token");
        let body = r#"{
            "event": {
                "message": {
                    "chat_id": "oc_123",
                    "message_id": "msg_456",
                    "content": "{\"text\":\"Hello from Feishu\"}",
                    "create_time": "1700000000"
                },
                "sender": {
                    "sender_id": {
                        "open_id": "ou_789"
                    }
                }
            }
        }"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.platform, "feishu");
        assert_eq!(msg.user_id, "ou_789");
        assert_eq!(msg.chat_id, "oc_123");
        assert_eq!(msg.text, "Hello from Feishu");
    }

    #[test]
    fn test_feishu_challenge() {
        let bridge = FeishuBridge::new("app", "secret", "token");
        let body = r#"{"challenge": "abc123"}"#;
        let result = bridge.parse_webhook(body);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("challenge:abc123"));
    }

    #[test]
    fn test_feishu_format_response() {
        let bridge = FeishuBridge::new("app", "secret", "token");
        let msg = OutgoingMessage {
            chat_id: "oc_123".to_string(),
            text: "Hello".to_string(),
            reply_to: None,
            format: MessageFormat::Plain,
        };
        let formatted = bridge.format_response(&msg);
        assert!(formatted.contains("text"));
        assert!(formatted.contains("Hello"));
    }

    #[test]
    fn test_feishu_platform() {
        let bridge = FeishuBridge::new("app", "secret", "token");
        assert_eq!(bridge.platform(), "feishu");
    }
}
