//! Type definitions module for ai-agent-sdk
//!
//! Contains all type definitions translated from the Claude Code TypeScript source.

// API types (from utils/filePersistence/types.ts) - original types used by existing code
pub mod api_types;

// New types from ~/claudecode/openclaudecode/src/types/
pub mod command;
pub mod connector_text;
pub mod file_suggestion;
pub mod hooks;
pub mod ids;
pub mod logs;
pub mod message;
pub mod message_queue_types;
pub mod notebook;
pub mod permissions;
pub mod plugin;
pub mod status_line;
pub mod task_types;
pub mod text_input_types;
pub mod tools;
pub mod utils;

// Re-exports for backward compatibility with existing code
pub use api_types::*;
pub use tools::ToolRender;
