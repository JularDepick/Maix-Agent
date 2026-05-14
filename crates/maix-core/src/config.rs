use crate::MaixResult;
use crate::model_router::AutoModeConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration — merged from system (exe dir) + user (~/.maix/) + env vars.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Active provider name
    #[serde(default)]
    pub provider: String,
    /// API key
    #[serde(default)]
    pub api_key: String,
    /// API base URL
    #[serde(default)]
    pub api_base: String,
    /// Model name
    #[serde(default)]
    pub model: String,

    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub server: ServerConfig,

    /// Hooks — lifecycle commands keyed by hook type.
    #[serde(default)]
    pub hooks: HashMap<String, Vec<HookEntry>>,
}

/// User-level settings stored in ~/.maix/settings.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSettings {
    /// Active provider name
    #[serde(default)]
    pub provider: String,
    /// API key
    #[serde(default)]
    pub api_key: String,
    /// API base URL
    #[serde(default)]
    pub api_base: String,
    /// Model name
    #[serde(default)]
    pub model: String,

    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub tools: ToolsConfig,

    /// Hooks — lifecycle commands keyed by hook type (PreToolUse, PostToolUse, Stop).
    #[serde(default)]
    pub hooks: HashMap<String, Vec<HookEntry>>,

    /// Environment variable overrides defined in settings.json.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// A single hook entry in settings.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// Tool name glob pattern. Empty matches all.
    #[serde(default)]
    pub matcher: String,
    /// Shell command to execute.
    pub command: String,
    /// Timeout in milliseconds. Default: 5000.
    pub timeout_ms: Option<u64>,
}

impl Config {
    /// Load config from system (exe dir/system.toml) + user (~/.maix/settings.json) + env vars.
    pub fn load() -> MaixResult<Self> {
        load_config(None)
    }

    /// Minimal config suitable for testing.
    pub fn minimal() -> Self {
        Config {
            provider: String::new(),
            api_key: String::new(),
            api_base: String::new(),
            model: String::new(),
            agent: AgentConfig::default(),
            memory: MemoryConfig::default(),
            tools: ToolsConfig::default(),
            server: ServerConfig::default(),
            hooks: HashMap::new(),
        }
    }

    /// Extract user-level settings from this config.
    pub fn user_settings(&self) -> UserSettings {
        UserSettings {
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            api_base: self.api_base.clone(),
            model: self.model.clone(),
            agent: self.agent.clone(),
            memory: self.memory.clone(),
            tools: self.tools.clone(),
            hooks: self.hooks.clone(),
            env: HashMap::new(),
        }
    }

    /// Save user-level settings to ~/.maix/settings.json.
    pub fn save_user_settings(settings: &UserSettings) -> MaixResult<PathBuf> {
        let path = user_settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(crate::MaixError::Io)?;
        }
        let json = serde_json::to_string_pretty(settings)
            .map_err(crate::MaixError::Json)?;
        std::fs::write(&path, json)
            .map_err(crate::MaixError::Io)?;
        Ok(path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,
    #[serde(default = "default_context_threshold")]
    pub context_threshold: f32,
    #[serde(default)]
    pub mode: AgentMode,
    /// Auto-mode routing: per-turn cheap vs capable model selection.
    #[serde(default)]
    pub auto_mode: AutoModeConfig,
}

fn default_max_tool_rounds() -> usize { 16 }
fn default_context_threshold() -> f32 { 0.9 }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 16,
            context_threshold: 0.9,
            mode: AgentMode::default(),
            auto_mode: AutoModeConfig::default(),
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
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
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

fn default_listen_addr() -> String { "127.0.0.1".into() }
fn default_listen_port() -> u16 { 26506 }

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1".into(),
            listen_port: 26506,
            transport: TransportMode::default(),
            socket_path: None,
        }
    }
}

/// Path to the directory containing the maix binary.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Path to system-level config: `<exe_dir>/system.toml`
pub fn system_config_path() -> PathBuf {
    exe_dir().join("system.toml")
}

/// Path to user-level settings: `~/.maix/settings.json`
pub fn user_settings_path() -> PathBuf {
    dirs_home().join(".maix").join("settings.json")
}

