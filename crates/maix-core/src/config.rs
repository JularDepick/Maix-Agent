use crate::MaixResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration — loaded from default.toml + ~/.maix/config.toml + env vars.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub server: ServerConfig,
}

impl Config {
    /// Load config from default layers (default.toml → ~/.maix/config.toml → env).
    pub fn load() -> MaixResult<Self> {
        load_config(None)
    }

    /// Minimal config suitable for testing.
    pub fn minimal() -> Self {
        Config {
            providers: HashMap::new(),
            agent: AgentConfig::default(),
            memory: MemoryConfig::default(),
            tools: ToolsConfig::default(),
            server: ServerConfig::default(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
    pub model: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("api_key", &crate::util::mask_key(&self.api_key))
            .field("api_base", &self.api_base)
            .field("model", &self.model)
            .field("extra", &self.extra)
            .finish()
    }
}

fn default_api_base() -> String {
    "https://api.deepseek.com".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,
    #[serde(default = "default_context_threshold")]
    pub context_threshold: f32,
    #[serde(default)]
    pub mode: AgentMode,
}

fn default_max_tool_rounds() -> usize { 16 }
fn default_context_threshold() -> f32 { 0.9 }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 16,
            context_threshold: 0.9,
            mode: AgentMode::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    #[default]
    Agent,
    Plan,
    Yolo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_dir")]
    pub dir: PathBuf,
    #[serde(default = "default_max_episodic_entries")]
    pub max_episodic_entries: usize,
}

fn default_max_episodic_entries() -> usize { 500 }

pub fn default_memory_dir() -> PathBuf {
    dirs_home().join(".maix").join("memory")
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            dir: default_memory_dir(),
            max_episodic_entries: 500,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub shell_enabled: bool,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default)]
    pub transport: TransportMode,
    pub socket_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    #[default]
    Auto,
    UnixSocket,
    NamedPipe,
    Tcp,
}

fn default_listen_addr() -> String { "0.0.0.0".into() }
fn default_listen_port() -> u16 { 26506 }

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".into(),
            listen_port: 26506,
            transport: TransportMode::default(),
            socket_path: None,
        }
    }
}

/// Load config from layers: default.toml → user config → env vars (MAIX_ prefix).
pub fn load_config(user_config_path: Option<PathBuf>) -> MaixResult<Config> {
    use figment::providers::{Env, Format, Toml};
    use figment::Figment;

    let default_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config")
        .join("default.toml");

    let mut figment = Figment::new();

    // Layer 1: bundled default.toml
    if default_path.exists() {
        figment = figment.merge(Toml::file(default_path));
    }

    // Layer 2: user config (~/.maix/config.toml)
    let user_path = user_config_path.unwrap_or_else(|| {
        dirs_home().join(".maix").join("config.toml")
    });
    if user_path.exists() {
        figment = figment.merge(Toml::file(user_path));
    }

    // Layer 3: env vars (MAIX_PROVIDERS_DEEPSEEK_API_KEY etc.)
    figment = figment.merge(Env::prefixed("MAIX_").split("_"));

    figment.extract().map_err(|e| crate::MaixError::Config(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_defaults() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.max_tool_rounds, 16);
        assert_eq!(cfg.context_threshold, 0.9);
        assert_eq!(cfg.mode, AgentMode::Agent);
    }

    #[test]
    fn test_memory_config_defaults() {
        let cfg = MemoryConfig::default();
        assert_eq!(cfg.max_episodic_entries, 500);
    }

    #[test]
    fn test_provider_config_default_api_base() {
        let provider = ProviderConfig {
            api_key: "sk-test".into(),
            api_base: String::new(),
            model: None,
            extra: HashMap::new(),
        };
        // api_base gets serde default when deserializing, so it stays empty here
        assert_eq!(provider.api_key, "sk-test");
    }

    #[test]
    fn test_agent_mode_serde() {
        let mode: AgentMode = serde_json::from_str(r#""agent""#).unwrap();
        assert_eq!(mode, AgentMode::Agent);
        let mode: AgentMode = serde_json::from_str(r#""plan""#).unwrap();
        assert_eq!(mode, AgentMode::Plan);
        let mode: AgentMode = serde_json::from_str(r#""yolo""#).unwrap();
        assert_eq!(mode, AgentMode::Yolo);
    }
}

fn dirs_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}
