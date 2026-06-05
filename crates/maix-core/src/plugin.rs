//! Plugin system — discover, load, and manage plugins.
//!
//! Plugins extend Maix with custom tools, commands, and hooks.
//! Convention: each plugin is a directory with a `plugin.toml` manifest.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Plugin manifest parsed from plugin.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub tools: PluginTools,
    #[serde(default)]
    pub commands: PluginCommands,
    #[serde(default)]
    pub hooks: PluginHooks,
}

/// Tool configuration in plugin manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginTools {
    #[serde(default)]
    pub files: Vec<String>,
}

/// Command configuration in plugin manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCommands {
    #[serde(default)]
    pub files: Vec<String>,
}

/// Hook configuration in plugin manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginHooks {
    #[serde(default)]
    pub pre_tool_use: Option<String>,
    #[serde(default)]
    pub post_tool_use: Option<String>,
    #[serde(default)]
    pub on_error: Option<String>,
}

/// Plugin lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Discovered,
    Loaded,
    Enabled,
    Disabled,
    Error(String),
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovered => write!(f, "discovered"),
            Self::Loaded => write!(f, "loaded"),
            Self::Enabled => write!(f, "enabled"),
            Self::Disabled => write!(f, "disabled"),
            Self::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}

/// A discovered or loaded plugin.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub state: PluginState,
}

