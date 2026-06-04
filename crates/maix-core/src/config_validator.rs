//! Configuration validation — friendly error messages for config issues.

/// A configuration error with fix hint.
#[derive(Debug, Clone)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
    pub fix: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error in '{}': {}\n  Fix: {}", self.field, self.message, self.fix)
    }
}

/// Default known model prefixes.
pub const DEFAULT_MODEL_PREFIXES: &[&str] = &[
    "gpt-", "claude-", "deepseek", "gemini", "auto",
    "qwen", "llama", "mistral", "mixtral", "phi-",
    "command-r", "yi-", "internlm", "glm-", "baichuan",
    "moonshot", "doubao", "spark", "hunyuan", "minimax",
];

/// Validates configuration values.
pub struct ConfigValidator {
    /// Known model prefixes (configurable).
    known_model_prefixes: Vec<String>,
}

impl ConfigValidator {
    /// Create a validator with default known model prefixes.
    pub fn new() -> Self {
        Self {
            known_model_prefixes: DEFAULT_MODEL_PREFIXES.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Create a validator with custom known model prefixes.
    pub fn with_model_prefixes(prefixes: Vec<String>) -> Self {
        Self {
            known_model_prefixes: prefixes,
        }
    }

    /// Add additional model prefixes to the known list.
    pub fn add_model_prefixes(mut self, prefixes: &[&str]) -> Self {
        for p in prefixes {
            if !self.known_model_prefixes.contains(&p.to_string()) {
                self.known_model_prefixes.push(p.to_string());
            }
        }
        self
    }

    /// Validate a TOML config string.
    pub fn validate_toml(content: &str) -> Result<toml::Value, Vec<ConfigError>> {
        toml::from_str(content).map_err(|e| {
            vec![ConfigError {
                field: "syntax".into(),
                message: format!("TOML parse error: {}", e),
                fix: "Check TOML syntax — missing quotes, brackets, or commas".into(),
            }]
        })
    }

    /// Validate required fields in a parsed TOML value.
    pub fn validate_fields(&self, config: &toml::Value) -> Vec<ConfigError> {
        let mut errors = Vec::new();

        // Check [provider] section
        if let Some(provider) = config.get("provider") {
            if provider.get("name").is_none() {
                errors.push(ConfigError {
                    field: "provider.name".into(),
                    message: "Provider name is required".into(),
                    fix: r#"Add: [provider]
name = "openai""#
                        .into(),
                });
            }
        } else {
            errors.push(ConfigError {
                field: "provider".into(),
                message: "[provider] section is missing".into(),
                fix: "Add a [provider] section to your config".into(),
            });
        }

        // Check model field
        if let Some(model) = config.get("model").and_then(|m| m.as_str()) {
            if !self.known_model_prefixes.iter().any(|p| model.starts_with(p.as_str())) {
                errors.push(ConfigError {
                    field: "model".into(),
                    message: format!("Unknown model: '{}'", model),
                    fix: format!(
                        "Known model prefixes: {}",
                        self.known_model_prefixes.join(", ")
                    ),
                });
            }
        }

        // Check api_key is not empty
        if let Some(key) = config.get("api_key").and_then(|k| k.as_str()) {
            if key.is_empty() {
                errors.push(ConfigError {
                    field: "api_key".into(),
                    message: "API key is empty".into(),
                    fix: "Set api_key in config or MAIX_API_KEY env var".into(),
                });
            }
        }

        errors
    }

    /// Validate a config file and return all errors.
    pub fn validate_file(&self, path: &std::path::Path) -> Result<toml::Value, Vec<ConfigError>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            vec![ConfigError {
                field: "file".into(),
                message: format!("Cannot read config: {}", e),
                fix: format!("Check if {} exists and is readable", path.display()),
            }]
        })?;

        let config = Self::validate_toml(&content)?;
        let field_errors = self.validate_fields(&config);
        if field_errors.is_empty() {
            Ok(config)
        } else {
            Err(field_errors)
        }
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_toml_valid() {
        let toml = r#"
[provider]
name = "openai"
model = "gpt-4o"
"#;
        assert!(ConfigValidator::validate_toml(toml).is_ok());
    }

    #[test]
    fn test_validate_toml_invalid() {
        let toml = "this is not valid toml [[[";
        assert!(ConfigValidator::validate_toml(toml).is_err());
    }

    #[test]
    fn test_validate_missing_provider() {
        let validator = ConfigValidator::new();
        let config: toml::Value = toml::from_str("model = 'gpt-4o'").unwrap();
        let errors = validator.validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "provider"));
    }

    #[test]
    fn test_validate_missing_provider_name() {
        let validator = ConfigValidator::new();
        let config: toml::Value = toml::from_str("[provider]\napi_base = 'test'").unwrap();
        let errors = validator.validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "provider.name"));
    }

    #[test]
    fn test_validate_unknown_model() {
        let validator = ConfigValidator::new();
        let config: toml::Value =
            toml::from_str("model = 'foobar-xyz'\n[provider]\nname = 'openai'")
                .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "model"));
    }

    #[test]
    fn test_validate_valid_config() {
        let validator = ConfigValidator::new();
        let config: toml::Value = toml::from_str(
            r#"
[provider]
name = "openai"

[model]
name = "gpt-4o"

api_key = "sk-test"
"#,
        )
        .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(!errors.iter().any(|e| e.field == "provider"));
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError {
            field: "test".into(),
            message: "msg".into(),
            fix: "fix".into(),
        };
        let display = format!("{}", err);
        assert!(display.contains("test"));
        assert!(display.contains("msg"));
        assert!(display.contains("fix"));
    }

    #[test]
    fn test_custom_model_prefixes() {
        let validator = ConfigValidator::with_model_prefixes(vec!["my-custom-".into()]);
        let config: toml::Value = toml::from_str(
            r#"[provider]
name = "custom"
model = "my-custom-v1"
api_key = "k"
"#,
        )
        .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(!errors.iter().any(|e| e.field == "model"));
    }

    #[test]
    fn test_custom_prefix_rejects_unknown() {
        let validator = ConfigValidator::with_model_prefixes(vec!["my-custom-".into()]);
        let config: toml::Value = toml::from_str(
            r#"
model = "gpt-4o"
[provider]
name = "custom"
api_key = "k"
"#,
        )
        .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "model"));
    }

    #[test]
    fn test_add_model_prefixes() {
        let validator = ConfigValidator::new().add_model_prefixes(&["custom-", "other-"]);
        let config: toml::Value = toml::from_str(
            r#"
model = "custom-v1"
[provider]
name = "custom"
api_key = "k"
"#,
        )
        .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(!errors.iter().any(|e| e.field == "model"));
    }

    #[test]
    fn test_default_prefixes_include_common() {
        let validator = ConfigValidator::new();
        let config: toml::Value = toml::from_str(
            r#"
model = "qwen-turbo"
[provider]
name = "openai"
api_key = "k"
"#,
        )
        .unwrap();
        let errors = validator.validate_fields(&config);
        assert!(!errors.iter().any(|e| e.field == "model"));
    }
}
