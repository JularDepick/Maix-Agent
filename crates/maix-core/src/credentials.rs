//! API key & credential management — env, file, or system keyring.

use std::path::PathBuf;

/// Resolve an API key by checking (in order):
/// 1. Environment variable
/// 2. Credential file (JSON: `{"keys": {"provider_name": "sk-..."}}`)
pub fn resolve_api_key(provider: &str) -> Option<String> {
    let env_var = format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"));
    if let Ok(key) = std::env::var(&env_var) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Fallback: check credential file
    let cred_path = credentials_path()?;
    if cred_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&cred_path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(key) = parsed["keys"][provider].as_str() {
                    return Some(key.to_string());
                }
            }
        }
    }

    None
}

/// Resolve the base URL for a provider (env or credential file).
pub fn resolve_api_base(provider: &str) -> Option<String> {
    let env_var = format!("{}_API_BASE", provider.to_uppercase().replace('-', "_"));
    if let Ok(base) = std::env::var(&env_var) {
        if !base.is_empty() {
            return Some(base);
        }
    }

    let cred_path = credentials_path()?;
    if cred_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&cred_path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(base) = parsed["bases"][provider].as_str() {
                    return Some(base.to_string());
                }
            }
        }
    }

    None
}

/// Path to the credentials file: `$MAIX_HOME/credentials.json`
fn credentials_path() -> Option<PathBuf> {
    let home = std::env::var("MAIX_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            Some(
                dirs_fallback()
                    .join(".maix")
            )
        })?;
    Some(home.join("credentials.json"))
}

fn dirs_fallback() -> PathBuf {
    if cfg!(windows) {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

// Example credentials.json format:
// {
//   "keys": {
//     "provider-a": "sk-...",
//     "provider-b": "sk-..."
//   },
//   "bases": {
//     "provider-a": "https://api.example-a.com",
//     "provider-b": "https://api.example-b.com"
//   }
// }
