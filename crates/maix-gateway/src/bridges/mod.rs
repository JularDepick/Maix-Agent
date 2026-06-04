//! Platform bridges — adapters for IM platforms (Telegram, Feishu, WeChat).
//!
//! Each bridge implements the `PlatformBridge` trait to translate
//! platform-specific webhooks into Maix messages and vice versa.

pub mod telegram;
pub mod feishu;

use serde::{Deserialize, Serialize};

/// A message from an IM platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub platform: String,
    pub user_id: String,
    pub chat_id: String,
    pub text: String,
    pub message_id: String,
    pub timestamp: i64,
    pub attachments: Vec<Attachment>,
}

/// An attachment (image, file, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub attachment_type: String,
    pub url: Option<String>,
    pub data: Option<Vec<u8>>,
    pub filename: Option<String>,
}

/// A response to send back to the IM platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub chat_id: String,
    pub text: String,
    pub reply_to: Option<String>,
    pub format: MessageFormat,
}

/// Message format for outgoing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageFormat {
    Plain,
    Markdown,
    Html,
}

/// Platform bridge trait — translates between IM platforms and Maix.
#[async_trait::async_trait]
pub trait PlatformBridge: Send + Sync {
    /// Platform name (e.g., "telegram", "feishu").
    fn platform(&self) -> &str;

    /// Verify webhook signature.
    fn verify_signature(&self, headers: &[(String, String)], body: &[u8]) -> bool;

    /// Parse an incoming webhook payload into a message.
    fn parse_webhook(&self, body: &str) -> Result<IncomingMessage, String>;

    /// Format a response for the platform.
    fn format_response(&self, msg: &OutgoingMessage) -> String;

    /// Send a message to the platform (requires HTTP client).
    async fn send_message(&self, msg: &OutgoingMessage) -> Result<(), String>;
}

/// User whitelist for access control.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserWhitelist {
    pub allowed_users: Vec<String>,
    pub allowed_chats: Vec<String>,
    pub admin_users: Vec<String>,
}

impl UserWhitelist {
    pub fn is_user_allowed(&self, user_id: &str) -> bool {
        self.allowed_users.is_empty() || self.allowed_users.contains(&user_id.to_string())
    }

    pub fn is_chat_allowed(&self, chat_id: &str) -> bool {
        self.allowed_chats.is_empty() || self.allowed_chats.contains(&chat_id.to_string())
    }

    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_users.contains(&user_id.to_string())
    }
}

/// Bridge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub platform: String,
    pub enabled: bool,
    pub webhook_secret: Option<String>,
    pub api_token: Option<String>,
    pub api_base: Option<String>,
    pub whitelist: UserWhitelist,
}

/// Bridge manager — holds all configured bridges.
pub struct BridgeManager {
    bridges: Vec<(BridgeConfig, Box<dyn PlatformBridge>)>,
}

impl BridgeManager {
    pub fn new() -> Self {
        Self {
            bridges: Vec::new(),
        }
    }

    pub fn register(&mut self, config: BridgeConfig, bridge: Box<dyn PlatformBridge>) {
        self.bridges.push((config, bridge));
    }

    pub fn handle_webhook(&self, platform: &str, headers: &[(String, String)], body: &str) -> Result<IncomingMessage, String> {
        for (config, bridge) in &self.bridges {
            if bridge.platform() == platform && config.enabled {
                if !bridge.verify_signature(headers, body.as_bytes()) {
                    return Err("signature verification failed".to_string());
                }
                let msg = bridge.parse_webhook(body)?;
                if !config.whitelist.is_user_allowed(&msg.user_id) {
                    return Err(format!("user {} not allowed", msg.user_id));
                }
                if !config.whitelist.is_chat_allowed(&msg.chat_id) {
                    return Err(format!("chat {} not allowed", msg.chat_id));
                }
                return Ok(msg);
            }
        }
        Err(format!("no bridge for platform '{}'", platform))
    }

    pub fn platforms(&self) -> Vec<&str> {
        self.bridges.iter().map(|(_, b)| b.platform()).collect()
    }

    pub fn enabled_platforms(&self) -> Vec<&str> {
        self.bridges
            .iter()
            .filter(|(c, _)| c.enabled)
            .map(|(_, b)| b.platform())
            .collect()
    }
}

