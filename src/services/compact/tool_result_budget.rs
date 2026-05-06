// Source: ~/claudecode/openclaudecode/src/utils/toolResultStorage.ts (aggregate budget section)
//! Per-message aggregate tool result budget enforcement.
//!
//! When a single turn produces many large tool results, their combined size
//! can exceed the per-message budget even though each individual result is
//! under its own threshold. This module enforces the aggregate budget by
//! persisting the largest fresh results to disk and replacing them with previews.
//!
//! State is tracked by tool_use_id so decisions are stable across compaction
//! turns, preserving prompt cache prefix.

use std::collections::{HashMap, HashSet};

use crate::tool_result_storage::{
    self, generate_preview, MAX_TOOL_RESULTS_PER_MESSAGE_CHARS, PREVIEW_SIZE_BYTES,
};
use crate::types::{Message, MessageRole};

/// XML tag that wraps persisted output messages.
const PERSISTED_OUTPUT_TAG: &str = "<persisted-output>";

/// Per-conversation-thread state for the aggregate tool result budget.
///
/// - `seen_ids`: results that have passed through the budget check (replaced or not).
///   Once seen, a result's fate is frozen for the conversation.
/// - `replacements`: subset of seen_ids that were persisted to disk, mapped to
///   the exact preview string shown to the model.
#[derive(Debug, Clone, Default)]
pub struct ContentReplacementState {
    pub seen_ids: HashSet<String>,
    pub replacements: HashMap<String, String>,
}

/// Record of one content-replacement decision for serialization.
#[derive(Debug, Clone)]
pub struct ToolResultReplacementRecord {
    pub tool_use_id: String,
    pub replacement: String,
}

/// Candidate tool result for budget enforcement.
#[derive(Debug, Clone)]
struct ToolResultCandidate {
    tool_use_id: String,
    content: String,
    size: usize,
}

/// Partitioned candidates by prior decision state.
#[derive(Debug, Default)]
struct CandidatePartition {
    /// Previously replaced — must re-apply the cached replacement.
    must_reapply: Vec<(ToolResultCandidate, String)>,
    /// Previously seen and left unreplaced — off-limits.
    frozen: Vec<ToolResultCandidate>,
    /// Never seen — eligible for new replacement decisions.
    fresh: Vec<ToolResultCandidate>,
}

/// Create a fresh replacement state for a new conversation thread.
pub fn create_content_replacement_state() -> ContentReplacementState {
    ContentReplacementState::default()
}

/// Check if content was already compacted by the budget or per-tool limit.
fn is_content_already_compacted(content: &str) -> bool {
    content.starts_with(PERSISTED_OUTPUT_TAG)
}

/// Collect tool result candidate messages from a group of consecutive user/tool messages.
///
/// Only includes messages with `role: Tool`, non-empty content, not already compacted.
fn collect_candidates(messages: &[Message]) -> Vec<ToolResultCandidate> {
    messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .filter_map(|m| {
            if m.content.is_empty() {
                return None;
            }
            if is_content_already_compacted(&m.content) {
                return None;
            }
            let tool_use_id = m.tool_call_id.clone()?;
            Some(ToolResultCandidate {
                tool_use_id,
                content: m.content.clone(),
                size: m.content.len(),
            })
        })
        .collect()
}

/// Group messages by API-level message boundary.
///
/// In the Rust SDK, consecutive tool messages between assistant messages form
/// a single API-level user/tool group. The budget enforces per-group.
fn group_message_ranges(messages: &[Message]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        match msg.role {
            MessageRole::Tool | MessageRole::User => {
                current_group.push(i);
            }
            MessageRole::Assistant => {
                if !current_group.is_empty() {
                    groups.push(current_group.clone());
                    current_group.clear();
                }
            }
            MessageRole::System => {
                // System messages don't create boundaries or join groups
            }
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }
    groups
}

/// Partition candidates by their prior decision state.
fn partition_by_prior_decision(
    candidates: Vec<ToolResultCandidate>,
    state: &ContentReplacementState,
) -> CandidatePartition {
    let mut partition = CandidatePartition::default();

    for c in candidates {
        if let Some(replacement) = state.replacements.get(&c.tool_use_id) {
            partition.must_reapply.push((c, replacement.clone()));
        } else if state.seen_ids.contains(&c.tool_use_id) {
            partition.frozen.push(c);
        } else {
            partition.fresh.push(c);
        }
    }

    partition
}

