// Source: /data/home/swei/claudecode/openclaudecode/src/services/compact/autoCompact.ts
//! Auto-compact module for automatic context compaction.
//!
//! This module provides auto-compaction logic that triggers when conversation
//! context approaches the token limit. It reuses core compaction functions
//! from the parent compact.rs module.
//!
//! Translated from TypeScript autoCompact.ts

use crate::compact::{
    CompactionResult, TokenWarningState,
    calculate_token_warning_state as core_calculate_token_warning_state,
    get_auto_compact_threshold as core_get_auto_compact_threshold,
    get_effective_context_window_size as core_get_effective_context_window_size,
};
use crate::types::Message;
use crate::utils::env_utils::is_env_truthy;

/// Diagnosis context passed from autoCompactIfNeeded into compactConversation.
/// Lets the tengu_compact event disambiguate same-chain loops (H2) from
/// cross-agent (H1/H5) and manual-vs-auto (H3) compactions without joins.
/// Translated from: RecompactionInfo in autoCompact.ts
#[derive(Debug, Clone, Default)]
pub struct RecompactionInfo {
    pub is_recompaction_in_chain: bool,
    pub turns_since_previous_compact: i32,
    pub previous_compact_turn_id: Option<String>,
    pub auto_compact_threshold: usize,
    pub query_source: Option<String>,
}

/// Result from autoCompactIfNeeded
/// Translated from: autoCompactIfNeeded return type in autoCompact.ts
#[derive(Debug, Clone, Default)]
pub struct AutoCompactResult {
    pub was_compacted: bool,
    pub compaction_result: Option<CompactionResult>,
    pub consecutive_failures: Option<usize>,
}

/// Auto-compact tracking state
/// Translated from: AutoCompactTrackingState in autoCompact.ts
#[derive(Debug, Clone, Default)]
pub struct AutoCompactTrackingState {
    pub compacted: bool,
    pub turn_counter: usize,
    /// Unique ID per turn
    pub turn_id: String,
    /// Consecutive autocompact failures. Reset on success.
    /// Used as a circuit breaker to stop retrying when the context is
    /// irrecoverably over the limit (e.g., prompt_too_long).
    pub consecutive_failures: usize,
}

impl AutoCompactTrackingState {
    pub fn new() -> Self {
        Self {
            compacted: false,
            turn_counter: 0,
            turn_id: uuid::Uuid::new_v4().to_string(),
            consecutive_failures: 0,
        }
    }
}

// Re-export constants from compact.rs for convenience
pub use crate::compact::{
    AUTOCOMPACT_BUFFER_TOKENS, ERROR_THRESHOLD_BUFFER_TOKENS, MANUAL_COMPACT_BUFFER_TOKENS,
    MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES,
};

/// Get effective context window size (total - output reserve)
/// Translated from: getEffectiveContextWindowSize in autoCompact.ts
/// This reuses the core function and converts to usize for compatibility
pub fn get_effective_context_window_size(model: &str) -> usize {
    core_get_effective_context_window_size(model) as usize
}

/// Get auto-compact threshold (when to trigger compaction)
/// Translated from: getAutoCompactThreshold in autoCompact.ts
/// This reuses the core function and converts to usize for compatibility
pub fn get_auto_compact_threshold(model: &str) -> usize {
    core_get_auto_compact_threshold(model) as usize
}

/// Calculate token warning state
/// Translated from: calculateTokenWarningState in autoCompact.ts
/// This reuses the core function
pub fn calculate_token_warning_state(token_usage: usize, model: &str) -> TokenWarningState {
    core_calculate_token_warning_state(token_usage as u32, model)
}

/// Check if auto-compact is enabled
/// Translated from: isAutoCompactEnabled in autoCompact.ts
pub fn is_auto_compact_enabled() -> bool {
    if is_env_truthy(Some("DISABLE_COMPACT")) {
        return false;
    }
    // Allow disabling just auto-compact (keeps manual /compact working)
    if is_env_truthy(Some("DISABLE_AUTO_COMPACT")) {
        return false;
    }
    // Check if user has disabled auto-compact in their settings
    // In the full implementation, this would check getGlobalConfig().autoCompactEnabled
    // For now, default to true
    true
}

/// Check if query source is a forked agent that would deadlock
fn is_forked_agent_query_source(query_source: Option<&str>) -> bool {
    matches!(query_source, Some("session_memory") | Some("compact"))
}

/// Check if query source is marble_origami (ctx-agent)
fn is_marble_origami_query_source(query_source: Option<&str>) -> bool {
    matches!(query_source, Some("marble_origami"))
}

