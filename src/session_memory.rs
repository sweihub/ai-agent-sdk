//! Session Memory - automatic conversation summarization
//!
//! Ported from ~/claudecode/openclaudecode/src/services/SessionMemory/sessionMemory.ts
//!
//! Session memory automatically maintains a markdown file with notes about the current conversation.
//! It runs periodically in the background using a forked subagent to extract key information
//! without interrupting the main conversation flow.

use crate::AgentError;
use crate::constants::env::system;
use crate::types::*;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default configuration for session memory
pub const DEFAULT_SESSION_MEMORY_CONFIG: SessionMemoryConfig = SessionMemoryConfig {
    minimum_message_tokens_to_init: 10000,
    minimum_tokens_between_update: 5000,
    tool_calls_between_updates: 3,
};

/// Session memory configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionMemoryConfig {
    /// Minimum context window tokens before initializing session memory
    pub minimum_message_tokens_to_init: u32,
    /// Minimum context window growth (in tokens) between updates
    pub minimum_tokens_between_update: u32,
    /// Number of tool calls between session memory updates
    pub tool_calls_between_updates: u32,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        DEFAULT_SESSION_MEMORY_CONFIG
    }
}

/// Session memory state
pub struct SessionMemoryState {
    config: Mutex<SessionMemoryConfig>,
    initialized: Mutex<bool>,
    tokens_at_last_extraction: AtomicU64,
    /// Last summarized message index (not UUID since Message lacks id field)
    last_summarized_index: Mutex<Option<usize>>,
    extraction_in_progress: Mutex<bool>,
}

impl SessionMemoryState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(DEFAULT_SESSION_MEMORY_CONFIG),
            initialized: Mutex::new(false),
            tokens_at_last_extraction: AtomicU64::new(0),
            last_summarized_index: Mutex::new(None),
            extraction_in_progress: Mutex::new(false),
        }
    }

    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    pub fn mark_initialized(&self) {
        *self.initialized.lock().unwrap() = true;
    }

    pub fn get_config(&self) -> SessionMemoryConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn set_config(&self, config: SessionMemoryConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_tokens_at_last_extraction(&self) -> u64 {
        self.tokens_at_last_extraction.load(Ordering::SeqCst)
    }

    pub fn set_tokens_at_last_extraction(&self, tokens: u64) {
        self.tokens_at_last_extraction
            .store(tokens, Ordering::SeqCst);
    }

    pub fn get_last_summarized_index(&self) -> Option<usize> {
        *self.last_summarized_index.lock().unwrap()
    }

    pub fn set_last_summarized_index(&self, index: Option<usize>) {
        *self.last_summarized_index.lock().unwrap() = index;
    }

    pub fn is_extraction_in_progress(&self) -> bool {
        *self.extraction_in_progress.lock().unwrap()
    }

    pub fn start_extraction(&self) {
        *self.extraction_in_progress.lock().unwrap() = true;
    }

    pub fn end_extraction(&self) {
        *self.extraction_in_progress.lock().unwrap() = false;
    }
}

impl Default for SessionMemoryState {
    fn default() -> Self {
        Self::new()
    }
}

/// Global session memory state
static SESSION_MEMORY_STATE: LazyLock<SessionMemoryState> = LazyLock::new(SessionMemoryState::new);

/// Get the session memory state
pub fn get_session_memory_state() -> &'static SessionMemoryState {
    &SESSION_MEMORY_STATE
}

/// Get the session memory directory
pub fn get_session_memory_dir() -> PathBuf {
    let home = std::env::var(system::HOME)
        .or_else(|_| std::env::var(system::USERPROFILE))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".open-agent-sdk")
        .join("session_memory")
}

/// Get the session memory file path
pub fn get_session_memory_path() -> PathBuf {
    get_session_memory_dir().join("notes.md")
}

/// Check if session memory has been initialized
pub fn is_session_memory_initialized() -> bool {
    SESSION_MEMORY_STATE.is_initialized()
}

/// Mark session memory as initialized
pub fn mark_session_memory_initialized() {
    SESSION_MEMORY_STATE.mark_initialized();
}

/// Get current session memory configuration
pub fn get_session_memory_config() -> SessionMemoryConfig {
    SESSION_MEMORY_STATE.get_config()
}

/// Set session memory configuration
pub fn set_session_memory_config(config: SessionMemoryConfig) {
    SESSION_MEMORY_STATE.set_config(config);
}

/// Get the last summarized message index
pub fn get_last_summarized_message_id() -> Option<usize> {
    SESSION_MEMORY_STATE.get_last_summarized_index()
}

/// Set the last summarized message index
pub fn set_last_summarized_message_id(message_id: Option<usize>) {
    SESSION_MEMORY_STATE.set_last_summarized_index(message_id);
}

/// Check if we've met the initialization threshold
pub fn has_met_initialization_threshold(current_token_count: u64) -> bool {
    let config = get_session_memory_config();
    current_token_count >= config.minimum_message_tokens_to_init as u64
}

/// Check if we've met the update threshold
pub fn has_met_update_threshold(current_token_count: u64) -> bool {
    let config = get_session_memory_config();
    let tokens_at_last = SESSION_MEMORY_STATE.get_tokens_at_last_extraction();
    let tokens_since_last = current_token_count.saturating_sub(tokens_at_last);
    tokens_since_last >= config.minimum_tokens_between_update as u64
}

/// Get tool calls between updates
pub fn get_tool_calls_between_updates() -> u32 {
    get_session_memory_config().tool_calls_between_updates
}

/// Record token count at extraction time
pub fn record_extraction_token_count(token_count: u64) {
    SESSION_MEMORY_STATE.set_tokens_at_last_extraction(token_count);
}

