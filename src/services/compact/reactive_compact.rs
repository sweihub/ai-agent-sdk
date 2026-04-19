// Source: ~/claudecode/openclaudecode/src/services/compact/reactiveCompact.ts
//! Reactive compact module for error recovery.
//!
//! When the API rejects a request due to context size (413 / media too large),
//! reactive compact tries to recover by compacting the oldest group of messages
//! that contains the overflow, then retrying the request.

use crate::compact::estimate_token_count;
use crate::services::compact::grouping::group_messages_by_api_round;
use crate::types::Message;

/// Result of a reactive compact attempt.
#[derive(Debug, Clone)]
pub struct ReactiveCompactResult {
    /// The compacted message list to retry with.
    pub messages: Vec<Message>,
    /// Whether a compact actually happened.
    pub compacted: bool,
}

/// Attempt reactive compact to reduce context size for retry.
///
/// Groups messages by API round, then drops the oldest groups until the
/// total token count falls below the effective context window size.
pub fn run_reactive_compact(
    messages: &[Message],
    model: &str,
) -> Result<ReactiveCompactResult, String> {
    let effective_window = crate::compact::get_effective_context_window_size(model) as usize;
    let token_count = estimate_token_count(messages, effective_window as u32) as usize;

    if token_count <= effective_window {
        return Ok(ReactiveCompactResult {
            messages: messages.to_vec(),
            compacted: false,
        });
    }

    let groups = group_messages_by_api_round(messages);
    if groups.len() <= 1 {
        return Ok(ReactiveCompactResult {
            messages: messages.to_vec(),
            compacted: false,
        });
    }

    // Drop oldest groups until under the window
    let mut remaining: Vec<Message> = messages.to_vec();
    for group in &groups {
        if remaining.len() <= 4 {
            break;
        }

        let new_len = remaining.len().saturating_sub(group.len());
        if new_len < 4 {
            break;
        }

        // Remove this group from remaining
        remaining = remove_group(&remaining, group);

        let new_tokens = estimate_token_count(&remaining, effective_window as u32) as usize;
        if new_tokens < effective_window {
            break;
        }
    }

    Ok(ReactiveCompactResult {
        messages: remaining,
        compacted: true,
    })
}

/// Remove a group of messages from the full list (by position matching).
fn remove_group(all: &[Message], group: &[Message]) -> Vec<Message> {
    if group.len() >= all.len() {
        return Vec::new();
    }

    // Find the group as a contiguous slice in all
    for i in 0..=all.len().saturating_sub(group.len()) {
        if all[i..i + group.len()].iter().zip(group.iter()).all(|(a, b)| {
            a.content == b.content && a.role == b.role
        }) {
            let mut result = all[..i].to_vec();
            result.extend_from_slice(&all[i + group.len()..]);
            return result;
        }
    }

    // Fallback: drop the oldest N messages where N = group size
    let keep = all.len().saturating_sub(group.len());
    all[keep..].to_vec()
}

/// Check if reactive compact is available.
pub fn is_reactive_compact_enabled() -> bool {
    !crate::utils::env_utils::is_env_truthy(Some("DISABLE_REACTIVE_COMPACT"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reactive_compact_below_threshold() {
        let messages = vec![Message {
            role: crate::types::MessageRole::User,
            content: "Hello".to_string(),
            ..Default::default()
        }];
        let result = run_reactive_compact(&messages, "claude-sonnet-4-6");
        assert!(result.is_ok());
        assert!(!result.unwrap().compacted);
    }

    #[test]
    fn test_reactive_compact_enabled() {
        assert!(is_reactive_compact_enabled());
    }
}