/// Legacy user config path (for migration): `~/.maix/config.toml`
fn legacy_user_config_path() -> PathBuf {
    dirs_home().join(".maix").join("config.toml")
}

/// Auto-create ~/.maix/settings.json and settings-default.md if they don't exist.
fn ensure_user_settings() {
    let path = user_settings_path();
    let maix_dir = path.parent().unwrap();

    // Try migrating from legacy config.toml
    if !path.exists() {
        let legacy = legacy_user_config_path();
        if legacy.exists() {
            if let Ok(content) = std::fs::read_to_string(&legacy) {
                if let Ok(settings) = toml::from_str::<UserSettings>(&content) {
                    if let Ok(json) = serde_json::to_string_pretty(&settings) {
                        let _ = std::fs::create_dir_all(maix_dir);
                        let _ = std::fs::write(&path, json);
                        tracing::info!("migrated legacy config.toml to settings.json");
                    }
                }
            }
        }
    }

    // Create default settings.json
    if !path.exists() {
        let default_settings = UserSettings::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_settings) {
            let _ = std::fs::create_dir_all(maix_dir);
            let _ = std::fs::write(&path, json);
            tracing::info!("created default settings at {}", path.display());
        }
    }

    // Create settings-default.md documentation
    let doc_path = maix_dir.join("settings-default.md");
    if !doc_path.exists() {
        let _ = std::fs::create_dir_all(maix_dir);
        let _ = std::fs::write(&doc_path, SETTINGS_DOC);
        tracing::info!("created settings doc at {}", doc_path.display());
    }
}

const SETTINGS_DOC: &str = r#"# Maix-Agent 用户配置说明

配置文件路径: `~/.maix/settings.json`

## 配置模板

```json
{
  "provider": "my-provider",
  "api_key": "sk-xxx",
  "api_base": "https://api.example.com/v1",
  "model": "model-name",

  "agent": {
    "max_tool_rounds": 16,
    "context_threshold": 0.9,
    "mode": "agent",
    "auto_mode": {
      "enabled": false,
      "cheap_model": "gpt-4o-mini",
      "cheap_provider": "",
      "capable_model": "claude-sonnet-4-6",
      "capable_provider": "anthropic"
    }
  },

  "memory": {
    "dir": "",
    "max_episodic_entries": 500
  },

  "tools": {
    "shell_enabled": false,
    "mcp_servers": []
  },

  "hooks": {
    "PreToolUse": [
      {
        "matcher": "fs_write",
        "command": "echo 'about to write $MAIX_FILE_PATH'",
        "timeout_ms": 5000
      }
    ],
    "PostToolUse": [
      {
        "matcher": "fs_edit",
        "command": "prettier --write $MAIX_FILE_PATH",
        "timeout_ms": 30000
      }
    ],
    "Stop": [
      {
        "matcher": "",
        "command": "notify-send 'Maix task complete'"
      }
    ]
  },

  "env": {}
}
```

## 配置项说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `provider` | string | 服务商名称（自定义标识） |
| `api_key` | string | API 密钥 |
| `api_base` | string | API 地址 |
| `model` | string | 模型名称 |

### agent — 智能体

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `max_tool_rounds` | number | `16` | 最大工具调用轮次 |
| `context_threshold` | number | `0.9` | 上下文压缩阈值 |
| `mode` | string | `"agent"` | 默认模式: agent / plan / yolo |

#### auto_mode — 自动模式路由

每轮对话根据任务复杂度自动选择 cheap（快速）或 capable（强力）模型。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `enabled` | boolean | `false` | 启用自动模式路由 |
| `cheap_model` | string | `""` | 简单任务使用的快速模型 |
| `cheap_provider` | string | `""` | 快速模型的服务商（空则用默认） |
| `capable_model` | string | `""` | 复杂任务使用的强力模型 |
| `capable_provider` | string | `""` | 强力模型的服务商（空则用默认） |