/// Plugin manager — discovers, loads, and manages plugins.
pub struct PluginManager {
    plugins: Vec<Plugin>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            plugin_dir,
        }
    }

    /// Discover plugins in the plugin directory.
    pub fn discover(&mut self) -> Vec<String> {
        let mut discovered = Vec::new();

        if !self.plugin_dir.exists() {
            return discovered;
        }

        let entries = match std::fs::read_dir(&self.plugin_dir) {
            Ok(e) => e,
            Err(_) => return discovered,
        };

        for entry in entries.flatten() {
            if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }

            let manifest_path = entry.path().join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }

            let content = match std::fs::read_to_string(&manifest_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let manifest: PluginManifest = match toml::from_str(&content) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let name = manifest.name.clone();
            self.plugins.push(Plugin {
                manifest,
                path: entry.path(),
                state: PluginState::Discovered,
            });
            discovered.push(name);
        }

        discovered
    }

    /// Load a plugin by name (validate manifest, prepare resources).
    pub fn load(&mut self, name: &str) -> Result<(), String> {
        let plugin = self.plugins.iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("plugin '{}' not found", name))?;

        // Validate manifest
        if plugin.manifest.name.is_empty() {
            return Err("plugin name is empty".to_string());
        }

        // Check that referenced files exist
        for tool_file in &plugin.manifest.tools.files {
            let tool_path = plugin.path.join(tool_file);
            if !tool_path.exists() {
                return Err(format!("tool file not found: {}", tool_path.display()));
            }
        }

        plugin.state = PluginState::Loaded;
        Ok(())
    }

    /// Enable a loaded plugin.
    pub fn enable(&mut self, name: &str) -> Result<(), String> {
        let plugin = self.plugins.iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("plugin '{}' not found", name))?;

        if plugin.state != PluginState::Loaded && plugin.state != PluginState::Disabled {
            return Err(format!("plugin '{}' must be loaded before enabling (current state: {})", name, plugin.state));
        }

        plugin.state = PluginState::Enabled;
        Ok(())
    }

    /// Disable an enabled plugin.
    pub fn disable(&mut self, name: &str) -> Result<(), String> {
        let plugin = self.plugins.iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("plugin '{}' not found", name))?;

        if plugin.state != PluginState::Enabled {
            return Err(format!("plugin '{}' is not enabled", name));
        }

        plugin.state = PluginState::Disabled;
        Ok(())
    }

    /// Unload a plugin.
    pub fn unload(&mut self, name: &str) -> Result<(), String> {
        let plugin = self.plugins.iter_mut()
            .find(|p| p.manifest.name == name)
            .ok_or_else(|| format!("plugin '{}' not found", name))?;

        if plugin.state == PluginState::Enabled {
            return Err(format!("plugin '{}' is enabled; disable it before unloading", name));
        }

        plugin.state = PluginState::Discovered;
        Ok(())
    }

    /// Get a plugin by name.
    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.manifest.name == name)
    }

    /// List all plugins.
    pub fn list(&self) -> &[Plugin] {
        &self.plugins
    }

    /// List enabled plugins.
    pub fn enabled_plugins(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.state == PluginState::Enabled).collect()
    }

    /// Get tool files from all enabled plugins.
    pub fn enabled_tool_files(&self) -> Vec<PathBuf> {
        self.plugins.iter()
            .filter(|p| p.state == PluginState::Enabled)
            .flat_map(|p| {
                p.manifest.tools.files.iter()
                    .map(|f| p.path.join(f))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Get hook scripts from all enabled plugins.
    pub fn enabled_hooks(&self) -> Vec<(String, PathBuf)> {
        let mut hooks = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.state == PluginState::Enabled) {
            if let Some(ref pre) = plugin.manifest.hooks.pre_tool_use {
                hooks.push(("pre_tool_use".to_string(), plugin.path.join(pre)));
            }
            if let Some(ref post) = plugin.manifest.hooks.post_tool_use {
                hooks.push(("post_tool_use".to_string(), plugin.path.join(post)));
            }
            if let Some(ref err) = plugin.manifest.hooks.on_error {
                hooks.push(("on_error".to_string(), plugin.path.join(err)));
            }
        }
        hooks
    }

    /// Format plugin list for display.
    pub fn format_list(&self) -> String {
        if self.plugins.is_empty() {
            return "No plugins found.".to_string();
        }

        let mut lines = vec![format!("Plugins ({}):", self.plugins.len())];
        for plugin in &self.plugins {
            lines.push(format!(
                "  {} v{} [{}] - {}",
                plugin.manifest.name,
                plugin.manifest.version,
                plugin.state,
                plugin.manifest.description
            ));
        }
        lines.join("\n")
    }

    /// Load and enable all discovered plugins.
    pub fn load_all(&mut self) -> Vec<String> {
        let names: Vec<String> = self.plugins.iter().map(|p| p.manifest.name.clone()).collect();
        let mut loaded = Vec::new();
        for name in &names {
            if self.load(name).is_ok() && self.enable(name).is_ok() {
                loaded.push(name.clone());
            }
        }
        loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn create_test_plugin(dir: &Path, name: &str) {
        let plugin_dir = dir.join(name);
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"
name = "{}"
version = "1.0.0"
description = "Test plugin"
author = "Test"

[tools]
files = ["tool.sh"]

[hooks]
pre_tool_use = "pre.sh"
"#,
            name
        );
        fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
        fs::write(plugin_dir.join("tool.sh"), "#!/bin/bash\necho hello").unwrap();
        fs::write(plugin_dir.join("pre.sh"), "#!/bin/bash\necho pre").unwrap();
    }

    #[test]
    fn test_discover_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "test-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        let discovered = mgr.discover();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0], "test-plugin");
    }

    #[test]
    fn test_load_and_enable() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "my-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();

        assert!(mgr.load("my-plugin").is_ok());
        assert!(mgr.enable("my-plugin").is_ok());

        let plugin = mgr.get("my-plugin").unwrap();
        assert_eq!(plugin.state, PluginState::Enabled);
    }

    #[test]
    fn test_disable_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "my-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();
        mgr.load("my-plugin").unwrap();
        mgr.enable("my-plugin").unwrap();

        assert!(mgr.disable("my-plugin").is_ok());
        assert_eq!(mgr.get("my-plugin").unwrap().state, PluginState::Disabled);
    }

    #[test]
    fn test_load_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        assert!(mgr.load("nonexistent").is_err());
    }

    #[test]
    fn test_enabled_tool_files() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "my-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();
        mgr.load("my-plugin").unwrap();
        mgr.enable("my-plugin").unwrap();

        let files = mgr.enabled_tool_files();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("tool.sh"));
    }

    #[test]
    fn test_enabled_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "my-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();
        mgr.load("my-plugin").unwrap();
        mgr.enable("my-plugin").unwrap();

        let hooks = mgr.enabled_hooks();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].0, "pre_tool_use");
    }

    #[test]
    fn test_format_list() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "test-plugin");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();

        let list = mgr.format_list();
        assert!(list.contains("test-plugin"));
        assert!(list.contains("1.0.0"));
    }

    #[test]
    fn test_load_all() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_plugin(tmp.path(), "plugin-a");
        create_test_plugin(tmp.path(), "plugin-b");

        let mut mgr = PluginManager::new(tmp.path().to_path_buf());
        mgr.discover();
        let loaded = mgr.load_all();
        assert_eq!(loaded.len(), 2);
    }
}
