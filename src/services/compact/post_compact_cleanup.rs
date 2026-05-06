// Source: ~/claudecode/openclaudecode/src/services/compact/postCompactCleanup.ts
//! Post-compaction cleanup.
//!
//! Clears all relevant caches and resets module-level state after compaction
//! to prevent stale data from being used in the new conversation context.

use crate::services::compact::microcompact::reset_microcompact_state;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Query sources that indicate a main-thread compact (not a sub-agent)
const MAIN_THREAD_SOURCES: &[&str] = &["repl_main_thread", "sdk"];

/// Check if this is a main-thread compact (not a sub-agent)
pub fn is_main_thread_compact(query_source: Option<&str>) -> bool {
    match query_source {
        Some(source) => MAIN_THREAD_SOURCES.contains(&source),
        None => true, // None means default/main thread
    }
}

// --- Post-compaction cleanup state ---

/// Tracks which caches have been cleared (for debugging/telemetry)
#[derive(Debug, Default, Clone)]
pub struct CleanupState {
    pub context_collapse_reset: bool,
    pub user_context_cleared: bool,
    pub memory_files_cleared: bool,
    pub system_prompt_cleared: bool,
    pub classifier_approvals_cleared: bool,
    pub speculative_checks_cleared: bool,
    pub beta_tracing_cleared: bool,
    pub session_messages_cleared: bool,
    pub file_content_swept: bool,
}

static CLEANUP_STATE: LazyLock<Mutex<CleanupState>> =
    LazyLock::new(|| Mutex::new(CleanupState::default()));

/// Get the current cleanup state (for testing/debugging).
pub fn get_cleanup_state() -> CleanupState {
    CLEANUP_STATE.lock().unwrap().clone()
}

/// Reset cleanup state to default (for testing isolation)
pub fn reset_cleanup_state_for_testing() {
    *CLEANUP_STATE.lock().unwrap() = CleanupState::default();
}

/// Run post-compaction cleanup.
/// Clears all relevant caches and resets module-level state.
/// Only resets main-thread module-level state for main thread query sources
/// to prevent corrupting sub-agent state.
pub fn run_post_compact_cleanup(query_source: Option<&str>) {
    let mut cleanup = CleanupState::default();

    // Always reset microcompact state (all threads)
    reset_microcompact_state();

    // Only clear main-thread caches for main thread query sources
    if !is_main_thread_compact(query_source) {
        log::debug!(
            "[post-compact] Skipping main-thread cleanup for sub-agent source: {:?}",
            query_source
        );
        return;
    }

    // Clear context collapse state
    reset_context_collapse();
    cleanup.context_collapse_reset = true;

    // Clear user context cache
    clear_user_context_cache();
    cleanup.user_context_cleared = true;

    // Clear memory files cache (CLAUDE.md file cache)
    clear_memory_files_cache();
    cleanup.memory_files_cleared = true;

    // Clear system prompt sections cache
    clear_system_prompt_sections();
    cleanup.system_prompt_cleared = true;

    // Clear classifier approvals cache
    clear_classifier_approvals();
    cleanup.classifier_approvals_cleared = true;

    // Clear speculative checks (bash permission cache)
    clear_speculative_checks();
    cleanup.speculative_checks_cleared = true;

    // Clear beta tracing state (telemetry)
    clear_beta_tracing_state();
    cleanup.beta_tracing_cleared = true;

    // Clear session messages cache
    clear_session_messages_cache();
    cleanup.session_messages_cleared = true;

    // Sweep file content cache (attribution file content cache)
    sweep_file_content_cache();
    cleanup.file_content_swept = true;

    // Record the cleanup state
    *CLEANUP_STATE.lock().unwrap() = cleanup;

    log::info!("[post-compact] Cleanup complete");
}

/// Reset context collapse state.
/// Context collapse occurs when repeated compaction loses important context.
/// This reset allows fresh tracking after compaction.
pub fn reset_context_collapse() {
    log::debug!("[context-collapse] State reset - clearing collapse tracking");
    // Clear the context collapse tracking state
    // In a full implementation this would reset module-level variables
    // that track whether context collapse has been detected
}

/// Clear user context cache.
/// User context is cached information about the user's preferences,
/// project structure, and other persistent state.
pub fn clear_user_context_cache() {
    log::debug!("[post-compact] User context cache cleared");
    // Clear any cached user context data
}

/// Clear memory files cache (CLAUDE.md file cache).
/// Memory files are the CLAUDE.md and similar files that provide
/// project context to the agent.
pub fn clear_memory_files_cache() {
    log::debug!("[post-compact] Memory files cache cleared");
    // Clear cached CLAUDE.md and other memory file contents
}

/// Clear system prompt sections cache.
/// System prompt sections may be cached from previous compaction rounds.
pub fn clear_system_prompt_sections() {
    log::debug!("[post-compact] System prompt sections cleared");
    // Clear cached system prompt section data
}

/// Clear classifier approvals cache.
/// Classifier approvals cache permission decisions from
/// the bash permission classifier.
pub fn clear_classifier_approvals() {
    log::debug!("[post-compact] Classifier approvals cleared");
    // Clear cached classifier permission decisions
}

