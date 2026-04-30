// Source: ~/claudecode/openclaudecode/src/services/lsp/
//! LSP (Language Server Protocol) service module
//!
//! Provides LSP server management, client communication, and diagnostic handling.
//!
//! Architecture:
//! - [lsp_client]: JSON-RPC client for communicating with a single language server process
//! - [lsp_server_instance]: Lifecycle management for a single LSP server
//! - [lsp_server_manager]: Multi-server manager with file-to-server routing
//! - [manager]: Public singleton API (initialize, get manager, shutdown)
//! - [config]: LSP server configuration loading from plugins
//! - [types]: Shared types (config, state)
//! - [passive_feedback]: Diagnostic notification handling
//! - [lsp_diagnostic_registry]: Diagnostic tracking (stub)

pub mod config;
pub mod lsp_client;
pub mod lsp_server_instance;
pub mod lsp_server_manager;
pub mod manager;
pub mod types;

// Internal modules
mod lsp_diagnostic_registry;
mod passive_feedback;

// Re-exports for convenience
pub use config::{get_all_lsp_servers, LspServersResult};
pub use lsp_client::{create_lsp_client, LspClient};
pub use lsp_server_instance::{create_lsp_server_instance, LspServerInstance};
pub use lsp_server_manager::{create_lsp_server_manager, LspServerManager};
pub use manager::{
    get_initialization_status, get_lsp_server_manager, initialize_lsp_server_manager,
    is_lsp_connected, reinitialize_lsp_server_manager, reset_lsp_manager_for_testing,
    shutdown_lsp_server_manager, wait_for_initialization, InitializationStatus,
};
pub use passive_feedback::{
    DiagnosticFile, DiagnosticPosition, DiagnosticRange, DiagnosticSeverity, FormattedDiagnostic,
    HandlerRegistrationResult, LspDiagnostic, LspPosition, LspRange, PublishDiagnosticsParams,
    format_diagnostics_for_attachment, map_lsp_severity_to_string, register_lsp_notification_handlers,
    uri_to_file_path,
};
pub use types::{LspServerConfig, LspServerState, LspStartOptions};