/// Count tool calls since a given message index
pub fn count_tool_calls_since(messages: &[Message], since_index: Option<usize>) -> usize {
    let mut tool_call_count = 0;
    let start_idx = since_index.unwrap_or(0);

    for (i, message) in messages.iter().enumerate() {
        if i < start_idx {
            continue;
        }

        if message.role == MessageRole::Assistant {
            // Count tool calls in this message
            // In Rust we store content as string, so we approximate
            if message.content.contains("tool_use") || message.tool_calls.is_some() {
                tool_call_count += 1;
            }
        }
    }

    tool_call_count
}

/// Check if we should extract memory based on thresholds
pub fn should_extract_memory(messages: &[Message]) -> bool {
    // Estimate token count
    let current_token_count = estimate_message_tokens(messages);

    // Check initialization threshold
    if !is_session_memory_initialized() {
        if !has_met_initialization_threshold(current_token_count) {
            return false;
        }
        mark_session_memory_initialized();
    }

    // Check token threshold
    let has_met_token_threshold = has_met_update_threshold(current_token_count);

    // Check tool call threshold
    let last_index = get_last_summarized_message_id();
    let tool_calls_since_last = count_tool_calls_since(messages, last_index);
    let has_met_tool_call_threshold =
        tool_calls_since_last >= get_tool_calls_between_updates() as usize;

    // Check if last assistant turn has tool calls (unsafe to extract)
    let has_tool_calls_in_last_turn = has_tool_calls_in_last_assistant_turn(messages);

    // Trigger extraction when:
    // 1. Both thresholds are met (tokens AND tool calls), OR
    // 2. No tool calls in last turn AND token threshold is met
    let should_extract = (has_met_token_threshold && has_met_tool_call_threshold)
        || (has_met_token_threshold && !has_tool_calls_in_last_turn);

    if should_extract {
        // Store the last message index
        if !messages.is_empty() {
            set_last_summarized_message_id(Some(messages.len() - 1));
        }
    }

    should_extract
}

/// Check if last assistant turn has tool calls
fn has_tool_calls_in_last_assistant_turn(messages: &[Message]) -> bool {
    // Find last assistant message and check for tool calls
    for message in messages.iter().rev() {
        if message.role == MessageRole::Assistant {
            // Check for tool calls
            if message.tool_calls.is_some() {
                return true;
            }
            // Also check content for tool_use blocks
            if message.content.contains("tool_use") {
                return true;
            }
            // Found last assistant message without tool calls
            return false;
        }
    }
    false
}

/// Estimate token count for messages
fn estimate_message_tokens(messages: &[Message]) -> u64 {
    // Simple estimation: ~4 characters per token
    let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
    (total_chars / 4) as u64
}

/// Get session memory content from file
pub async fn get_session_memory_content() -> Result<Option<String>, AgentError> {
    let path = get_session_memory_path();

    if !path.exists() {
        return Ok(None);
    }

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AgentError::Io(e)),
    }
}

/// Initialize session memory file with template
pub async fn init_session_memory_file() -> Result<String, AgentError> {
    let dir = get_session_memory_dir();
    let path = get_session_memory_path();

    // Create directory
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(AgentError::Io)?;

    // Check if file already exists
    if !path.exists() {
        // Create with template
        let template = get_session_memory_template();
        tokio::fs::write(&path, template)
            .await
            .map_err(AgentError::Io)?;
    }

    // Return current content
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(content),
        Err(e) => Err(AgentError::Io(e)),
    }
}

/// Get session memory template
fn get_session_memory_template() -> String {
    r#"# Session Notes

This file contains automatically extracted notes about the current conversation.

## Key Points

-

## Decisions Made

-

## Open Items

-

## Context

"#
    .to_string()
}

/// Manual extraction result
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ManualExtractionResult {
    pub success: bool,
    pub memory_path: Option<String>,
    pub error: Option<String>,
}

/// Wait for any in-progress extraction to complete
pub async fn wait_for_session_memory_extraction() {
    // In Rust, this would need async coordination
    // For now, simplified implementation
    while SESSION_MEMORY_STATE.is_extraction_in_progress() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

/// Reset session memory state (for testing)
pub fn reset_session_memory_state() {
    SESSION_MEMORY_STATE.set_config(DEFAULT_SESSION_MEMORY_CONFIG);
    SESSION_MEMORY_STATE.set_tokens_at_last_extraction(0);
    SESSION_MEMORY_STATE.set_last_summarized_index(None);
    *SESSION_MEMORY_STATE.initialized.lock().unwrap() = false;
    *SESSION_MEMORY_STATE.extraction_in_progress.lock().unwrap() = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DEFAULT_SESSION_MEMORY_CONFIG;
        assert_eq!(config.minimum_message_tokens_to_init, 10000);
        assert_eq!(config.minimum_tokens_between_update, 5000);
        assert_eq!(config.tool_calls_between_updates, 3);
    }

    #[test]
    fn test_session_memory_state() {
        let state = SessionMemoryState::new();
        assert!(!state.is_initialized());

        state.mark_initialized();
        assert!(state.is_initialized());
    }

    #[test]
    fn test_has_met_initialization_threshold() {
        reset_session_memory_state();
        assert!(has_met_initialization_threshold(10000));
        assert!(!has_met_initialization_threshold(9999));
    }

    #[test]
    fn test_has_met_update_threshold() {
        reset_session_memory_state();
        record_extraction_token_count(5000);
        assert!(has_met_update_threshold(10000));
        assert!(!has_met_update_threshold(7499));
    }
}
