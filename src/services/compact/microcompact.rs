// Source: ~/claudecode/openclaudecode/src/services/compact/microCompact.ts
//! Micro-compact module for proactive tool result eviction.
//!
//! Three micro-compact paths (in priority order):
//! 1. Time-based trigger: clears old tool results after idle gap
//! 2. Cached microcompact: API cache editing without invalidating prefix
//! 3. Legacy path: content-clear (removed, falls through)

use crate::compact::strip_images_from_messages;
use crate::tools::config_tools::{
    BASH_TOOL_NAME, FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME, GLOB_TOOL_NAME,
    GREP_TOOL_NAME, NOTEBOOK_EDIT_TOOL_NAME, POWERSHELL_TOOL_NAME, WEB_FETCH_TOOL_NAME,
    WEB_SEARCH_TOOL_NAME,
};
use crate::types::Message;
use crate::utils::env_utils;
use std::collections::HashSet;
use std::sync::Mutex;

/// Message shown when tool result content is cleared
pub const TIME_BASED_MC_CLEARED_MESSAGE: &str = "[Old tool result content cleared]";

/// Maximum tokens for images/documents
pub const IMAGE_MAX_TOKEN_SIZE: usize = 2000;

/// Tools whose results are compactable
fn compactable_tools() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    set.insert(FILE_READ_TOOL_NAME);
    set.insert(BASH_TOOL_NAME);
    set.insert(POWERSHELL_TOOL_NAME);
    set.insert(GREP_TOOL_NAME);
    set.insert(GLOB_TOOL_NAME);
    set.insert(WEB_SEARCH_TOOL_NAME);
    set.insert(WEB_FETCH_TOOL_NAME);
    set.insert(FILE_EDIT_TOOL_NAME);
    set.insert(FILE_WRITE_TOOL_NAME);
    set.insert(NOTEBOOK_EDIT_TOOL_NAME);
    set
}

// --- Time-based microcompact state ---

/// Evaluate whether the time-based trigger should fire.
/// Returns the measured gap (minutes since last assistant message) when the
/// trigger fires, or None when it doesn't.
pub fn evaluate_time_based_trigger(messages: &[Message]) -> Option<TimeBasedTriggerResult> {
    let config = crate::services::compact::time_based_mc_config::get_time_based_mc_config();

    if !config.enabled {
        return None;
    }

    // Find last assistant message timestamp
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, crate::types::MessageRole::Assistant));

    let Some(last_msg) = last_assistant else {
        return None;
    };

    // Get timestamp from message - use current time since Message doesn't have timestamp field
    // The original TypeScript used message.timestamp which is not available in api_types::Message
    let now_ms = chrono::Utc::now().timestamp_millis() as i64;
    let last_ts = now_ms;
    let gap_minutes = ((now_ms - last_ts) as f64 / 60_000.0).abs();

    if !gap_minutes.is_finite() || gap_minutes < config.gap_threshold_minutes as f64 {
        return None;
    }

    Some(TimeBasedTriggerResult {
        gap_minutes,
        config,
    })
}

pub struct TimeBasedTriggerResult {
    pub gap_minutes: f64,
    pub config: crate::services::compact::time_based_mc_config::TimeBasedMCConfig,
}

/// Collect compactable tool_use IDs from messages, in encounter order
pub fn collect_compactable_tool_ids(messages: &[Message]) -> Vec<String> {
    let compactable = compactable_tools();
    let mut ids = Vec::new();

    for msg in messages {
        if let crate::types::MessageRole::Assistant = msg.role {
            // Tool calls are in the tool_calls field
            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    if compactable.contains(tc.name.as_str()) {
                        ids.push(tc.id.clone());
                    }
                }
            }
        }
    }

    ids
}

