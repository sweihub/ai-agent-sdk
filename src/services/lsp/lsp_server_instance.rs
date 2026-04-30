// Source: ~/claudecode/openclaudecode/src/services/lsp/LSPServerInstance.ts
//! LSP Server Instance - manages the lifecycle of a single LSP server
//!
//! Provides state tracking, health monitoring, and request forwarding for an LSP server.
//! Supports manual restart with configurable retry limits.
//!
//! State machine transitions:
//! - stopped -> starting -> running
//! - running -> stopping -> stopped
//! - any -> error (on failure)
//! - error -> starting (on retry)

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::services::lsp::lsp_client::{create_lsp_client, LspClient};
use crate::services::lsp::types::{LspServerConfig, LspServerState};
use crate::utils::cwd::get_cwd;
use crate::utils::debug::{log_for_debugging, DebugLogLevel};
#[allow(unused_imports)]
use crate::utils::errors::error_message;

use crate::services::lsp::lsp_client::NotificationHandler;
use crate::services::lsp::lsp_client::RequestHandlerFn;

/// LSP error code for "content modified" - transient error during indexing
const LSP_ERROR_CONTENT_MODIFIED: i64 = -32801;

/// Maximum number of retries for transient LSP errors
const MAX_RETRIES_FOR_TRANSIENT_ERRORS: u32 = 3;

/// Base delay in milliseconds for exponential backoff on transient errors
const RETRY_BASE_DELAY_MS: u64 = 500;

/// Internal mutable state for the server instance
struct LspServerInternal {
    /// Current server state - Arc-wrapped for crash callback sharing
    state: Arc<Mutex<LspServerState>>,
    /// When the server was last started
    start_time: Mutex<Option<std::time::SystemTime>>,
    /// Last error encountered - Arc-wrapped for crash callback sharing
    last_error: Arc<Mutex<Option<String>>>,
    /// Number of times restart() has been called
    restart_count: AtomicU32,
    /// Number of crash recovery attempts - Arc-wrapped for crash callback sharing
    crash_recovery_count: Arc<AtomicU32>,
}

/// LSP server instance returned by [create_lsp_server_instance].
///
/// Manages the lifecycle of a single LSP server with state tracking, health
/// monitoring, and request forwarding. Supports manual restart with
/// configurable retry limits.
///
/// Internally uses Arc for shared state, so cloning the manager or instance
/// shares the same underlying process.
pub struct LspServerInstance {
    /// Unique server identifier
    name: String,
    /// Server configuration
    config: LspServerConfig,
    /// LSP client for process management
    client: Arc<LspClient>,
    /// Internal mutable state
    internal: Arc<LspServerInternal>,
}

impl LspServerInstance {
    /// Get the unique server identifier
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server configuration
    pub fn config(&self) -> &LspServerConfig {
        &self.config
    }

    /// Get the current server state
    pub fn state(&self) -> LspServerState {
        self.internal.state.lock().unwrap().clone()
    }

    /// Get the time when the server was last started
    pub fn start_time(&self) -> Option<std::time::SystemTime> {
        *self.internal.start_time.lock().unwrap()
    }

    /// Get the last error encountered
    pub fn last_error(&self) -> Option<String> {
        self.internal.last_error.lock().unwrap().clone()
    }

    /// Get the number of times restart() has been called
    pub fn restart_count(&self) -> u32 {
        self.internal.restart_count.load(Ordering::SeqCst)
    }

    /// Check if server is healthy and ready for requests
    pub fn is_healthy(&self) -> bool {
        *self.internal.state.lock().unwrap() == LspServerState::Running
            && self.client.is_initialized()
    }

    /// Start the server and initialize it with workspace information.
    ///
    /// If the server is already running or starting, this method returns immediately.
    /// On failure, sets state to 'error', logs for monitoring, and returns an error.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state = self.internal.state.lock().unwrap().clone();
        drop(self.internal.state.lock().unwrap());

        if state == LspServerState::Running || state == LspServerState::Starting {
            return Ok(());
        }

