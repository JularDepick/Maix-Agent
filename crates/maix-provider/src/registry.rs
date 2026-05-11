use super::LLMProvider;
use maix_core::MaixResult;
use std::collections::HashMap;
use std::sync::Arc;

/// A registry of named LLM providers.
///
/// ```ignore
/// let mut reg = ProviderRegistry::new();
/// reg.insert("deepseek", deepseek_provider);
/// let p = reg.get("deepseek")?;
/// ```
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn LLMProvider>>,
    default: Option<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default: None,
        }
    }

    /// Register a named provider.
    pub fn insert(&mut self, name: &str, provider: Arc<dyn LLMProvider>) {
        self.providers.insert(name.to_string(), provider);
    }

    /// Set the default provider name.
    pub fn set_default(&mut self, name: &str) {
        self.default = Some(name.to_string());
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> MaixResult<Arc<dyn LLMProvider>> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| maix_core::MaixError::Provider(format!(
                "unknown provider: {name}. Available: {}",
                self.list_names().join(", ")
            )))
    }

    /// Get the default provider, or error if none set.
    pub fn default(&self) -> MaixResult<Arc<dyn LLMProvider>> {
        let name = self
            .default
            .as_deref()
            .ok_or_else(|| maix_core::MaixError::Provider(
                "no default provider set".into()
            ))?;
        self.get(name)
    }

    /// List all registered provider names.
    pub fn list_names(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OpenAICompatProvider;

    #[test]
    fn test_registry_insert_and_get() {
        let mut reg = ProviderRegistry::new();
        let provider = Arc::new(
            OpenAICompatProvider::new(
                "https://api.test.com".into(),
                "sk-test".into(),
                "test-model".into(),
            )
        );

        reg.insert("test", provider.clone());
        assert_eq!(reg.list_names(), vec!["test"]);

        let found = reg.get("test").unwrap();
        assert_eq!(found.model_name(), "test-model");
        assert_eq!(found.context_window(), 128_000);
    }

    #[test]
    fn test_registry_default() {
        let mut reg = ProviderRegistry::new();
        let provider = Arc::new(
            OpenAICompatProvider::new(
                "https://api.test.com".into(),
                "sk-test".into(),
                "test-model".into(),
            )
        );
        reg.insert("test", provider);
        reg.set_default("test");

        let default = reg.default().unwrap();
        assert_eq!(default.model_name(), "test-model");
    }

    #[test]
    fn test_registry_unknown_provider() {
        let reg = ProviderRegistry::new();
        assert!(reg.get("nonexistent").is_err());
        assert!(reg.default().is_err());
    }

    #[test]
    fn test_provider_builder() {
        let caps = crate::ProviderCapabilities {
            max_context: 1_000_000,
            supports_reasoning: true,
            supports_tool_use: true,
            supports_vision: false,
            supports_streaming: true,
            max_tool_calls_per_turn: 16,
        };

        let provider = OpenAICompatProvider::new(
            "https://api.deepseek.com".into(),
            "sk-test".into(),
            "deepseek-chat".into(),
        )
        .with_context_window(1_000_000)
        .with_reasoning()
        .with_capabilities(caps)
        .with_header("X-Custom", "value");

        assert_eq!(provider.model_name(), "deepseek-chat");
        assert_eq!(provider.context_window(), 1_000_000);
        assert!(provider.capabilities().supports_reasoning);
    }
}