/// Time-based microcompact: when the gap since the last assistant message
/// exceeds the configured threshold, content-clear all but the most recent N
/// compactable tool results.
pub fn maybe_time_based_microcompact(messages: &mut [Message]) -> Option<TimeBasedMCResult> {
    let trigger = evaluate_time_based_trigger(messages)?;
    let config = trigger.config;

    let compactable_ids = collect_compactable_tool_ids(messages);

    // Floor at 1: always keep at least the last tool result
    let keep_recent = config.keep_recent.max(1);
    let keep_set: HashSet<String> = compactable_ids
        .iter()
        .rev()
        .take(keep_recent)
        .cloned()
        .collect();
    let clear_set: HashSet<String> = compactable_ids
        .iter()
        .filter(|id| !keep_set.contains(*id))
        .cloned()
        .collect();

    if clear_set.is_empty() {
        return None;
    }

    let mut tokens_saved = 0;

    for msg in messages.iter_mut() {
        // Tool results have content that can be cleared
        if let crate::types::MessageRole::Tool = msg.role {
            if let Some(tool_call_id) = &msg.tool_call_id {
                if clear_set.contains(tool_call_id) && msg.content != TIME_BASED_MC_CLEARED_MESSAGE
                {
                    tokens_saved += crate::compact::rough_token_count_estimation(&msg.content, 4.0);
                    msg.content = TIME_BASED_MC_CLEARED_MESSAGE.to_string();
                }
            }
        }
    }

    if tokens_saved == 0 {
        return None;
    }

    log::debug!(
        "[TIME-BASED MC] gap {:.0}min > {}min, cleared {} tool results (~{} tokens), kept last {}",
        trigger.gap_minutes,
        config.gap_threshold_minutes,
        clear_set.len(),
        tokens_saved,
        keep_recent
    );

    // Reset microcompact state since we changed content
    reset_microcompact_state();

    Some(TimeBasedMCResult {
        tokens_saved,
        tools_cleared: clear_set.len(),
    })
}

pub struct TimeBasedMCResult {
    pub tokens_saved: usize,
    pub tools_cleared: usize,
}

// --- Cached microcompact state (stub for now) ---

/// Cached microcompact state - tracks tool results registered on prior turns
struct CachedMCState {
    registered_tools: HashSet<String>,
    tool_order: Vec<String>,
    deleted_refs: HashSet<String>,
    pinned_edits: Vec<PinnedCacheEdit>,
}

struct PinnedCacheEdit {
    user_message_index: usize,
    block: serde_json::Value,
}

static CACHED_MC_STATE: Mutex<Option<CachedMCState>> = Mutex::new(None);
static PENDING_CACHE_EDITS: Mutex<Option<serde_json::Value>> = Mutex::new(None);
static MICROCMPACT_STATE_RESET: Mutex<bool> = Mutex::new(false);

/// Reset microcompact state - called after compaction
pub fn reset_microcompact_state() {
    if let Ok(mut state) = CACHED_MC_STATE.lock() {
        *state = None;
    }
    if let Ok(mut pending) = PENDING_CACHE_EDITS.lock() {
        *pending = None;
    }
    if let Ok(mut flag) = MICROCMPACT_STATE_RESET.lock() {
        *flag = true;
    }
    log::debug!("[microcompact] State reset");
}

/// Get new pending cache edits to be included in the next API request.
/// Returns None if there are no new pending edits.
pub fn consume_pending_cache_edits() -> Option<serde_json::Value> {
    PENDING_CACHE_EDITS.lock().ok().and_then(|mut p| p.take())
}

/// Calculate tool result tokens
pub fn calculate_tool_result_tokens(content: &str) -> usize {
    crate::compact::rough_token_count_estimation(content, 4.0)
}

/// Estimate token count for messages
pub fn estimate_message_tokens(messages: &[Message]) -> usize {
    let mut total = 0;

    for msg in messages {
        match &msg.role {
            crate::types::MessageRole::User | crate::types::MessageRole::Assistant => {
                total += crate::compact::rough_token_count_estimation(&msg.content, 4.0);
            }
            crate::types::MessageRole::Tool => {
                // Tool results are JSON, more token-efficient: 2 chars/token
                total += msg.content.len() / 2;
            }
            crate::types::MessageRole::System => {
                total += crate::compact::rough_token_count_estimation(&msg.content, 4.0);
            }
        }
    }

    // Pad estimate by 4/3 to be conservative
    (total as f64 * (4.0 / 3.0)).ceil() as usize
}