路由逻辑:
- **Off** (简单): 问候、简短问题 → cheap_model
- **High** (复杂): 编码、调试、多步任务 → capable_model
- **Max** (深度推理): "think step by step"、架构设计 → capable_model

### memory — 记忆

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `dir` | string | `""` | 存储目录，空则 `~/.maix/memory` |
| `max_episodic_entries` | number | `500` | 最大条目数 |

### tools — 工具

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `shell_enabled` | boolean | `false` | 启用 Shell |
| `mcp_servers` | array | `[]` | MCP 服务器配置 |

#### mcp_servers 示例

```json
"mcp_servers": [
  {
    "name": "filesystem",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    "env": {}
  }
]
```

### hooks — 生命周期钩子

在工具执行前后运行用户自定义命令。

| 钩子 | 触发时机 | 用途 |
|------|----------|------|
| `PreToolUse` | 工具执行前 | 拦截危险操作、日志记录 |
| `PostToolUse` | 工具执行后 | 自动格式化、通知 |
| `Stop` | Agent 循环结束 | 发送通知、清理 |

环境变量:
- `MAIX_TOOL_NAME` — 当前工具名
- `MAIX_FILE_PATH` — 操作的文件路径（如适用）
- `MAIX_TOOL_INPUT` — JSON 格式工具输入
- `MAIX_TOOL_OUTPUT` — JSON 格式工具输出（仅 PostToolUse）
- `MAIX_WORKING_DIR` — 工作目录

PreToolUse hook 非零退出码会阻止工具执行。

### env — 环境变量覆盖

在 settings.json 中定义环境变量，系统环境变量优先级更高。

## 环境变量

所有环境变量以 `MAIX_` 开头：

| 环境变量 | 对应配置 |
|----------|----------|
| `MAIX_API_KEY` | `api_key` |
| `MAIX_API_BASE` | `api_base` |
| `MAIX_MODEL` | `model` |
| `MAIX_PROVIDER` | `provider` |
| `MAIX_AGENT_MAX_TOOL_ROUNDS` | `agent.max_tool_rounds` |
| `MAIX_AGENT_CONTEXT_THRESHOLD` | `agent.context_threshold` |
| `MAIX_AGENT_MODE` | `agent.mode` |
| `MAIX_TOOLS_SHELL_ENABLED` | `tools.shell_enabled` |

优先级：系统环境变量 > settings.json env 字段 > settings.json 直接配置
"#;

/// Load config from layers: default.toml → system.toml → settings.json → env vars.
pub fn load_config(user_config_path: Option<PathBuf>) -> MaixResult<Config> {
    use figment::providers::{Env, Format, Json, Toml};
    use figment::Figment;

    // Pre-step: apply env vars from settings.json's "env" field
    apply_settings_env_vars();

    let mut figment = Figment::new();

    // Layer 1: bundled default.toml (lowest priority)
    let default_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config")
        .join("default.toml");
    if default_path.exists() {
        figment = figment.merge(Toml::file(default_path));
    }

    // Layer 2: system config (exe dir/system.toml)
    let sys_path = system_config_path();
    if sys_path.exists() {
        figment = figment.merge(Toml::file(sys_path));
    }

    // Layer 3: user settings (~/.maix/settings.json)
    ensure_user_settings();
    if let Some(user_path) = user_config_path {
        if user_path.exists() {
            figment = figment.merge(Toml::file(user_path));
        }
    } else {
        let settings_path = user_settings_path();
        if settings_path.exists() {
            figment = figment.merge(Json::file(settings_path));
        }
    }

    // Layer 4: env vars (MAIX_ prefix) — highest priority
    figment = figment.merge(Env::prefixed("MAIX_").split("_"));

    let config: Config = figment.extract().map_err(|e| crate::MaixError::Config(Box::new(e)))?;

    Ok(config)
}

