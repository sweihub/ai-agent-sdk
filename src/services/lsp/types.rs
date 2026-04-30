// Source: ~/claudecode/openclaudecode/src/services/lsp/types.ts
//! LSP (Language Server Protocol) shared types

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// LSP server configuration with all fields from plugin config
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerConfig {
    /// Command to spawn the language server process
    pub command: String,
    /// Arguments for the language server command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set for the server process
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Mapping from file extensions (e.g., ".ts") to language IDs (e.g., "typescript")
    #[serde(default)]
    pub extension_to_language: HashMap<String, String>,
    /// Working directory for the server process
    #[serde(default)]
    pub workspace_folder: Option<String>,
    /// Server-specific initialization options passed during LSP initialize
    #[serde(default)]
    pub initialization_options: Option<serde_json::Value>,
    /// Timeout in milliseconds for server startup and initialization
    #[serde(default)]
    pub startup_timeout: Option<u64>,
    /// Maximum number of crash recovery attempts (default: 3)
    #[serde(default)]
    pub max_restarts: Option<u32>,
}

/// LSP server state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl Default for LspServerState {
    fn default() -> Self {
        LspServerState::Stopped
    }
}

impl std::fmt::Display for LspServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            LspServerState::Stopped => write!(f, "stopped"),
            LspServerState::Starting => write!(f, "starting"),
            LspServerState::Running => write!(f, "running"),
            LspServerState::Stopping => write!(f, "stopping"),
            LspServerState::Error => write!(f, "error"),
        }
    }
}

impl LspServerState {
    /// Convert from string representation
    pub fn from_str(s: &str) -> Self {
        match s {
            "stopped" => LspServerState::Stopped,
            "starting" => LspServerState::Starting,
            "running" => LspServerState::Running,
            "stopping" => LspServerState::Stopping,
            "error" => LspServerState::Error,
            _ => LspServerState::Stopped,
        }
    }
}

/// Options for starting an LSP server process
#[derive(Debug, Clone, Default)]
pub struct LspStartOptions {
    /// Environment variables to set for the process
    pub env: Option<HashMap<String, String>>,
    /// Working directory for the process
    pub cwd: Option<String>,
}