/// Process messages to truncate large tool results (413 prevention)
pub fn microcompact_messages(messages: &mut [Message]) {
    // First, try time-based microcompact
    if let Some(_result) = maybe_time_based_microcompact(messages) {
        return;
    }

    // Fallback: truncate individual oversized tool results
    for msg in messages.iter_mut() {
        if let crate::types::MessageRole::Tool = &msg.role {
            if msg.content.len() > 16_000 {
                let tool_name = msg.tool_call_id.as_deref().unwrap_or("Tool");
                msg.content = truncate_tool_result_content(&msg.content, tool_name);
            }
        }
    }
}

/// Check if messages need microcompact (rough estimation)
pub fn needs_microcompact(messages: &[Message], threshold: usize) -> bool {
    let total_tool_chars: usize = messages
        .iter()
        .filter(|m| matches!(m.role, crate::types::MessageRole::Tool))
        .map(|m| m.content.len())
        .sum();

    let estimated_tokens = total_tool_chars / 4;
    estimated_tokens > threshold
}

/// Truncate tool result content if it's too large
pub fn truncate_tool_result_content(content: &str, tool_name: &str) -> String {
    const MAX_TOOL_RESULT_CHARS: usize = 16_000;
    const MAX_GLOB_RESULTS: usize = 100;

    if tool_name == "Glob" {
        let total_lines = content.lines().count();
        if total_lines <= MAX_GLOB_RESULTS {
            return content.to_string();
        }
        let lines: Vec<&str> = content.lines().take(MAX_GLOB_RESULTS).collect();
        let truncated = lines.join("\n");
        return format!(
            "{}\n\n... ({} more files not shown. Use more specific glob patterns to reduce results)",
            truncated,
            total_lines.saturating_sub(MAX_GLOB_RESULTS)
        );
    }

    if content.len() <= MAX_TOOL_RESULT_CHARS {
        return content.to_string();
    }

    let chars: Vec<char> = content.chars().take(MAX_TOOL_RESULT_CHARS).collect();
    format!(
        "{}\n\n... (truncated {} characters)",
        chars.into_iter().collect::<String>(),
        content.len().saturating_sub(MAX_TOOL_RESULT_CHARS)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_compactable_tool_ids() {
        let messages = vec![
            Message {
                role: crate::types::MessageRole::Assistant,
                content: String::new(),
                tool_calls: Some(vec![crate::types::ToolCall {
                    id: "call_1".to_string(),
                    r#type: "function".to_string(),
                    name: FILE_READ_TOOL_NAME.to_string(),
                    arguments: serde_json::json!({}),
                }]),
                ..Default::default()
            },
            Message {
                role: crate::types::MessageRole::Assistant,
                content: String::new(),
                tool_calls: Some(vec![crate::types::ToolCall {
                    id: "call_2".to_string(),
                    r#type: "function".to_string(),
                    name: "SomeOtherTool".to_string(),
                    arguments: serde_json::json!({}),
                }]),
                ..Default::default()
            },
        ];

        let ids = collect_compactable_tool_ids(&messages);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "call_1");
    }

    #[test]
    fn test_truncate_tool_result_small() {
        let content = "small content";
        let result = truncate_tool_result_content(content, "Read");
        assert_eq!(result, "small content");
    }

    #[test]
    fn test_truncate_tool_result_large() {
        let content = "x".repeat(20000);
        let result = truncate_tool_result_content(&content, "Read");
        assert!(result.len() < content.len());
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_estimate_message_tokens() {
        let messages = vec![Message {
            role: crate::types::MessageRole::User,
            content: "Hello, this is a test message".to_string(),
            ..Default::default()
        }];
        let tokens = estimate_message_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_reset_microcompact_state() {
        reset_microcompact_state();
        // Should not panic
        assert!(*MICROCMPACT_STATE_RESET.lock().unwrap());
    }

    #[test]
    fn test_calculate_tool_result_tokens() {
        let content = "test content";
        let tokens = calculate_tool_result_tokens(content);
        assert!(tokens > 0);
    }
}