/// Pick the largest fresh results to replace until under budget.
fn select_fresh_to_replace(
    fresh: Vec<ToolResultCandidate>,
    frozen_size: usize,
    limit: usize,
) -> Vec<ToolResultCandidate> {
    let mut sorted = fresh;
    // Sort descending by size — largest first
    sorted.sort_by(|a, b| b.size.cmp(&a.size));

    let mut selected = Vec::new();
    let mut remaining = frozen_size + sorted.iter().map(|c| c.size).sum::<usize>();

    for c in sorted {
        if remaining <= limit {
            break;
        }
        let size = c.size;
        selected.push(c);
        remaining -= size;
    }

    selected
}

/// Build a persisted-output preview message for a tool result.
fn build_persisted_message(tool_use_id: &str, content: &str) -> Option<String> {
    // Try to persist to disk via the storage layer.
    // Threshold 0 forces maybe_persist_large_result to always persist (content.len() > 0).
    let persist_result = tool_result_storage::maybe_persist_large_result(
        content,
        tool_use_id,
        "", // tool name not available at this layer
        None, // project_dir — use generic persistence
        None, // session_id
        0, // threshold: force persistence for any non-empty content
    );

    if persist_result.1 {
        // Persisted by maybe_persist_large_result
        Some(persist_result.0)
    } else {
        // Fallback: build inline preview without disk persistence
        let preview = tool_result_storage::generate_preview(content);
        Some(format!(
            "<persisted-output>\n\
             Output too large ({} chars). Replaced by aggregate budget.\n\n\
             Preview (first {} bytes):\n\
             {}\n\
             {}\n\
             </persisted-output>",
            content.len(),
            PREVIEW_SIZE_BYTES,
            preview.text,
            if preview.has_more { "..." } else { "" }
        ))
    }
}

/// Enforce the per-message budget on aggregate tool result size.
///
/// For each message group whose tool result blocks together exceed the
/// per-message limit, the largest fresh results are replaced with previews.
///
/// Returns newly replaced records for transcript persistence.
pub fn enforce_tool_result_budget(
    messages: &mut Vec<Message>,
    state: &mut ContentReplacementState,
) -> Vec<ToolResultReplacementRecord> {
    let limit = MAX_TOOL_RESULTS_PER_MESSAGE_CHARS;
    let groups = group_message_ranges(messages.as_slice());

    // Global replacement map for this pass (re-applies + fresh)
    let mut replacement_map: HashMap<String, String> = HashMap::new();
    let mut newly_replaced: Vec<ToolResultReplacementRecord> = Vec::new();

    for group in groups {
        // Collect only from messages in this group
        let candidates: Vec<ToolResultCandidate> = group
            .iter()
            .filter_map(|&idx| {
                let msg = &messages[idx];
                if msg.role != MessageRole::Tool || msg.content.is_empty() {
                    return None;
                }
                if is_content_already_compacted(&msg.content) {
                    return None;
                }
                msg.tool_call_id.as_ref().map(|id| ToolResultCandidate {
                    tool_use_id: id.clone(),
                    content: msg.content.clone(),
                    size: msg.content.len(),
                })
            })
            .collect();

        if candidates.is_empty() {
            continue;
        }

        let partition = partition_by_prior_decision(candidates, state);

        // Re-apply cached replacements
        for (c, replacement) in partition.must_reapply {
            replacement_map.insert(c.tool_use_id, replacement);
        }

        // If no fresh candidates, just mark everything as seen and continue
        if partition.fresh.is_empty() {
            // All IDs already in seen_ids from prior pass
            continue;
        }

        // Compute sizes
        let frozen_size: usize = partition.frozen.iter().map(|c| c.size).sum();
        let fresh_size: usize = partition.fresh.iter().map(|c| c.size).sum();

        // Select largest fresh to replace if over budget
        let selected = if frozen_size + fresh_size > limit {
            select_fresh_to_replace(partition.fresh.clone(), frozen_size, limit)
        } else {
            Vec::new()
        };

        // Mark non-selected candidates as seen (frozen for future turns)
        let selected_ids: HashSet<String> = selected.iter().map(|c| c.tool_use_id.clone()).collect();
        for c in partition.fresh.iter().chain(partition.frozen.iter()) {
            if !selected_ids.contains(&c.tool_use_id) {
                state.seen_ids.insert(c.tool_use_id.clone());
            }
        }

        // Persist selected candidates and build replacements

        for c in selected {
            state.seen_ids.insert(c.tool_use_id.clone());
            if let Some(replacement) = build_persisted_message(&c.tool_use_id, &c.content) {
                replacement_map.insert(c.tool_use_id.clone(), replacement.clone());
                state.replacements.insert(c.tool_use_id.clone(), replacement.clone());
                newly_replaced.push(ToolResultReplacementRecord {
                    tool_use_id: c.tool_use_id,
                    replacement,
                });
            }
        }
    }

    // Apply replacements to messages
    if replacement_map.is_empty() {
        return newly_replaced;
    }

    for msg in messages.iter_mut() {
        if msg.role == MessageRole::Tool {
            if let Some(ref tool_use_id) = msg.tool_call_id {
                if let Some(replacement) = replacement_map.get(tool_use_id) {
                    msg.content = replacement.clone();
                }
            }
        }
    }

    newly_replaced
}

