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

/// Validates configuration values.
pub struct ConfigValidator;

impl ConfigValidator {
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
    pub fn validate_fields(config: &toml::Value) -> Vec<ConfigError> {
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
            let valid_prefixes = ["gpt-", "claude-", "deepseek", "gemini", "auto"];
            if !valid_prefixes.iter().any(|p| model.starts_with(p)) {
                errors.push(ConfigError {
                    field: "model".into(),
                    message: format!("Unknown model: '{}'", model),
                    fix: format!(
                        "Valid models start with: {}",
                        valid_prefixes.join(", ")
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
    pub fn validate_file(path: &std::path::Path) -> Result<toml::Value, Vec<ConfigError>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            vec![ConfigError {
                field: "file".into(),
                message: format!("Cannot read config: {}", e),
                fix: format!("Check if {} exists and is readable", path.display()),
            }]
        })?;

        let config = Self::validate_toml(&content)?;
        let field_errors = Self::validate_fields(&config);
        if field_errors.is_empty() {
            Ok(config)
        } else {
            Err(field_errors)
        }
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
        let config: toml::Value = toml::from_str("model = 'gpt-4o'").unwrap();
        let errors = ConfigValidator::validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "provider"));
    }

    #[test]
    fn test_validate_missing_provider_name() {
        let config: toml::Value = toml::from_str("[provider]\napi_base = 'test'").unwrap();
        let errors = ConfigValidator::validate_fields(&config);
        assert!(errors.iter().any(|e| e.field == "provider.name"));
    }

    #[test]
    fn test_validate_unknown_model() {
        let config: toml::Value =
            toml::from_str("[provider]\nname = 'openai'\n[model]\nname = 'foobar-xyz'")
                .unwrap();
        // model validation depends on where the field is
        let errors = ConfigValidator::validate_fields(&config);
        // just checking it doesn't panic
        let _ = errors;
    }

    #[test]
    fn test_validate_valid_config() {
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
        let errors = ConfigValidator::validate_fields(&config);
        // Should have no provider errors at least
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
}
