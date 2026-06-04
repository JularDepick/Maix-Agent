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

        let mut body = serde_json::json!({
            "chat_id": msg.chat_id,
            "text": msg.text,
        });
        if !parse_mode.is_empty() {
            body["parse_mode"] = serde_json::json!(parse_mode);
        }
        if let Some(ref reply_to) = msg.reply_to {
            body["reply_to_message_id"] = serde_json::json!(reply_to);
        }

        tracing::info!("Telegram send to {}: {} chars", msg.chat_id, msg.text.len());

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Telegram HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Telegram API error {}: {}", status, text));
        }

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

    #[test]
    fn test_telegram_with_api_base() {
        let bridge = TelegramBridge::new("token").with_api_base("https://custom.api");
        assert_eq!(bridge.api_base, "https://custom.api");
    }

    #[test]
    fn test_telegram_verify_signature_always_true() {
        let bridge = TelegramBridge::new("token");
        assert!(bridge.verify_signature(&[], b"body"));
    }

    #[test]
    fn test_telegram_parse_edited_message() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{
            "edited_message": {
                "message_id": 42,
                "from": {"id": 100},
                "chat": {"id": 200},
                "date": 1700000000,
                "text": "edited text"
            }
        }"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.text, "edited text");
        assert_eq!(msg.message_id, "42");
    }

    #[test]
    fn test_telegram_parse_document() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{
            "message": {
                "message_id": 1,
                "from": {"id": 100},
                "chat": {"id": 200},
                "date": 1700000000,
                "text": "",
                "document": {"file_id": "doc123", "file_name": "report.pdf"}
            }
        }"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].attachment_type, "document");
        assert_eq!(msg.attachments[0].filename.as_deref(), Some("report.pdf"));
    }

    #[test]
    fn test_telegram_format_html() {
        let bridge = TelegramBridge::new("test-token");
        let msg = OutgoingMessage {
            chat_id: "123".into(),
            text: "bold".into(),
            reply_to: None,
            format: MessageFormat::Html,
        };
        assert_eq!(bridge.format_response(&msg), "bold");
    }

    #[test]
    fn test_telegram_format_plain() {
        let bridge = TelegramBridge::new("test-token");
        let msg = OutgoingMessage {
            chat_id: "123".into(),
            text: "plain text".into(),
            reply_to: None,
            format: MessageFormat::Plain,
        };
        assert_eq!(bridge.format_response(&msg), "plain text");
    }

    #[test]
    fn test_telegram_parse_invalid_json() {
        let bridge = TelegramBridge::new("test-token");
        assert!(bridge.parse_webhook("not json at all").is_err());
    }

    #[test]
    fn test_telegram_parse_missing_from() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{"message":{"message_id":1,"chat":{"id":20},"date":1700000000,"text":"hi"}}"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.user_id, ""); // defaults to empty
        assert_eq!(msg.text, "hi");
    }

    #[test]
    fn test_telegram_parse_missing_text() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000}}"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.text, ""); // defaults to empty
    }

    #[test]
    fn test_telegram_parse_empty_photo() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"","photo":[]}}"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_telegram_parse_document_no_filename() {
        let bridge = TelegramBridge::new("test-token");
        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"","document":{"file_id":"doc1"}}}"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        assert!(msg.attachments[0].filename.is_none());
    }

    #[test]
    fn test_telegram_photo_url_format() {
        let bridge = TelegramBridge::new("bot-token").with_api_base("https://api.telegram.org");
        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"","photo":[{"file_id":"photo123","width":90,"height":90}]}}"#;
        let msg = bridge.parse_webhook(body).unwrap();
        assert_eq!(msg.attachments.len(), 1);
        let url = msg.attachments[0].url.as_deref().unwrap();
        assert!(url.contains("photo123"));
        assert!(url.contains("bot-token"));
    }
}