        // Cap crash-recovery attempts
        let max_restarts = self.config.max_restarts.unwrap_or(3);
        let current_state = self.internal.state.lock().unwrap().clone();
        let crash_count = self.internal.crash_recovery_count.load(Ordering::SeqCst);
        if current_state == LspServerState::Error && crash_count > max_restarts {
            let err = format!(
                "LSP server '{}' exceeded max crash recovery attempts ({})",
                self.name, max_restarts
            );
            *self.internal.last_error.lock().unwrap() = Some(err.clone());
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        // Set state to starting
        *self.internal.state.lock().unwrap() = LspServerState::Starting;
        log_for_debugging(
            &format!("[LSP] Starting LSP server instance: {}", self.name),
            DebugLogLevel::Debug,
        );

        let result = (async {
            // Start the client process
            self.client
                .start(
                    self.config.command.clone(),
                    self.config.args.clone(),
                    Some(crate::services::lsp::types::LspStartOptions {
                        env: Some(self.config.env.clone()),
                        cwd: self.config.workspace_folder.clone().or_else(|| {
                            Some(get_cwd().to_string_lossy().to_string())
                        }),
                    }),
                )
                .await?;

            // Build initialize params
            let workspace_folder = self
                .config
                .workspace_folder
                .clone()
                .unwrap_or_else(|| get_cwd().to_string_lossy().to_string());
            let workspace_uri = url::Url::from_file_path(&workspace_folder)
                .ok()
                .map(|u| u.to_string())
                .unwrap_or_else(|| format!("file://{}", workspace_folder));

            let init_params = serde_json::json!({
                "processId": std::process::id(),
                "initializationOptions": self.config.initialization_options.clone().unwrap_or(serde_json::json!({})),
                "workspaceFolders": [
                    {
                        "uri": workspace_uri,
                        "name": std::path::Path::new(&workspace_folder)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| workspace_folder.clone()),
                    }
                ],
                "rootPath": workspace_folder,
                "rootUri": workspace_uri,
                "capabilities": {
                    "workspace": {
                        "configuration": false,
                        "workspaceFolders": false,
                    },
                    "textDocument": {
                        "synchronization": {
                            "dynamicRegistration": false,
                            "willSave": false,
                            "willSaveWaitUntil": false,
                            "didSave": true,
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true,
                            "tagSupport": { "valueSet": [1, 2] },
                            "versionSupport": false,
                            "codeDescriptionSupport": true,
                            "dataSupport": false,
                        },
                        "hover": {
                            "dynamicRegistration": false,
                            "contentFormat": ["markdown", "plaintext"],
                        },
                        "definition": {
                            "dynamicRegistration": false,
                            "linkSupport": true,
                        },
                        "references": {
                            "dynamicRegistration": false,
                        },
                        "documentSymbol": {
                            "dynamicRegistration": false,
                            "hierarchicalDocumentSymbolSupport": true,
                        },
                        "callHierarchy": {
                            "dynamicRegistration": false,
                        },
                    },
                    "general": {
                        "positionEncodings": ["utf-16"],
                    },
                },
            });

            // Initialize the server
            let client = self.client.clone();
            let init_handle = tokio::spawn(async move { client.initialize(init_params).await });

            if let Some(startup_timeout) = self.config.startup_timeout {
                tokio::time::timeout(
                    std::time::Duration::from_millis(startup_timeout),
                    init_handle,
                )
                .await
                .map_err(|_| {
                    format!(
                        "LSP server '{}' timed out after {}ms during initialization",
                        self.name, startup_timeout
                    )
                })?
                .map_err(|e| format!("Init join error: {}", e))?;
            } else {
                init_handle.await.map_err(|e| format!("Init join error: {}", e))??;
            }

            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await;

        match result {
            Ok(()) => {
                *self.internal.state.lock().unwrap() = LspServerState::Running;
                *self.internal.start_time.lock().unwrap() = Some(std::time::SystemTime::now());
                self.internal.crash_recovery_count.store(0, Ordering::SeqCst);
                log_for_debugging(
                    &format!("[LSP] Server instance started: {}", self.name),
                    DebugLogLevel::Debug,
                )
            }
            Err(e) => {
                // Clean up on failure
                let _ = self.client.stop().await;
                *self.internal.state.lock().unwrap() = LspServerState::Error;
                *self.internal.last_error.lock().unwrap() = Some(e.to_string());
                log_for_debugging(
                    &format!("[LSP] Failed to start '{}': {}", self.name, e),
                    DebugLogLevel::Error,
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Stop the server gracefully.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state = self.internal.state.lock().unwrap().clone();
        if state == LspServerState::Stopped || state == LspServerState::Stopping {
            return Ok(());
        }

        *self.internal.state.lock().unwrap() = LspServerState::Stopping;

        self.client.stop().await?;

        *self.internal.state.lock().unwrap() = LspServerState::Stopped;
        log_for_debugging(
            &format!("[LSP] Server instance stopped: {}", self.name),
            DebugLogLevel::Debug,
        );

        Ok(())
    }

    /// Manually restart the server by stopping and starting it.
    ///
    /// Increments restart_count and enforces max_restarts limit.
    pub async fn restart(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Stop first
        if let Err(e) = self.stop().await {
            let err = format!(
                "Failed to stop LSP server '{}' during restart: {}",
                self.name,
                e.to_string()
            );
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        self.internal.restart_count.fetch_add(1, Ordering::SeqCst);
        let restart_count = self.internal.restart_count.load(Ordering::SeqCst);
        let max_restarts = self.config.max_restarts.unwrap_or(3);

        if restart_count > max_restarts {
            let err = format!(
                "Max restart attempts ({}) exceeded for server '{}'",
                max_restarts, self.name
            );
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        // Start again
        if let Err(e) = self.start().await {
            let err = format!(
                "Failed to start LSP server '{}' during restart (attempt {}/{}): {}",
                self.name,
                restart_count,
                max_restarts,
                e.to_string()
            );
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        Ok(())
    }

    /// Send an LSP request to the server with retry logic for transient errors.
    ///
    /// Automatically retries on "content modified" errors (code -32801) which occur
    /// when servers like rust-analyzer are still indexing.
    pub async fn send_request(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_healthy() {
            let state = self.internal.state.lock().unwrap().clone();
            let last_err = self.internal.last_error.lock().unwrap().clone();
            let err = format!(
                "Cannot send request to LSP server '{}': server is {}{}",
                self.name,
                state,
                last_err
                    .map(|e| format!(", last error: {}", e))
                    .unwrap_or_default()
            );
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        let mut last_error: Option<String> = None;

        for attempt in 0..=MAX_RETRIES_FOR_TRANSIENT_ERRORS {
            match self.client.send_request_raw(method.clone(), params.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let err_msg = e.to_string();
                    last_error = Some(err_msg.clone());

                    // Check if this is a transient "content modified" error
                    // We check the error message since Rust doesn't have instanceof
                    if err_msg.contains(&LSP_ERROR_CONTENT_MODIFIED.to_string())
                        && attempt < MAX_RETRIES_FOR_TRANSIENT_ERRORS
                    {
                        let delay = RETRY_BASE_DELAY_MS * 2_u64.pow(attempt);
                        log_for_debugging(
                            &format!(
                                "[LSP] Request '{}' to '{}' got ContentModified error, \
                                 retrying in {}ms (attempt {}/{})...",
                                method,
                                self.name,
                                delay,
                                attempt + 1,
                                MAX_RETRIES_FOR_TRANSIENT_ERRORS
                            ),
                            DebugLogLevel::Debug,
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }

                    break;
                }
            }
        }

        let err = format!(
            "LSP request '{}' failed for server '{}': {}",
            method,
            self.name,
            last_error.unwrap_or_else(|| "unknown error".to_string())
        );
        log_for_debugging(&err, DebugLogLevel::Error);
        Err(err.into())
    }

    /// Send a notification to the LSP server (fire-and-forget).
    pub async fn send_notification(
        &self,
        method: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_healthy() {
            let state = self.internal.state.lock().unwrap().clone();
            let err =
                format!("Cannot send notification to LSP server '{}': server is {}", self.name, state);
            log_for_debugging(&err, DebugLogLevel::Error);
            return Err(err.into());
        }

        self.client
            .send_notification(method.clone(), params)
            .await
            .map_err(|e| {
                let err = format!(
                    "LSP notification '{}' failed for server '{}': {}",
                    method,
                    self.name,
                    e.to_string()
                );
                log_for_debugging(&err, DebugLogLevel::Error);
                Box::<dyn std::error::Error + Send + Sync>::from(err)
            })
    }

    /// Register a handler for LSP notifications from the server.
    pub fn on_notification(&self, method: String, handler: NotificationHandler) {
        self.client.on_notification(method, handler);
    }

    /// Register a handler for LSP requests from the server.
    pub fn on_request(&self, method: String, handler: RequestHandlerFn) {
        self.client.on_request(method, handler);
    }
}

/// Creates and manages a single LSP server instance.
///
/// Uses factory function pattern with closures for state encapsulation.
/// Provides state tracking, health monitoring, and request forwarding.
///
/// # Arguments
/// * `name` - Unique identifier for this server instance
/// * `config` - Server configuration including command, args, and limits
///
/// # Example
/// ```ignore
/// let instance = create_lsp_server_instance("my-server", config);
/// instance.start().await?;
/// let result = instance.send_request("textDocument/definition".into(), params).await?;
/// instance.stop().await?;
/// ```
pub fn create_lsp_server_instance(name: &str, config: LspServerConfig) -> LspServerInstance {
    // Validate unimplemented fields
    // (Rust doesn't have the same optional fields, but we validate structure)

    // Create Arc-wrapped state for sharing with crash callback
    let state = Arc::new(Mutex::new(LspServerState::Stopped));
    let last_error = Arc::new(Mutex::new(None));
    let crash_recovery_count = Arc::new(AtomicU32::new(0));

    let internal = Arc::new(LspServerInternal {
        state: state.clone(),
        start_time: Mutex::new(None),
        last_error: last_error.clone(),
        restart_count: AtomicU32::new(0),
        crash_recovery_count: crash_recovery_count.clone(),
    });

    let state_clone = state.clone();
    let last_error_clone = last_error.clone();
    let crash_count = crash_recovery_count.clone();

    let client = Arc::new(create_lsp_client(
        name,
        Some(Box::new(
            move |_error: Box<dyn std::error::Error + Send + Sync>| {
                *state_clone.lock().unwrap() = LspServerState::Error;
                *last_error_clone.lock().unwrap() = Some(_error.to_string());
                crash_count.fetch_add(1, Ordering::SeqCst);
            },
        )),
    ));

    LspServerInstance {
        name: name.to_string(),
        config,
        client,
        internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_error_codes() {
        assert_eq!(LSP_ERROR_CONTENT_MODIFIED, -32801);
        assert_eq!(MAX_RETRIES_FOR_TRANSIENT_ERRORS, 3);
    }
}
