//! Language server manager — spawns and manages LSP clients per language.

use super::client::{LspClient, path_to_uri};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Manages multiple LSP clients for different languages.
pub struct LspManager {
    clients: HashMap<String, LspClient>,
    #[allow(dead_code)]
    root_path: PathBuf,
    root_uri: String,
}

impl LspManager {
    /// Create a new LSP manager for the given workspace root.
    pub fn new(root_path: PathBuf) -> Self {
        let root_uri = path_to_uri(&root_path);
        Self {
            clients: HashMap::new(),
            root_path,
            root_uri,
        }
    }

    /// Get or create an LSP client for the given file.
    pub async fn get_or_start(&mut self, path: &Path) -> Result<&mut LspClient, String> {
        let lang = LspClient::language_id_from_path(path);
        let lang_key = lang.to_string();

        if !self.clients.contains_key(&lang_key) {
            let (command, args) = Self::detect_lsp_server(lang)?;
            let client = LspClient::connect(command, &args, &self.root_uri).await?;
            self.clients.insert(lang_key.clone(), client);
        }

        Ok(self.clients.get_mut(&lang_key).unwrap())
    }

    /// Detect the LSP server command for a language.
    fn detect_lsp_server(lang: &str) -> Result<(&'static str, Vec<&'static str>), String> {
        match lang {
            "rust" => {
                if super::which("rust-analyzer") {
                    Ok(("rust-analyzer", vec![]))
                } else {
                    Err("rust-analyzer not found on PATH".into())
                }
            }
            "typescript" | "javascript" => {
                if super::which("typescript-language-server") {
                    Ok(("typescript-language-server", vec!["--stdio"]))
                } else if super::which("tsserver") {
                    Ok(("tsserver", vec![]))
                } else {
                    Err("typescript-language-server not found on PATH".into())
                }
            }
            "python" => {
                if super::which("pyright-langserver") {
                    Ok(("pyright-langserver", vec!["--stdio"]))
                } else if super::which("pylsp") {
                    Ok(("pylsp", vec![]))
                } else {
                    Err("pyright-langserver not found on PATH".into())
                }
            }
            "go" => {
                if super::which("gopls") {
                    Ok(("gopls", vec![]))
                } else {
                    Err("gopls not found on PATH".into())
                }
            }
            _ => Err(format!("no LSP server configured for language: {}", lang)),
        }
    }

    /// Shutdown all connected language servers.
    pub async fn shutdown_all(&mut self) {
        for (_, mut client) in self.clients.drain() {
            let _ = client.shutdown().await;
        }
    }
}
