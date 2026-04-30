// Source: ~/claudecode/openclaudecode/src/services/lsp/manager.ts
//! LSP Manager - public API for the LSP subsystem
//!
//! Provides the singleton LSP server manager instance, initialization state
//! management, and the public API for accessing LSP functionality.

use std::sync::Arc;

use crate::services::lsp::lsp_server_manager::{create_lsp_server_manager, LspServerManager};
use crate::services::lsp::passive_feedback::register_lsp_notification_handlers;
use crate::services::lsp::types::LspServerState;
use crate::utils::debug::{log_for_debugging, DebugLogLevel};
use crate::utils::env_utils::is_bare_mode;

/// Initialization state of the LSP server manager
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitializationState {
    NotStarted,
    Pending,
    Success,
    Failed,
}

/// Internal state for the singleton manager
struct LspManagerState {
    manager: Option<LspServerManager>,
    state: InitializationState,
    last_error: Option<String>,
    generation: u64,
    init_notify: Arc<tokio::sync::Notify>,
}

impl LspManagerState {
    fn new() -> Self {
        Self {
            manager: None,
            state: InitializationState::NotStarted,
            last_error: None,
            generation: 0,
            init_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

/// Global singleton instance of the LSP server manager.
static LSP_MANAGER_STATE: once_cell::sync::Lazy<std::sync::Mutex<LspManagerState>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(LspManagerState::new()));

/// Test-only sync reset. Clears the module-scope singleton state so that
/// reinitialize_lsp_server_manager() early-returns on 'not-started' in tests.
pub fn reset_lsp_manager_for_testing() {
    let mut state = LSP_MANAGER_STATE.lock().unwrap();
    state.state = InitializationState::NotStarted;
    state.last_error = None;
    state.generation += 1;
}

/// Get the singleton LSP server manager instance.
///
/// Returns None if not yet initialized, initialization failed, or still pending.
/// Callers should check for None and handle gracefully, as initialization happens
/// asynchronously during application startup. Use [get_initialization_status] to
/// distinguish between pending, failed, and not-started states.
pub fn get_lsp_server_manager() -> Option<LspServerManager> {
    let state = LSP_MANAGER_STATE.lock().unwrap();
    if state.state == InitializationState::Failed {
        return None;
    }
    state.manager.clone()
}

/// Initialization status of the LSP server manager.
#[derive(Debug, Clone)]
pub enum InitializationStatus {
    /// Initialization has not been attempted
    NotStarted,
    /// Initialization is in progress
    Pending,
    /// Initialization completed successfully
    Success,
    /// Initialization failed with the given error
    Failed { error: String },
}

/// Get the current initialization status of the LSP server manager.
pub fn get_initialization_status() -> InitializationStatus {
    let state = LSP_MANAGER_STATE.lock().unwrap();
    match state.state {
        InitializationState::Failed => InitializationStatus::Failed {
            error: state
                .last_error
                .clone()
                .unwrap_or_else(|| "Initialization failed".to_string()),
        },
        InitializationState::NotStarted => InitializationStatus::NotStarted,
        InitializationState::Pending => InitializationStatus::Pending,
        InitializationState::Success => InitializationStatus::Success,
    }
}

/// Check whether at least one language server is connected and healthy.
///
/// Used by LSPTool.is_enabled() to determine whether the tool should be available.
pub fn is_lsp_connected() -> bool {
    let state = LSP_MANAGER_STATE.lock().unwrap();
    if state.state == InitializationState::Failed {
        return false;
    }
    let Some(manager) = state.manager.as_ref() else {
        return false;
    };
    let servers = manager.get_all_servers();
    if servers.is_empty() {
        return false;
    }
    servers.values().any(|s| s.state() != LspServerState::Error)
}

/// Initialize the LSP server manager singleton.
///
/// This function is called during application startup. It creates the manager
/// instance, then starts async initialization (loading LSP configs) in the
/// background without blocking the startup process.
///
/// Safe to call multiple times - will only initialize once (idempotent).
/// If initialization previously failed, calling again will retry.
pub fn initialize_lsp_server_manager() {
    // --bare / SIMPLE: no LSP. LSP is for editor integration.
    if is_bare_mode() {
        return;
    }

    log_for_debugging(
        "[LSP MANAGER] initialize_lsp_server_manager() called",
        DebugLogLevel::Debug,
    );

    let mut state = LSP_MANAGER_STATE.lock().unwrap();

    // Skip if already initialized or currently initializing
    if state.manager.is_some() && state.state != InitializationState::Failed {
        log_for_debugging(
            "[LSP MANAGER] Already initialized or initializing, skipping",
            DebugLogLevel::Debug,
        );
        return;
    }

    // Reset state for retry if previous initialization failed
    if state.state == InitializationState::Failed {
        state.manager = None;
        state.last_error = None;
    }

    // Create the manager instance and mark as pending
    state.manager = Some(create_lsp_server_manager());
    state.state = InitializationState::Pending;
    log_for_debugging(
        "[LSP MANAGER] Created manager instance, state=pending",
        DebugLogLevel::Debug,
    );

    // Increment generation to invalidate any pending initializations
    state.generation += 1;
    let current_generation = state.generation;

    // Take a clone of the manager reference for the async task
    // We take ownership of the manager to avoid lifetime issues
    let manager = state.manager.take().unwrap();

    drop(state); // Release the lock before spawning async work

    log_for_debugging(
        &format!(
            "[LSP MANAGER] Starting async initialization (generation {})",
            current_generation
        ),
        DebugLogLevel::Debug,
    );

    // Start initialization asynchronously without blocking
    tokio::spawn(async move {
        let cur_gen = current_generation;
        let init_result = manager.initialize().await;

        // Determine action while holding lock, then release before any await.
        // Returns `Some(manager)` when it should be shut down (stale init),
        // `None` when the manager was stored in state or dropped on error.
        let manager_to_shutdown: Option<LspServerManager> = {
            let mut state = LSP_MANAGER_STATE.lock().unwrap();
            match &init_result {
                Ok(()) => {
                    if cur_gen == state.generation && state.state == InitializationState::Pending {
                        state.state = InitializationState::Success;
                        state.manager = Some(manager);
                        state.init_notify.notify_waiters();
                        log_for_debugging(
                            "[LSP MANAGER] LSP server manager initialized successfully",
                            DebugLogLevel::Debug,
                        );
                        if let Some(ref mgr) = state.manager {
                            register_lsp_notification_handlers(mgr);
                        }
                        None
                    } else {
                        Some(manager) // stale init
                    }
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    if cur_gen == state.generation && state.state == InitializationState::Pending {
                        state.state = InitializationState::Failed;
                        state.last_error = Some(err_msg.clone());
                        state.manager = None;
                        state.init_notify.notify_waiters();
                        // manager is dropped here — no servers need cleanup
                        log_for_debugging(
                            &format!(
                                "[LSP MANAGER] Failed to initialize: {}",
                                err_msg
                            ),
                            DebugLogLevel::Error,
                        );
                        None
                    } else {
                        Some(manager) // stale init
                    }
                }
            }
        };

        // Handle stale init outside the lock
        if let Some(stale_manager) = manager_to_shutdown {
            if let Err(e) = stale_manager.shutdown().await {
                log_for_debugging(
                    &format!(
                        "[LSP MANAGER] Stale init shutdown failed: {}",
                        e
                    ),
                    DebugLogLevel::Debug,
                );
            }
        }
    });
}

/// Force re-initialization of the LSP server manager, even after a prior
/// successful init. Called from refresh_active_plugins() after plugin caches
/// are cleared, so newly-loaded plugin LSP servers are picked up.
pub fn reinitialize_lsp_server_manager() {
    let mut state = LSP_MANAGER_STATE.lock().unwrap();

    if state.state == InitializationState::NotStarted {
        // initialize_lsp_server_manager() was never called (e.g. headless path)
        return;
    }

    log_for_debugging(
        "[LSP MANAGER] reinitialize_lsp_server_manager() called",
        DebugLogLevel::Debug,
    );

    // Best-effort shutdown of any running servers on the old instance
    if state.manager.is_some() {
        let old_manager = state.manager.take().unwrap();
        drop(state);
        tokio::spawn(async move {
            if let Err(e) = old_manager.shutdown().await {
                log_for_debugging(
                    &format!(
                        "[LSP MANAGER] Old instance shutdown during reinit failed: {}",
                        e
                    ),
                    DebugLogLevel::Debug,
                );
            }
        });
    }

    // Force the idempotence check to fall through
    let mut state = LSP_MANAGER_STATE.lock().unwrap();
    state.manager = None;
    state.state = InitializationState::NotStarted;
    state.last_error = None;

    drop(state);
    initialize_lsp_server_manager();
}

/// Wait for LSP initialization to complete (success or failure).
///
/// Returns `true` if initialization succeeded, `false` if it failed
/// or was never started. Useful for tools that need LSP but may be
/// called before async initialization completes.
pub async fn wait_for_initialization() -> bool {
    loop {
        match get_initialization_status() {
            InitializationStatus::Success => return true,
            InitializationStatus::Failed { .. } => return false,
            InitializationStatus::NotStarted => return false,
            InitializationStatus::Pending => {
                let notify = {
                    let state = LSP_MANAGER_STATE.lock().unwrap();
                    state.init_notify.clone()
                };
                notify.notified().await;
                // Re-check status after notification
            }
        }
    }
}

/// Shutdown the LSP server manager and clean up resources.
///
/// Stops all running LSP servers and clears internal state.
/// Safe to call when not initialized (no-op).
pub async fn shutdown_lsp_server_manager() {
    let mut state = LSP_MANAGER_STATE.lock().unwrap();
    let manager = state.manager.take();
    drop(state);

    if let Some(manager) = manager {
        if let Err(e) = manager.shutdown().await {
            log_for_debugging(
                &format!("[LSP MANAGER] Failed to shutdown: {}", e),
                DebugLogLevel::Error,
            );
        } else {
            log_for_debugging(
                "[LSP MANAGER] LSP server manager shut down successfully",
                DebugLogLevel::Debug,
            );
        }
    }

    let mut state = LSP_MANAGER_STATE.lock().unwrap();
    state.manager = None;
    state.state = InitializationState::NotStarted;
    state.last_error = None;
    state.generation += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization_status_not_started() {
        reset_lsp_manager_for_testing();
        let status = get_initialization_status();
        assert!(matches!(status, InitializationStatus::NotStarted));
    }

    #[test]
    fn test_get_lsp_server_manager_before_init() {
        reset_lsp_manager_for_testing();
        assert!(get_lsp_server_manager().is_none());
    }

    #[test]
    fn test_is_lsp_connected_before_init() {
        reset_lsp_manager_for_testing();
        assert!(!is_lsp_connected());
    }

    #[test]
    fn test_reset_lsp_manager_for_testing() {
        reset_lsp_manager_for_testing();
        assert!(matches!(
            get_initialization_status(),
            InitializationStatus::NotStarted
        ));
        assert!(get_lsp_server_manager().is_none());
    }
}
