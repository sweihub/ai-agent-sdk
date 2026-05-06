// Source: ~/claudecode/openclaudecode/src/services/SessionMemory/sessionMemoryUtils.ts
//! Session memory utility functions — state management, thresholds, config.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default extraction wait timeout (15 seconds)
const EXTRACTION_WAIT_TIMEOUT: Duration = Duration::from_secs(15);

/// Stale extraction threshold (1 minute)
const EXTRACTION_STALE_THRESHOLD: Duration = Duration::from_secs(60);

/// Configuration for session memory extraction thresholds
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionMemoryConfig {
    /// Minimum context window tokens before initializing session memory
    pub minimum_message_tokens_to_init: u64,
    /// Minimum context window growth between session memory updates
    pub minimum_tokens_between_update: u64,
    /// Number of tool calls between session memory updates
    pub tool_calls_between_updates: u64,
}

/// Default configuration for session memory (backward compat constant)
pub const DEFAULT_SESSION_MEMORY_CONFIG: SessionMemoryConfig = SessionMemoryConfig {
    minimum_message_tokens_to_init: 10_000,
    minimum_tokens_between_update: 5_000,
    tool_calls_between_updates: 3,
};

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            minimum_message_tokens_to_init: 10_000,
            minimum_tokens_between_update: 5_000,
            tool_calls_between_updates: 3,
        }
    }
}

struct SessionMemoryState {
    config: SessionMemoryConfig,
    /// UUID of the last message summarized by session memory
    last_summarized_message_id: Option<String>,
    /// When the current extraction started (if any)
    extraction_started_at: Option<Instant>,
    /// Token count at last extraction (for measuring context growth)
    tokens_at_last_extraction: u64,
    /// Whether session memory has been initialized
    session_memory_initialized: bool,
    /// File path to the session memory notes file
    memory_path: Option<String>,
}

impl Default for SessionMemoryState {
    fn default() -> Self {
        Self {
            config: SessionMemoryConfig::default(),
            last_summarized_message_id: None,
            extraction_started_at: None,
            tokens_at_last_extraction: 0,
            session_memory_initialized: false,
            memory_path: None,
        }
    }
}

static STATE: std::sync::LazyLock<Mutex<SessionMemoryState>> =
    std::sync::LazyLock::new(|| Mutex::new(SessionMemoryState::default()));

/// Get the message ID up to which the session memory is current
pub fn get_last_summarized_message_id() -> Option<String> {
    STATE.lock().unwrap().last_summarized_message_id.clone()
}

/// Set the last summarized message ID (called from session_memory.rs)
pub fn set_last_summarized_message_id(id: Option<&str>) {
    let mut state = STATE.lock().unwrap();
    state.last_summarized_message_id = id.map(str::to_string);
}

/// Mark extraction as started (called from session_memory.rs)
pub fn mark_extraction_started() {
    STATE.lock().unwrap().extraction_started_at = Some(Instant::now());
}

/// Mark extraction as completed (called from session_memory.rs)
pub fn mark_extraction_completed() {
    STATE.lock().unwrap().extraction_started_at = None;
}

