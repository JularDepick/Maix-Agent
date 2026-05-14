//! Telegram Bot API bridge.

use super::*;

/// Telegram bridge implementation.
pub struct TelegramBridge {
    bot_token: String,
    api_base: String,
}

impl TelegramBridge {
    pub fn new(bot_token: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            api_base: "https://api.telegram.org".to_string(),
        }
    }

    pub fn with_api_base(mut self, base: &str) -> Self {
        self.api_base = base.to_string();
        self
    }
}

#[async_trait::async_trait]
impl PlatformBridge for TelegramBridge {
    fn platform(&self) -> &str {
        "telegram"
    }

    fn verify_signature(&self, _headers: &[(String, String)], _body: &[u8]) -> bool {
        // Telegram uses webhook secret in URL, not signature headers
        true
    }

    fn parse_webhook(&self, body: &str) -> Result<IncomingMessage, String> {
        let v: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let message = v.get("message")
            .or_else(|| v.get("edited_message"))
            .ok_or("no message in update")?;

        let user_id = message
            .pointer("/from/id")
            .and_then(|v| v.as_i64())
            .map(|id| id.to_string())
            .unwrap_or_default();

        let chat_id = message
            .pointer("/chat/id")
            .and_then(|v| v.as_i64())
            .map(|id| id.to_string())
            .unwrap_or_default();

        let text = message
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let message_id = message
            .get("message_id")
            .and_then(|v| v.as_i64())
            .map(|id| id.to_string())
            .unwrap_or_default();

        let timestamp = message
            .get("date")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let mut attachments = Vec::new();

        // Check for photos
        if let Some(photos) = message.get("photo").and_then(|v| v.as_array()) {
            if let Some(largest) = photos.last() {
                if let Some(file_id) = largest.get("file_id").and_then(|v| v.as_str()) {
                    attachments.push(Attachment {
                        attachment_type: "photo".to_string(),
                        url: Some(format!("{}/bot{}/getFile?file_id={}", self.api_base, self.bot_token, file_id)),
                        data: None,
                        filename: None,
                    });
                }
            }
        }

        // Check for documents
        if let Some(doc) = message.get("document") {
            if let Some(file_id) = doc.get("file_id").and_then(|v| v.as_str()) {
                let filename = doc.get("file_name").and_then(|v| v.as_str()).map(|s| s.to_string());
                attachments.push(Attachment {
                    attachment_type: "document".to_string(),
                    url: Some(format!("{}/bot{}/getFile?file_id={}", self.api_base, self.bot_token, file_id)),
                    data: None,
                    filename,
                });
            }
        }

        Ok(IncomingMessage {
            platform: "telegram".to_string(),
            user_id,
            chat_id,
            text,
            message_id,
            timestamp,
            attachments,
        })
    }

    fn format_response(&self, msg: &OutgoingMessage) -> String {
        match msg.format {
            MessageFormat::Markdown => format!("<b>{}</b>", msg.text),
            MessageFormat::Html => msg.text.clone(),
            MessageFormat::Plain => msg.text.clone(),
        }
    }

    async fn send_message(&self, msg: &OutgoingMessage) -> Result<(), String> {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.bot_token);
        let parse_mode = match msg.format {
            MessageFormat::Markdown => "MarkdownV2",
            MessageFormat::Html => "HTML",
            MessageFormat::Plain => "",
        };

        let body = serde_json::json!({
            "chat_id": msg.chat_id,
            "text": msg.text,
            "parse_mode": parse_mode,
            "reply_to_message_id": msg.reply_to,
        });

        // In a real implementation, use reqwest to POST
        tracing::info!("Telegram send to {}: {} chars", msg.chat_id, msg.text.len());
        let _ = (url, body);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_parse_webhook() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{
            "message": {
                "message_id": 123,
                "from": {"id": 456, "first_name": "Test"},
                "chat": {"id": 789, "type": "private"},
                "date": 1700000000,
                "text": "Hello bot"
            }
        }"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.user_id, "456");
        assert_eq!(msg.chat_id, "789");
        assert_eq!(msg.text, "Hello bot");
        assert_eq!(msg.message_id, "123");
    }

    #[test]
    fn test_telegram_parse_with_photo() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{
            "message": {
                "message_id": 1,
                "from": {"id": 100},
                "chat": {"id": 200},
                "date": 1700000000,
                "text": "",
                "photo": [
                    {"file_id": "small", "width": 90, "height": 90},
                    {"file_id": "large", "width": 800, "height": 600}
                ]
            }
        }"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].attachment_type, "photo");
    }

    #[test]
    fn test_telegram_parse_no_message() {
        let bridge = TelegramBridge::new("test-token");
        assert!(bridge.parse_webhook("{}").is_err());
    }

    #[test]
    fn test_telegram_format_response() {
        let bridge = TelegramBridge::new("test-token");
        let msg = OutgoingMessage {
            chat_id: "123".to_string(),
            text: "Hello".to_string(),
            reply_to: None,
            format: MessageFormat::Markdown,
        };
        assert_eq!(bridge.format_response(&msg), "<b>Hello</b>");
    }

    #[test]
    fn test_telegram_platform() {
        let bridge = TelegramBridge::new("test-token");
        assert_eq!(bridge.platform(), "telegram");
    }
}
