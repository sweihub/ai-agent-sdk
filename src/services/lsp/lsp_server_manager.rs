// Source: ~/claudecode/openclaudecode/src/services/lsp/LSPServerManager.ts
//! LSP Server Manager - manages multiple LSP server instances
//!
//! Manages multiple LSP server instances and routes requests based on file extensions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::services::lsp::config::get_all_lsp_servers;
use crate::services::lsp::lsp_server_instance::{create_lsp_server_instance, LspServerInstance};
use crate::services::lsp::types::LspServerState;
use crate::services::lsp::lsp_client::{NotificationHandler, RequestHandlerFn};
use crate::utils::debug::{log_for_debugging, DebugLogLevel};
use crate::utils::errors::error_message;

/// LSP server manager returned by [create_lsp_server_manager].
///
/// Manages multiple LSP server instances and routes requests based on file extensions.
#[derive(Clone)]
pub struct LspServerManager {
    /// Server instances keyed by server name
    servers: Arc<std::sync::Mutex<HashMap<String, Arc<LspServerInstance>>>>,
    /// Extension to server name mapping: ext -> list of server names
    extension_map: Arc<std::sync::Mutex<HashMap<String, Vec<String>>>>,
    /// Track which files are open: file_uri -> server_name
    opened_files: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl LspServerManager {
    /// Initialize the manager by loading all configured LSP servers.
    pub async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let result = get_all_lsp_servers().await;
        let server_configs = &result.servers;

        log_for_debugging(
            &format!(
                "[LSP SERVER MANAGER] getAllLspServers returned {} server(s)",
                server_configs.len()
            ),
            DebugLogLevel::Debug,
        );

        let mut servers = self.servers.lock().unwrap();
        let mut extension_map = self.extension_map.lock().unwrap();

        for (server_name, config) in server_configs {
            // Validate config
            if config.command.is_empty() {
                log_for_debugging(
                    &format!(
                        "[LSP] Server '{}' missing required 'command' field",
                        server_name
                    ),
                    DebugLogLevel::Error,
                );
                continue;
            }

            if config.extension_to_language.is_empty() {
                log_for_debugging(
                    &format!(
                        "[LSP] Server '{}' missing required 'extensionToLanguage' field",
                        server_name
                    ),
                    DebugLogLevel::Error,
                );
                continue;
            }

            // Map file extensions to this server
            for ext in config.extension_to_language.keys() {
                let normalized = ext.to_lowercase();
                extension_map
                    .entry(normalized)
                    .or_insert_with(Vec::new)
                    .push(server_name.clone());
            }

            // Create server instance
            let instance = Arc::new(create_lsp_server_instance(server_name, config.clone()));

            // Register handler for workspace/configuration requests from the server
            instance.on_request(
                "workspace/configuration".to_string(),
                Box::new(
                    move |params: serde_json::Value| -> std::pin::Pin<
                        Box<dyn std::future::Future<Output = serde_json::Value> + Send>,
                    > {
                        log_for_debugging(
                            "[LSP] Received workspace/configuration request from server",
                            DebugLogLevel::Debug,
                        );
                        let items = params
                            .get("items")
                            .and_then(|i| i.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|_| serde_json::json!(null))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        Box::pin(async move { serde_json::json!(items) })
                    },
                ) as RequestHandlerFn,
            );

            servers.insert(server_name.clone(), instance);
        }

        log_for_debugging(
            &format!(
                "[LSP] Manager initialized with {} servers",
                servers.len()
            ),
            DebugLogLevel::Debug,
        );

        Ok(())
    }

