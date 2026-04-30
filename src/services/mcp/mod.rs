//! MCP (Model Context Protocol) service module

pub mod agent_mcp;
pub mod auth;
pub mod mcp_string_utils;
pub mod normalization;
pub mod tool_executor;

// Additional stubs
mod channel_allowlist;
mod channel_notification;
mod channel_permissions;
mod claudeai;
pub mod client;
mod config;
mod elicitation_handler;
mod env_expansion;
mod headers_helper;
mod in_process_transport;
mod mcp_connection_manager;
mod oauth_port;
mod official_registry;
mod sdk_control_transport;
mod types;
mod use_manage_mcp_connections;
mod utils;
mod vscode_sdk_mcp;
mod xaa;
mod xaa_idp_login;

pub use auth::*;
pub use mcp_string_utils::*;
pub use normalization::normalize_name_for_mcp;
pub use types::{ConfigScope, McpServerConnection};