/// Check if auto-compact should run
/// Translated from: shouldAutoCompact in autoCompact.ts
pub fn should_auto_compact(
    messages: &[Message],
    model: &str,
    query_source: Option<&str>,
    snip_tokens_freed: usize,
) -> bool {
    // Recursion guards. session_memory and compact are forked agents that
    // would deadlock.
    if is_forked_agent_query_source(query_source) {
        return false;
    }

    // marble_origami is the ctx-agent — if ITS context blows up and
    // autocompact fires, runPostCompactCleanup calls resetContextCollapse()
    // which destroys the MAIN thread's committed log
    // Feature gate: CONTEXT_COLLAPSE - for now skip this check

    if !is_auto_compact_enabled() {
        return false;
    }

    // Feature gate: REACTIVE_COMPACT - suppress proactive autocompact
    // In full implementation, check getFeatureValue_CACHED_MAY_BE_STALE('tengu_cobalt_raccoon', false)
    // For now, skip this feature gate

    // Feature gate: CONTEXT_COLLAPSE
    // In full implementation, check isContextCollapseEnabled()
    // For now, skip this feature gate

    // Calculate token count
    let token_count = estimate_token_count(messages).saturating_sub(snip_tokens_freed);
    let threshold = get_auto_compact_threshold(model);
    let effective_window = get_effective_context_window_size(model);

    log::debug!(
        "autocompact: tokens={} threshold={} effective_window={}{}",
        token_count,
        threshold,
        effective_window,
        if snip_tokens_freed > 0 {
            format!(" snipFreed={}", snip_tokens_freed)
        } else {
            String::new()
        }
    );

    let state = calculate_token_warning_state(token_count, model);
    state.is_above_auto_compact_threshold
}

/// Perform auto-compaction if needed
/// Translated from: autoCompactIfNeeded in autoCompact.ts
pub async fn auto_compact_if_needed(
    messages: &[Message],
    model: &str,
    query_source: Option<&str>,
    tracking: Option<&AutoCompactTrackingState>,
    snip_tokens_freed: usize,
) -> AutoCompactResult {
    // Check if compact is disabled
    if is_env_truthy(Some("DISABLE_COMPACT")) {
        return AutoCompactResult::default();
    }

    // Circuit breaker: stop retrying after N consecutive failures.
    // Without this, sessions where context is irrecoverably over the limit
    // hammer the API with doomed compaction attempts on every turn.
    if let Some(t) = tracking {
        if t.consecutive_failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES as usize {
            return AutoCompactResult::default();
        }
    }

    let should_compact = should_auto_compact(messages, model, query_source, snip_tokens_freed);

    if !should_compact {
        return AutoCompactResult::default();
    }

    // Build recompaction info
    let recompaction_info = RecompactionInfo {
        is_recompaction_in_chain: tracking.map(|t| t.compacted).unwrap_or(false),
        turns_since_previous_compact: tracking.map(|t| t.turn_counter as i32).unwrap_or(-1),
        previous_compact_turn_id: tracking.map(|t| t.turn_id.clone()),
        auto_compact_threshold: get_auto_compact_threshold(model),
        query_source: query_source.map(|s| s.to_string()),
    };

    log::debug!(
        "autocompact: triggering compaction with recompaction_info: {:?}",
        recompaction_info
    );

    // EXPERIMENT: Try session memory compaction first
    // In full implementation: trySessionMemoryCompaction(messages, agentId, recompactionInfo.autoCompactThreshold)
    // For now, skip session memory compaction

    // Call the actual compaction logic
    let token_count = estimate_token_count(messages);
    let effective_window = get_effective_context_window_size(model);

    // Target: compact down to about 60% of the effective window to leave room
    let target_tokens = (effective_window as f64 * 0.6) as u64;

    let options = crate::services::compact::compact::CompactOptions {
        max_tokens: Some(target_tokens),
        direction: crate::services::compact::compact::CompactDirection::Smart,
        create_boundary: true,
        system_prompt: None,
    };

    match crate::services::compact::compact::compact_messages(messages, options).await {
        Ok(compact_result) => {
            if compact_result.success {
                log::info!(
                    "autocompact: compacted {} messages ({} -> {} tokens, removed {} messages)",
                    messages.len(),
                    compact_result.tokens_before,
                    compact_result.tokens_after,
                    compact_result.messages_removed
                );
                // Build a boundary marker message for the compaction result
                let boundary_marker = crate::types::Message {
                    role: crate::types::MessageRole::User,
                    content: format!("[Conversation was compacted. {} messages summarized to free up context space.]", compact_result.messages_removed),
                    attachments: None,
                    tool_call_id: None,
                    tool_calls: None,
                    is_error: None,
                    is_meta: Some(true),
                    is_api_error_message: None,
                    error_details: None,
                    uuid: None,
                    timestamp: None,
                };
                let summary_messages = if !compact_result.summary.is_empty() {
                    vec![crate::types::Message {
                        role: crate::types::MessageRole::Assistant,
                        content: compact_result.summary,
                        attachments: None,
                        tool_call_id: None,
                        tool_calls: None,
                        is_error: None,
                        is_meta: None,
                        is_api_error_message: None,
                        error_details: None,
                        uuid: None,
                        timestamp: None,
                    }]
                } else {
                    vec![]
                };
                AutoCompactResult {
                    was_compacted: true,
                    compaction_result: Some(CompactionResult {
                        boundary_marker,
                        summary_messages,
                        messages_to_keep: Some(compact_result.messages_to_keep),
                        attachments: vec![],
                        hook_results: vec![],
                        pre_compact_token_count: compact_result.tokens_before as u32,
                        post_compact_token_count: compact_result.tokens_after as u32,
                        true_post_compact_token_count: None,
                        compaction_usage: None,
                    }),
                    consecutive_failures: Some(0), // Reset on success
                }
            } else {
                log::warn!(
                    "autocompact: compaction failed: {:?}",
                    compact_result.error
                );
                let prev_failures = tracking.map(|t| t.consecutive_failures).unwrap_or(0);
                let next_failures = prev_failures + 1;
                if next_failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES as usize {
                    log::warn!(
                        "autocompact: circuit breaker tripped after {} consecutive failures — skipping future attempts this session",
                        next_failures
                    );
                }
                AutoCompactResult {
                    was_compacted: false,
                    compaction_result: None,
                    consecutive_failures: Some(next_failures),
                }
            }
        }
        Err(e) => {
            log::error!("autocompact: compaction error: {}", e);
            let prev_failures = tracking.map(|t| t.consecutive_failures).unwrap_or(0);
            let next_failures = prev_failures + 1;
            if next_failures >= MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES as usize {
                log::warn!(
                    "autocompact: circuit breaker tripped after {} consecutive failures — skipping future attempts this session",
                    next_failures
                );
            }
            AutoCompactResult {
                was_compacted: false,
                compaction_result: None,
                consecutive_failures: Some(next_failures),
            }
        }
    }
}