    /// Shutdown all running servers and clear state.
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let to_stop: Vec<(String, Arc<LspServerInstance>)> = {
            let servers = self.servers.lock().unwrap();
            servers
                .iter()
                .filter(|(_, s)| {
                    s.state() == LspServerState::Running || s.state() == LspServerState::Error
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        let mut errors = Vec::new();
        for (name, server) in &to_stop {
            if let Err(e) = server.stop().await {
                errors.push(format!("{}: {}", name, e.to_string()));
            }
        }

        // Clear all state
        self.servers.lock().unwrap().clear();
        self.extension_map.lock().unwrap().clear();
        self.opened_files.lock().unwrap().clear();

        if !errors.is_empty() {
            let err = format!(
                "Failed to stop {} LSP server(s): {}",
                errors.len(),
                errors.join("; ")
            );
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        Ok(())
    }

    /// Get the LSP server instance for a given file path.
    pub fn get_server_for_file(&self, file_path: &str) -> Option<Arc<LspServerInstance>> {
        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default()
            .to_lowercase();

        let extension_map = self.extension_map.lock().unwrap();
        let server_names = extension_map.get(&ext)?;

        if server_names.is_empty() {
            return None;
        }

        let server_name = &server_names[0];
        let servers = self.servers.lock().unwrap();
        servers.get(server_name).cloned()
    }

    /// Ensure the appropriate LSP server is started for the given file.
    pub async fn ensure_server_started(
        &self,
        file_path: &str,
    ) -> Option<Arc<LspServerInstance>> {
        let server = self.get_server_for_file(file_path)?;

        let state = server.state();
        if state == LspServerState::Stopped || state == LspServerState::Error {
            if let Err(e) = server.start().await {
                log_for_debugging(
                    &format!(
                        "[LSP] Failed to start LSP server for file {}: {}",
                        file_path,
                        e.to_string()
                    ),
                    DebugLogLevel::Error,
                );
                return None;
            }
        }

        Some(server)
    }

    /// Send a request to the appropriate LSP server for the given file.
    pub async fn send_request(
        &self,
        file_path: &str,
        method: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let server = self
            .ensure_server_started(file_path)
            .await
            .ok_or_else(|| format!("No LSP server for file: {}", file_path))?;

        server.send_request(method, params).await
    }

    /// Get all registered server instances.
    pub fn get_all_servers(&self) -> HashMap<String, Arc<LspServerInstance>> {
        self.servers.lock().unwrap().clone()
    }

    /// Synchronize file open to LSP server (sends didOpen notification).
    pub async fn open_file(
        &self,
        file_path: &str,
        content: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = self
            .ensure_server_started(file_path)
            .await
            .ok_or_else(|| format!("No LSP server for file: {}", file_path))?;

        let file_uri = path_to_file_uri(file_path);

        // Skip if already opened on this server
        {
            let opened = self.opened_files.lock().unwrap();
            if let Some(opened_server) = opened.get(&file_uri) {
                if opened_server == server.name() {
                    log_for_debugging(
                        &format!(
                            "[LSP] File already open, skipping didOpen for {}",
                            file_path
                        ),
                        DebugLogLevel::Debug,
                    );
                    return Ok(());
                }
            }
        }

        // Get language ID from server's extensionToLanguage mapping
        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default()
            .to_lowercase();
        let language_id = server
            .config()
            .extension_to_language
            .get(&ext)
            .cloned()
            .unwrap_or_else(|| "plaintext".to_string());

        server
            .send_notification(
                "textDocument/didOpen".to_string(),
                serde_json::json!({
                    "textDocument": {
                        "uri": file_uri,
                        "languageId": language_id,
                        "version": 1,
                        "text": content,
                    }
                }),
            )
            .await?;

        // Track that this file is now open on this server
        self.opened_files
            .lock()
            .unwrap()
            .insert(file_uri, server.name().to_string());

        log_for_debugging(
            &format!(
                "[LSP] Sent didOpen for {} (languageId: {})",
                file_path, language_id
            ),
            DebugLogLevel::Debug,
        );

        Ok(())
    }

    /// Synchronize file change to LSP server (sends didChange notification).
    pub async fn change_file(
        &self,
        file_path: &str,
        content: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = self.get_server_for_file(file_path);

        match server {
            Some(ref s) if s.state() == LspServerState::Running => {
                let file_uri = path_to_file_uri(file_path);

                let is_open = {
                    let opened = self.opened_files.lock().unwrap();
                    opened.get(&file_uri).map(|n| n == s.name()).unwrap_or(false)
                };

                if !is_open {
                    return self.open_file(file_path, content).await;
                }

                s.send_notification(
                    "textDocument/didChange".to_string(),
                    serde_json::json!({
                        "textDocument": {
                            "uri": file_uri,
                            "version": 1,
                        },
                        "contentChanges": [{ "text": content }],
                    }),
                )
                .await?;

                log_for_debugging(
                    &format!("[LSP] Sent didChange for {}", file_path),
                    DebugLogLevel::Debug,
                );
            }
            _ => {
                let _ = self.open_file(file_path, content).await;
            }
        }

        Ok(())
    }

    /// Synchronize file save to LSP server (sends didSave notification).
    pub async fn save_file(
        &self,
        file_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = self.get_server_for_file(file_path);

        match server {
            Some(s) if s.state() == LspServerState::Running => {
                let file_uri = path_to_file_uri(file_path);
                s.send_notification(
                    "textDocument/didSave".to_string(),
                    serde_json::json!({
                        "textDocument": { "uri": file_uri }
                    }),
                )
                .await?;

                log_for_debugging(
                    &format!("[LSP] Sent didSave for {}", file_path),
                    DebugLogLevel::Debug,
                );
            }
            _ => {}
        }

        Ok(())
    }

    /// Synchronize file close to LSP server (sends didClose notification).
    pub async fn close_file(
        &self,
        file_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = self.get_server_for_file(file_path);

        match server {
            Some(s) if s.state() == LspServerState::Running => {
                let file_uri = path_to_file_uri(file_path);

                s.send_notification(
                    "textDocument/didClose".to_string(),
                    serde_json::json!({
                        "textDocument": { "uri": file_uri }
                    }),
                )
                .await?;

                self.opened_files.lock().unwrap().remove(&file_uri);

                log_for_debugging(
                    &format!("[LSP] Sent didClose for {}", file_path),
                    DebugLogLevel::Debug,
                );
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a file is already open on a compatible LSP server.
    pub fn is_file_open(&self, file_path: &str) -> bool {
        let file_uri = path_to_file_uri(file_path);
        self.opened_files.lock().unwrap().contains_key(&file_uri)
    }
}

/// Convert a file path to a file:// URI.
fn path_to_file_uri(path: &str) -> String {
    let absolute = if std::path::Path::new(path).is_absolute() {
        path.to_string()
    } else {
        std::path::PathBuf::from(path)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string())
    };

    url::Url::from_file_path(&absolute)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", absolute))
}

/// Creates an LSP server manager instance.
pub fn create_lsp_server_manager() -> LspServerManager {
    LspServerManager {
        servers: Arc::new(std::sync::Mutex::new(HashMap::new())),
        extension_map: Arc::new(std::sync::Mutex::new(HashMap::new())),
        opened_files: Arc::new(std::sync::Mutex::new(HashMap::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_lsp_server_manager() {
        let manager = create_lsp_server_manager();
        assert!(manager.get_all_servers().is_empty());
    }

    #[test]
    fn test_path_to_file_uri() {
        let result = path_to_file_uri("/tmp/test.ts");
        assert!(result.starts_with("file://"));
    }
}