/// Read the "env" field from settings.json and set those env vars
/// (only if not already set in the actual environment).
fn apply_settings_env_vars() {
    let path = user_settings_path();
    if !path.exists() {
        return;
    }
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(env_map) = value.get("env").and_then(|v| v.as_object()) {
                for (key, val) in env_map {
                    if let Some(val_str) = val.as_str() {
                        if std::env::var(key).is_err() {
                            std::env::set_var(key, val_str);
                        }
                    }
                }
            }
        }
    }
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
    fn test_agent_mode_serde() {
        let mode: AgentMode = serde_json::from_str(r#""agent""#).unwrap();
        assert_eq!(mode, AgentMode::Agent);
        let mode: AgentMode = serde_json::from_str(r#""plan""#).unwrap();
        assert_eq!(mode, AgentMode::Plan);
        let mode: AgentMode = serde_json::from_str(r#""yolo""#).unwrap();
        assert_eq!(mode, AgentMode::Yolo);
    }

    #[test]
    fn test_validate_config() {
        let config = Config::minimal();
        let errors = validate_config(&config);
        assert!(!errors.is_empty()); // api_key is empty
    }

    #[test]
    fn test_validate_config_valid() {
        let mut config = Config::minimal();
        config.api_key = "sk-test".to_string();
        config.api_base = "https://api.example.com".to_string();
        let errors = validate_config(&config);
        assert!(errors.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

/// A configuration validation error.
#[derive(Debug, Clone)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Validate a configuration and return any errors.
pub fn validate_config(config: &Config) -> Vec<ConfigError> {
    let mut errors = Vec::new();

    if config.api_key.is_empty() {
        errors.push(ConfigError {
            field: "api_key".into(),
            message: "API key is not configured. Set in ~/.maix/settings.json or MAIX_API_KEY env var".into(),
        });
    }

    if !config.api_base.is_empty() && !config.api_base.starts_with("http") {
        errors.push(ConfigError {
            field: "api_base".into(),
            message: "API base URL must start with http:// or https://".into(),
        });
    }

    if config.agent.max_tool_rounds == 0 {
        errors.push(ConfigError {
            field: "agent.max_tool_rounds".into(),
            message: "Must be greater than 0".into(),
        });
    }

    if config.agent.context_threshold < 0.1 || config.agent.context_threshold > 1.0 {
        errors.push(ConfigError {
            field: "agent.context_threshold".into(),
            message: "Must be between 0.1 and 1.0".into(),
        });
    }

    errors
}

// ---------------------------------------------------------------------------
// Config export/import
// ---------------------------------------------------------------------------

/// Export config to JSON (with api_key masked).
pub fn export_config(config: &Config) -> MaixResult<String> {
    let mut settings = config.user_settings();
    // Mask API key for security
    if !settings.api_key.is_empty() {
        let len = settings.api_key.len();
        if len > 8 {
            settings.api_key = format!("{}...{}", &settings.api_key[..4], &settings.api_key[len - 4..]);
        } else {
            settings.api_key = "***".to_string();
        }
    }
    serde_json::to_string_pretty(&settings).map_err(crate::MaixError::Json)
}

/// Import config from JSON string.
pub fn import_config(json: &str) -> MaixResult<UserSettings> {
    serde_json::from_str(json).map_err(crate::MaixError::Json)
}

/// Show config diff from defaults.
pub fn config_diff(config: &Config) -> String {
    let defaults = Config::minimal();
    let mut diffs = Vec::new();

    if config.provider != defaults.provider {
        diffs.push(format!("  provider: {} → {}", defaults.provider, config.provider));
    }
    if config.model != defaults.model {
        diffs.push(format!("  model: {} → {}", defaults.model, config.model));
    }
    if config.api_base != defaults.api_base {
        diffs.push(format!("  api_base: {} → {}", defaults.api_base, config.api_base));
    }
    if config.agent.max_tool_rounds != defaults.agent.max_tool_rounds {
        diffs.push(format!("  agent.max_tool_rounds: {} → {}", defaults.agent.max_tool_rounds, config.agent.max_tool_rounds));
    }
    if config.agent.context_threshold != defaults.agent.context_threshold {
        diffs.push(format!("  agent.context_threshold: {} → {}", defaults.agent.context_threshold, config.agent.context_threshold));
    }

    if diffs.is_empty() {
        "No differences from defaults.".to_string()
    } else {
        format!("Config differences from defaults:\n{}", diffs.join("\n"))
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