/// Query-loop integration point for the aggregate budget.
///
/// Applies enforcement and returns newly replaced records.
/// The caller should persist records to the transcript for resume reconstruction.
///
/// Returns the messages with replacements applied (same vec).
pub fn apply_tool_result_budget(
    messages: &mut Vec<Message>,
    state: Option<&mut ContentReplacementState>,
) -> Vec<ToolResultReplacementRecord> {
    let state = match state {
        Some(s) => s,
        None => return Vec::new(),
    };
    enforce_tool_result_budget(messages, state)
}

/// Reconstruct replacement state from records loaded from a transcript.
///
/// Freezes all candidate tool_use_ids in the loaded messages and populates
/// the replacements map from stored records.
pub fn reconstruct_content_replacement_state(
    messages: &[Message],
    records: &[ToolResultReplacementRecord],
) -> ContentReplacementState {
    let mut state = create_content_replacement_state();

    // Freeze all tool result candidates in messages
    for msg in messages {
        if msg.role == MessageRole::Tool {
            if let Some(ref id) = msg.tool_call_id {
                state.seen_ids.insert(id.clone());
            }
        }
    }

    // Populate replacements for IDs that are still in messages
    for record in records {
        if state.seen_ids.contains(&record.tool_use_id) {
            state
                .replacements
                .insert(record.tool_use_id.clone(), record.replacement.clone());
        }
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_message(tool_use_id: &str, content: &str) -> Message {
        Message {
            role: MessageRole::Tool,
            content: content.to_string(),
            attachments: None,
            tool_call_id: Some(tool_use_id.to_string()),
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            timestamp: None,
        }
    }

    fn make_assistant_message(content: &str) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: content.to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            timestamp: None,
        }
    }

    fn make_user_message(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: content.to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            timestamp: None,
        }
    }

    #[test]
    fn test_create_content_replacement_state() {
        let state = create_content_replacement_state();
        assert!(state.seen_ids.is_empty());
        assert!(state.replacements.is_empty());
    }

    #[test]
    fn test_is_content_already_compacted() {
        assert!(is_content_already_compacted("<persisted-output>\n..."));
        assert!(!is_content_already_compacted("normal output"));
        assert!(!is_content_already_compacted("output containing <persisted-output> tag"));
    }

    #[test]
    fn test_group_message_ranges_single_group() {
        let messages = vec![
            make_tool_message("id1", "result1"),
            make_tool_message("id2", "result2"),
        ];
        let groups = group_message_ranges(&messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0, 1]);
    }

    #[test]
    fn test_group_message_ranges_split_by_assistant() {
        let messages = vec![
            make_tool_message("id1", "r1"), // index 0
            make_assistant_message("ok"),   // index 1 — creates boundary
            make_tool_message("id2", "r2"), // index 2
        ];
        let groups = group_message_ranges(&messages);
        // Assistant message splits tool messages into two groups
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![2]);
    }

    #[test]
    fn test_group_message_ranges_with_user() {
        let messages = vec![
            make_user_message("hello"),
            make_tool_message("id1", "r1"),
        ];
        let groups = group_message_ranges(&messages);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0, 1]);
    }

    #[test]
    fn test_collect_candidates_skips_compacted() {
        let messages = vec![
            make_tool_message("id1", "normal"),
            make_tool_message("id2", "<persisted-output>\n..."),
            make_tool_message("id3", "also normal"),
        ];
        let candidates = collect_candidates(&messages);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].tool_use_id, "id1");
        assert_eq!(candidates[1].tool_use_id, "id3");
    }

    #[test]
    fn test_collect_candidates_skips_empty() {
        let messages = vec![
            make_tool_message("id1", "content"),
            make_tool_message("id2", ""),
        ];
        let candidates = collect_candidates(&messages);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_partition_by_prior_decision() {
        let mut state = create_content_replacement_state();
        state.seen_ids.insert("frozen-id".to_string());
        state
            .replacements
            .insert("reapply-id".to_string(), "cached preview".to_string());

        let candidates = vec![
            ToolResultCandidate {
                tool_use_id: "reapply-id".to_string(),
                content: "big content".to_string(),
                size: 11,
            },
            ToolResultCandidate {
                tool_use_id: "frozen-id".to_string(),
                content: "frozen content".to_string(),
                size: 14,
            },
            ToolResultCandidate {
                tool_use_id: "fresh-id".to_string(),
                content: "fresh content".to_string(),
                size: 13,
            },
        ];

        let partition = partition_by_prior_decision(candidates, &state);
        assert_eq!(partition.must_reapply.len(), 1);
        assert_eq!(partition.must_reapply[0].1, "cached preview");
        assert_eq!(partition.frozen.len(), 1);
        assert_eq!(partition.frozen[0].tool_use_id, "frozen-id");
        assert_eq!(partition.fresh.len(), 1);
        assert_eq!(partition.fresh[0].tool_use_id, "fresh-id");
    }

    #[test]
    fn test_select_fresh_to_replace_picks_largest() {
        let fresh = vec![
            ToolResultCandidate {
                tool_use_id: "small".to_string(),
                content: "x".to_string(),
                size: 1,
            },
            ToolResultCandidate {
                tool_use_id: "medium".to_string(),
                content: "a".repeat(100),
                size: 100,
            },
            ToolResultCandidate {
                tool_use_id: "large".to_string(),
                content: "b".repeat(500),
                size: 500,
            },
        ];
        // frozen_size = 0, limit = 200, total = 601
        // Should pick large (500), remaining = 101 <= 200 → stop
        let selected = select_fresh_to_replace(fresh, 0, 200);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].tool_use_id, "large");
    }

    #[test]
    fn test_select_fresh_to_replace_under_budget() {
        let fresh = vec![ToolResultCandidate {
            tool_use_id: "tiny".to_string(),
            content: "x".to_string(),
            size: 10,
        }];
        let selected = select_fresh_to_replace(fresh, 0, 100_000);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_select_fresh_to_replace_with_frozen_overage() {
        // frozen alone exceeds budget — fresh should be accepted as overage
        let fresh = vec![
            ToolResultCandidate {
                tool_use_id: "f1".to_string(),
                content: "a".repeat(100),
                size: 100,
            },
            ToolResultCandidate {
                tool_use_id: "f2".to_string(),
                content: "b".repeat(200),
                size: 200,
            },
        ];
        // frozen = 50_001, limit = 50_000 → already over
        // total = 50_301, picks f2 (200) → remaining 50_101 > 50_000
        // picks f1 (100) → remaining 50_001 > 50_000 → no more fresh
        let selected = select_fresh_to_replace(fresh, 50_001, 50_000);
        assert_eq!(selected.len(), 2); // Both picked but still over — that's expected
    }

    #[test]
    fn test_enforce_budget_under_limit() {
        let mut messages = vec![
            make_tool_message("id1", &"a".repeat(1000)),
            make_tool_message("id2", &"b".repeat(2000)),
        ];
        let mut state = create_content_replacement_state();
        let records = enforce_tool_result_budget(&mut messages, &mut state);
        // Total = 3000 << 200_000 → no replacements
        assert!(records.is_empty());
        // But messages should be marked as seen
        assert_eq!(state.seen_ids.len(), 2);
        assert!(state.replacements.is_empty());
    }

    #[test]
    fn test_enforce_budget_over_limit_replaces_largest() {
        let limit = MAX_TOOL_RESULTS_PER_MESSAGE_CHARS;
        // Create messages that exceed the limit
        let mut messages = vec![
            make_tool_message("small", &"s".repeat(50_000)),
            make_tool_message("big", &"b".repeat(200_000)),
        ];
        let mut state = create_content_replacement_state();
        let records = enforce_tool_result_budget(&mut messages, &mut state);

        // big (200k) should be replaced first, total after = 50k < 200k
        assert!(!records.is_empty());
        let replaced_id = &records[0].tool_use_id;
        assert_eq!(replaced_id, "big");
        assert!(messages.iter().any(|m| m.tool_call_id.as_deref() == Some("big") && is_content_already_compacted(&m.content)));
    }

    #[test]
    fn test_enforce_budget_reapplies_cached() {
        // Pre-seed state with a replacement
        let mut state = create_content_replacement_state();
        state
            .replacements
            .insert("cached-id".to_string(), "<persisted-output>\ncached\n</persisted-output>".to_string());

        let mut messages = vec![make_tool_message(
            "cached-id",
            "original content that was already replaced",
        )];

        let records = enforce_tool_result_budget(&mut messages, &mut state);
        // No new replacements, but the cached replacement should be re-applied
        assert!(records.is_empty());
        assert_eq!(
            messages[0].content,
            "<persisted-output>\ncached\n</persisted-output>"
        );
    }

    #[test]
    fn test_apply_tool_result_budget_no_state() {
        let mut messages = vec![make_tool_message("id1", &"x".repeat(300_000))];
        let records = apply_tool_result_budget(&mut messages, None);
        assert!(records.is_empty());
        // Content unchanged — no state means budget enforcement is off
        assert_eq!(messages[0].content.len(), 300_000);
    }

    #[test]
    fn test_apply_tool_result_budget_with_state() {
        let mut messages = vec![
            make_tool_message("id1", &"a".repeat(150_000)),
            make_tool_message("id2", &"b".repeat(100_000)),
        ];
        let mut state = create_content_replacement_state();
        let records = apply_tool_result_budget(&mut messages, Some(&mut state));

        // Total 250k > 200k limit → largest (id1, 150k) replaced
        // remaining = 100k < 200k
        assert!(!records.is_empty());
    }

    #[test]
    fn test_reconstruct_content_replacement_state() {
        let messages = vec![
            make_tool_message("id1", "content1"),
            make_tool_message("id2", "content2"),
        ];
        let records = vec![ToolResultReplacementRecord {
            tool_use_id: "id1".to_string(),
            replacement: "<persisted-output>\nreplaced\n</persisted-output>".to_string(),
        }];

        let state = reconstruct_content_replacement_state(&messages, &records);

        // Both IDs should be seen
        assert!(state.seen_ids.contains("id1"));
        assert!(state.seen_ids.contains("id2"));
        // Only id1 should have a replacement
        assert!(state.replacements.contains_key("id1"));
        assert!(!state.replacements.contains_key("id2"));
    }

    #[test]
    fn test_reconstruct_ignores_missing_ids() {
        let messages = vec![make_tool_message("id1", "content")];
        let records = vec![
            ToolResultReplacementRecord {
                tool_use_id: "id1".to_string(),
                replacement: "r1".to_string(),
            },
            ToolResultReplacementRecord {
                tool_use_id: "missing-id".to_string(),
                replacement: "r2".to_string(),
            },
        ];

        let state = reconstruct_content_replacement_state(&messages, &records);
        assert!(state.replacements.contains_key("id1"));
        assert!(!state.replacements.contains_key("missing-id"));
    }

    #[test]
    fn test_build_persisted_message() {
        let content = "x".repeat(10_000);
        let msg = build_persisted_message("test-id", &content);
        assert!(msg.is_some());
        let wrapped = msg.unwrap();
        assert!(wrapped.trim().starts_with(PERSISTED_OUTPUT_TAG));
        assert!(wrapped.trim().ends_with("</persisted-output>"));
        assert!(wrapped.contains(&content.len().to_string()));
    }
}