impl Default for BridgeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitelist_empty_allows_all() {
        let wl = UserWhitelist::default();
        assert!(wl.is_user_allowed("anyone"));
        assert!(wl.is_chat_allowed("any-chat"));
    }

    #[test]
    fn test_whitelist_restrictive() {
        let wl = UserWhitelist {
            allowed_users: vec!["user1".to_string(), "user2".to_string()],
            allowed_chats: vec!["chat-a".to_string()],
            admin_users: vec!["user1".to_string()],
        };
        assert!(wl.is_user_allowed("user1"));
        assert!(wl.is_user_allowed("user2"));
        assert!(!wl.is_user_allowed("user3"));
        assert!(wl.is_chat_allowed("chat-a"));
        assert!(!wl.is_chat_allowed("chat-b"));
        assert!(wl.is_admin("user1"));
        assert!(!wl.is_admin("user2"));
    }

    #[test]
    fn test_bridge_manager_platforms() {
        let mgr = BridgeManager::new();
        assert!(mgr.platforms().is_empty());
        assert!(mgr.enabled_platforms().is_empty());
    }

    #[test]
    fn test_message_format() {
        assert_eq!(MessageFormat::Plain, MessageFormat::Plain);
        assert_ne!(MessageFormat::Plain, MessageFormat::Markdown);
    }

    #[test]
    fn test_bridge_manager_with_bridges() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: true,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));

        assert_eq!(mgr.platforms(), vec!["telegram"]);
        assert_eq!(mgr.enabled_platforms(), vec!["telegram"]);
    }

    #[test]
    fn test_bridge_manager_disabled_platform() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: false,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));

        assert_eq!(mgr.platforms(), vec!["telegram"]);
        assert!(mgr.enabled_platforms().is_empty());
    }

    #[test]
    fn test_bridge_manager_handle_webhook() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: true,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));

        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"hi"}}"#;
        let msg = mgr.handle_webhook("telegram", &[], body).unwrap();
        assert_eq!(msg.user_id, "10");
        assert_eq!(msg.text, "hi");
    }

    #[test]
    fn test_bridge_manager_handle_webhook_user_blocked() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: true,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist {
                allowed_users: vec!["999".into()],
                allowed_chats: vec![],
                admin_users: vec![],
            },
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));

        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"hi"}}"#;
        let result = mgr.handle_webhook("telegram", &[], body);
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn test_bridge_manager_unknown_platform() {
        let mgr = BridgeManager::new();
        let result = mgr.handle_webhook("discord", &[], "{}");
        assert!(result.unwrap_err().contains("no bridge"));
    }

    #[test]
    fn test_whitelist_empty_string_rejected() {
        let wl = UserWhitelist {
            allowed_users: vec!["user1".into()],
            allowed_chats: vec!["chat1".into()],
            admin_users: vec!["admin1".into()],
        };
        assert!(!wl.is_user_allowed(""));
        assert!(!wl.is_chat_allowed(""));
        assert!(!wl.is_admin(""));
    }

    #[test]
    fn test_whitelist_empty_admin_list() {
        let wl = UserWhitelist {
            allowed_users: vec!["user1".into()],
            allowed_chats: vec![],
            admin_users: vec![],
        };
        assert!(!wl.is_admin("user1"));
        assert!(!wl.is_admin(""));
    }

    #[test]
    fn test_handle_webhook_chat_blocked() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: true,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist {
                allowed_users: vec![],
                allowed_chats: vec!["999".into()],
                admin_users: vec![],
            },
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));
        let body = r#"{"message":{"message_id":1,"from":{"id":10},"chat":{"id":20},"date":1700000000,"text":"hi"}}"#;
        let result = mgr.handle_webhook("telegram", &[], body);
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn test_handle_webhook_disabled_bridge() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: false,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));
        let result = mgr.handle_webhook("telegram", &[], "{}");
        assert!(result.unwrap_err().contains("no bridge"));
    }

    #[test]
    fn test_handle_webhook_invalid_json() {
        let mut mgr = BridgeManager::new();
        let config = BridgeConfig {
            platform: "telegram".into(),
            enabled: true,
            webhook_secret: None,
            api_token: None,
            api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(config, Box::new(telegram::TelegramBridge::new("token")));
        let result = mgr.handle_webhook("telegram", &[], "not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_platforms_multiple() {
        let mut mgr = BridgeManager::new();
        let tg_config = BridgeConfig {
            platform: "telegram".into(), enabled: true,
            webhook_secret: None, api_token: None, api_base: None,
            whitelist: UserWhitelist::default(),
        };
        let fs_config = BridgeConfig {
            platform: "feishu".into(), enabled: true,
            webhook_secret: None, api_token: None, api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(tg_config, Box::new(telegram::TelegramBridge::new("t")));
        mgr.register(fs_config, Box::new(feishu::FeishuBridge::new("a", "s", "v")));
        assert_eq!(mgr.platforms().len(), 2);
        assert_eq!(mgr.enabled_platforms().len(), 2);
    }

    #[test]
    fn test_enabled_platforms_mixed() {
        let mut mgr = BridgeManager::new();
        let tg_config = BridgeConfig {
            platform: "telegram".into(), enabled: true,
            webhook_secret: None, api_token: None, api_base: None,
            whitelist: UserWhitelist::default(),
        };
        let fs_config = BridgeConfig {
            platform: "feishu".into(), enabled: false,
            webhook_secret: None, api_token: None, api_base: None,
            whitelist: UserWhitelist::default(),
        };
        mgr.register(tg_config, Box::new(telegram::TelegramBridge::new("t")));
        mgr.register(fs_config, Box::new(feishu::FeishuBridge::new("a", "s", "v")));
        assert_eq!(mgr.platforms().len(), 2);
        assert_eq!(mgr.enabled_platforms(), vec!["telegram"]);
    }
}