/// Wait for any in-progress session memory extraction to complete (with timeout).
/// Returns immediately if no extraction is in progress or if extraction is stale.
pub async fn wait_for_session_memory_extraction() {
    let start = Instant::now();
    loop {
        let started = { STATE.lock().unwrap().extraction_started_at };
        match started {
            None => return,
            Some(t) if t.elapsed() > EXTRACTION_STALE_THRESHOLD => return,
            _ => {}
        }
        if start.elapsed() > EXTRACTION_WAIT_TIMEOUT {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Set the session memory configuration
pub fn set_session_memory_config(partial: SessionMemoryConfig) {
    let mut state = STATE.lock().unwrap();
    if partial.minimum_message_tokens_to_init > 0 {
        state.config.minimum_message_tokens_to_init = partial.minimum_message_tokens_to_init;
    }
    if partial.minimum_tokens_between_update > 0 {
        state.config.minimum_tokens_between_update = partial.minimum_tokens_between_update;
    }
    if partial.tool_calls_between_updates > 0 {
        state.config.tool_calls_between_updates = partial.tool_calls_between_updates;
    }
}

/// Get the current session memory configuration
pub fn get_session_memory_config() -> SessionMemoryConfig {
    STATE.lock().unwrap().config.clone()
}

/// Record the context size at the time of extraction.
/// Used to measure context growth for minimumTokensBetweenUpdate threshold.
pub fn record_extraction_token_count(current_token_count: u64) {
    STATE.lock().unwrap().tokens_at_last_extraction = current_token_count;
}

/// Check if session memory has been initialized (met minimumTokensToInit threshold)
pub fn is_session_memory_initialized() -> bool {
    STATE.lock().unwrap().session_memory_initialized
}

/// Mark session memory as initialized
pub fn mark_session_memory_initialized() {
    STATE.lock().unwrap().session_memory_initialized = true;
}

/// Check if we've met the threshold to initialize session memory.
/// Uses total context window tokens (same as autocompact) for consistent behavior.
pub fn has_met_initialization_threshold(current_token_count: u64) -> bool {
    let state = STATE.lock().unwrap();
    current_token_count >= state.config.minimum_message_tokens_to_init
}

/// Check if we've met the threshold for the next update.
/// Measures actual context window growth since last extraction.
pub fn has_met_update_threshold(current_token_count: u64) -> bool {
    let state = STATE.lock().unwrap();
    let tokens_since = current_token_count.saturating_sub(state.tokens_at_last_extraction);
    tokens_since >= state.config.minimum_tokens_between_update
}

/// Get the configured number of tool calls between updates
pub fn get_tool_calls_between_updates() -> u64 {
    STATE.lock().unwrap().config.tool_calls_between_updates
}

/// Get the session memory file path
pub fn get_session_memory_path() -> Option<String> {
    STATE.lock().unwrap().memory_path.clone()
}

/// Set the session memory file path (called during file setup)
pub fn set_session_memory_path(path: String) {
    STATE.lock().unwrap().memory_path = Some(path);
}

/// Reset session memory state (useful for testing)
pub fn reset_session_memory_state() {
    *STATE.lock().unwrap() = SessionMemoryState::default();
}

/// Get the extraction wait timeout
pub fn get_extraction_wait_timeout() -> Duration {
    EXTRACTION_WAIT_TIMEOUT
}

/// Get the extraction stale threshold
pub fn get_extraction_stale_threshold() -> Duration {
    EXTRACTION_STALE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        reset_session_memory_state();
        let config = get_session_memory_config();
        assert_eq!(config.minimum_message_tokens_to_init, 10_000);
        assert_eq!(config.minimum_tokens_between_update, 5_000);
        assert_eq!(config.tool_calls_between_updates, 3);
    }

    #[test]
    fn test_initialization_threshold() {
        reset_session_memory_state();
        assert!(!is_session_memory_initialized());
        assert!(!has_met_initialization_threshold(5_000));
        assert!(has_met_initialization_threshold(10_000));
    }

    #[test]
    #[serial_test::serial]
    fn test_update_threshold() {
        reset_session_memory_state();
        record_extraction_token_count(10_000);
        assert!(!has_met_update_threshold(12_000));
        assert!(has_met_update_threshold(15_000));
    }

    #[test]
    #[serial_test::serial]
    fn test_extraction_tracking() {
        reset_session_memory_state();
        assert!(get_last_summarized_message_id().is_none());
        set_last_summarized_message_id(Some("msg_123"));
        assert_eq!(get_last_summarized_message_id(), Some("msg_123".to_string()));
    }

    #[test]
    fn test_mark_extraction() {
        reset_session_memory_state();
        mark_extraction_started();
        // Subsequent call to mark_extraction_completed clears it
        mark_extraction_completed();
    }
}