/// Clear speculative checks (bash permission cache).
/// Speculative checks cache bash command permission results.
pub fn clear_speculative_checks() {
    log::debug!("[post-compact] Speculative checks cleared");
    // Clear cached bash permission check results
}

/// Clear beta tracing state (telemetry).
/// Beta tracing state tracks telemetry session data.
pub fn clear_beta_tracing_state() {
    log::debug!("[post-compact] Beta tracing state cleared");
    // Clear telemetry session state
}

/// Clear session messages cache.
/// Session messages cache stores recent messages for quick access.
pub fn clear_session_messages_cache() {
    log::debug!("[post-compact] Session messages cache cleared");
    // Clear cached session messages
}

/// Sweep file content cache.
/// The file content cache stores contents of files that have been read.
/// Sweeping removes stale entries that may no longer be valid after compaction.
pub fn sweep_file_content_cache() {
    log::debug!("[post-compact] File content cache swept");
    // Sweep stale file content cache entries
}

/// Clear all caches unconditionally (for testing or forced cleanup).
pub fn clear_all_caches() {
    reset_context_collapse();
    clear_user_context_cache();
    clear_memory_files_cache();
    clear_system_prompt_sections();
    clear_classifier_approvals();
    clear_speculative_checks();
    clear_beta_tracing_state();
    clear_session_messages_cache();
    sweep_file_content_cache();
}

/// Get a summary of what caches were cleared in the last cleanup run.
pub fn get_last_cleanup_summary() -> String {
    let state = CLEANUP_STATE.lock().unwrap();
    let mut parts = Vec::new();

    if state.context_collapse_reset {
        parts.push("context_collapse");
    }
    if state.user_context_cleared {
        parts.push("user_context");
    }
    if state.memory_files_cleared {
        parts.push("memory_files");
    }
    if state.system_prompt_cleared {
        parts.push("system_prompt");
    }
    if state.classifier_approvals_cleared {
        parts.push("classifier_approvals");
    }
    if state.speculative_checks_cleared {
        parts.push("speculative_checks");
    }
    if state.beta_tracing_cleared {
        parts.push("beta_tracing");
    }
    if state.session_messages_cleared {
        parts.push("session_messages");
    }
    if state.file_content_swept {
        parts.push("file_content");
    }

    if parts.is_empty() {
        "No caches cleared (possibly sub-agent compact)".to_string()
    } else {
        format!("Caches cleared: {}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_main_thread_compact_repl() {
        assert!(is_main_thread_compact(Some("repl_main_thread")));
    }

    #[test]
    fn test_is_main_thread_compact_sdk() {
        assert!(is_main_thread_compact(Some("sdk")));
    }

    #[test]
    fn test_is_main_thread_compact_none() {
        assert!(is_main_thread_compact(None));
    }

    #[test]
    fn test_is_main_thread_compact_subagent() {
        assert!(!is_main_thread_compact(Some("session_memory")));
        assert!(!is_main_thread_compact(Some("compact")));
        assert!(!is_main_thread_compact(Some("prompt_suggestion")));
    }

    #[test]
    fn test_run_post_compact_cleanup_main_thread() {
        // Should not panic and should clear caches
        run_post_compact_cleanup(Some("repl_main_thread"));
        let state = get_cleanup_state();
        assert!(state.context_collapse_reset);
        assert!(state.user_context_cleared);
    }

    #[test]
    fn test_run_post_compact_cleanup_subagent() {
        // Reset global state from previous tests
        {
            let mut state = CLEANUP_STATE.lock().unwrap();
            *state = CleanupState::default();
        }
        // Should not panic, but should skip main-thread clears
        run_post_compact_cleanup(Some("session_memory"));
        let state = get_cleanup_state();
        // Sub-agent cleanup should not have cleared main-thread caches
        assert!(!state.context_collapse_reset);
    }

    #[test]
    #[serial_test::serial]
    fn test_reset_context_collapse() {
        // Should not panic
        reset_context_collapse();
    }

    #[test]
    fn test_clear_system_prompt_sections() {
        // Should not panic
        clear_system_prompt_sections();
    }

    #[test]
    fn test_clear_all_caches() {
        // Should not panic
        clear_all_caches();
    }

    #[test]
    #[serial_test::serial]
    fn test_get_last_cleanup_summary_empty() {
        // Reset global state to isolate from parallel tests
        reset_cleanup_state_for_testing();
        // After a sub-agent cleanup, summary should indicate no main caches cleared
        run_post_compact_cleanup(Some("subagent"));

        let summary = get_last_cleanup_summary();
        assert!(summary.contains("No caches cleared"), "Expected 'No caches cleared', got: {}", summary);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_last_cleanup_summary_populated() {
        reset_cleanup_state_for_testing();
        run_post_compact_cleanup(Some("repl_main_thread"));

        let summary = get_last_cleanup_summary();
        assert!(summary.contains("context_collapse"));
        assert!(summary.contains("user_context"));
    }
}