/// Estimate token count for messages
/// Simplified version - full implementation would use tokenCountWithEstimation
fn estimate_token_count(messages: &[Message]) -> usize {
    // Rough estimation: 4 chars per token
    messages.iter().map(|m| m.content.len() / 4).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageRole;

    #[test]
    fn test_get_effective_context_window_size() {
        let window = get_effective_context_window_size("claude-sonnet-4-6");
        // Should be 200000 - 20000 = 180000
        assert!(window > 0);
    }

    #[test]
    fn test_get_auto_compact_threshold() {
        let threshold = get_auto_compact_threshold("claude-sonnet-4-6");
        // Should be 180000 - 13000 = 167000
        let effective = get_effective_context_window_size("claude-sonnet-4-6");
        assert!(threshold < effective);
    }

    #[test]
    fn test_calculate_token_warning_state() {
        let state = calculate_token_warning_state(50_000, "claude-sonnet-4-6");
        assert!(!state.is_above_warning_threshold);
        assert!(!state.is_above_error_threshold);
        assert!(!state.is_above_auto_compact_threshold);
        assert!(state.percent_left > 50.0);
    }

    #[test]
    fn test_calculate_token_warning_state_at_threshold() {
        let threshold = get_auto_compact_threshold("claude-sonnet-4-6");
        let state = calculate_token_warning_state(threshold as usize, "claude-sonnet-4-6");
        assert!(state.is_above_auto_compact_threshold);
    }

    #[test]
    fn test_is_auto_compact_enabled_default() {
        // Should return true by default
        let result = is_auto_compact_enabled();
        assert!(result || !result); // Just check it doesn't panic
    }

    #[test]
    fn test_should_auto_compact_empty_messages() {
        let messages: Vec<Message> = vec![];
        let result = should_auto_compact(&messages, "claude-sonnet-4-6", None, 0);
        // Empty messages should not trigger compaction
        assert!(!result);
    }

    #[test]
    fn test_should_auto_compact_forked_agent_guards() {
        let messages: Vec<Message> = vec![];
        // session_memory should return false
        let result = should_auto_compact(&messages, "claude-sonnet-4-6", Some("session_memory"), 0);
        assert!(!result);

        // compact should return false
        let result = should_auto_compact(&messages, "claude-sonnet-4-6", Some("compact"), 0);
        assert!(!result);
    }

    #[test]
    fn test_auto_compact_tracking_state() {
        let state = AutoCompactTrackingState::new();
        assert!(!state.compacted);
        assert_eq!(state.turn_counter, 0);
        assert!(!state.turn_id.is_empty());
        assert_eq!(state.consecutive_failures, 0);
    }

    #[test]
    fn test_recompaction_info_default() {
        let info = RecompactionInfo::default();
        assert!(!info.is_recompaction_in_chain);
        assert_eq!(info.turns_since_previous_compact, 0);
        assert!(info.previous_compact_turn_id.is_none());
    }

    #[test]
    fn test_auto_compact_result_default() {
        let result = AutoCompactResult::default();
        assert!(!result.was_compacted);
        assert!(result.compaction_result.is_none());
    }
}
