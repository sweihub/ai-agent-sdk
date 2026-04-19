// Source: ~/claudecode/openclaudecode/src/utils/messages.ts (lines 5133-5460)
//! Message normalization service for tool_use/tool_result pairing validation.
//!
//! Translated from `ensureToolResultPairing` in the TypeScript claude code SDK.
//! This function runs before each API request to ensure tool_use and tool_result
//! blocks are properly paired, preventing API 400 errors from mismatched IDs.

use crate::constants::messages::NO_CONTENT_MESSAGE;
use crate::utils::messages::{
    AssistantMessage, AssistantMessageContent, ContentBlock, Message, MessageContent, NormalizedMessage,
    NormalizedUserMessage, UserMessageExtra,
};
use std::collections::HashSet;

/// Placeholder content for synthetic tool results injected when a tool_use
/// has no matching tool_result in the message history.
const SYNTHETIC_TOOL_RESULT_PLACEHOLDER: &str =
    "[Synthetic error: tool result missing due to conversation resume / truncation]";

/// Error thrown when strict tool result pairing is enabled and a mismatch
/// is detected. In strict mode we refuse to repair and fail fast instead.
#[derive(Debug, Clone)]
pub struct StrictPairingError {
    pub message_types: String,
}

impl std::fmt::Display for StrictPairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ensureToolResultPairing: tool_use/tool_result pairing mismatch detected (strict mode). \
             Refusing to repair — would inject synthetic placeholders into model context. \
             Message structure: {}",
            self.message_types
        )
    }
}

impl std::error::Error for StrictPairingError {}

/// Check if strict tool result pairing mode is enabled.
///
/// In strict mode, instead of repairing mismatches we throw an error.
/// This is used in development / testing environments.
pub fn get_strict_tool_result_pairing() -> bool {
    // Check environment variable: AI_CODE_STRICT_TOOL_RESULT_PAIRING
    // or the legacy CLAUDE_CODE_STRICT_TOOL_RESULT_PAIRING
    std::env::var("AI_CODE_STRICT_TOOL_RESULT_PAIRING")
        .ok()
        .as_ref()
        .map(|v| v == "true" || v == "1" || v == "yes")
        .or_else(|| {
            std::env::var("CLAUDE_CODE_STRICT_TOOL_RESULT_PAIRING")
                .ok()
                .as_ref()
                .map(|v| v == "true" || v == "1" || v == "yes")
        })
        .unwrap_or(false)
}

// --------------------------------------------------------------------------
// Helper functions for JSON values (assistant message content)
// --------------------------------------------------------------------------

/// Helper: extract tool_use_id from a JSON value if it's a tool_result block.
fn json_tool_result_id(block: &serde_json::Value) -> Option<String> {
    block
        .get("type")
        .and_then(|t| t.as_str())
        .filter(|t| *t == "tool_result")
        .and_then(|_| block.get("tool_use_id"))
        .and_then(|id| id.as_str())
        .map(String::from)
}

/// Helper: extract id from a server_tool_use or mcp_tool_use block.
fn json_server_tool_use_id(block: &serde_json::Value) -> Option<String> {
    let block_type = block.get("type")?.as_str()?;
    if block_type == "server_tool_use" || block_type == "mcp_tool_use" {
        block.get("id").and_then(|id| id.as_str()).map(String::from)
    } else {
        None
    }
}

/// Helper: check if a JSON value is a tool_use block and extract its id.
fn json_tool_use_id(block: &serde_json::Value) -> Option<String> {
    block
        .get("type")
        .and_then(|t| t.as_str())
        .filter(|t| *t == "tool_use")
        .and_then(|_| block.get("id"))
        .and_then(|id| id.as_str())
        .map(String::from)
}

/// Helper: check if a JSON value is a tool_result block.
fn json_is_tool_result(block: &serde_json::Value) -> bool {
    block.get("type").and_then(|t| t.as_str()) == Some("tool_result")
}

/// Helper: check if a JSON value is a tool_use block.
fn json_is_tool_use(block: &serde_json::Value) -> bool {
    block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
}

/// Helper: check if a JSON value is an orphaned server tool use.
fn json_is_orphaned_server_tool_use(
    block: &serde_json::Value,
    server_result_ids: &HashSet<String>,
) -> bool {
    let block_type = block.get("type").and_then(|t| t.as_str());
    match block_type {
        Some("server_tool_use" | "mcp_tool_use") => {
            let id = block.get("id").and_then(|i| i.as_str());
            id.map(|id| !server_result_ids.contains(id)).unwrap_or(false)
        }
        _ => false,
    }
}

// --------------------------------------------------------------------------
// Helper functions for ContentBlock enum (user message content)
// --------------------------------------------------------------------------

/// Extract tool_use_id from a ContentBlock if it's a ToolResult variant.
fn content_block_tool_result_id(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
        _ => None,
    }
}

/// Check if a ContentBlock is a tool_result variant.
fn content_block_is_tool_result(block: &ContentBlock) -> bool {
    matches!(block, ContentBlock::ToolResult { .. })
}

/// Helper: get the content blocks from a user message's MessageContent as JSON values.
fn user_message_to_json_blocks(content: &MessageContent) -> Vec<serde_json::Value> {
    match content {
        MessageContent::Blocks(blocks) => {
            blocks
                .iter()
                .map(|b| serde_json::to_value(b).unwrap_or_else(|_| serde_json::json!({})))
                .collect()
        }
        MessageContent::String(s) => {
            vec![serde_json::json!({"type": "text", "text": s})]
        }
    }
}

/// Helper: check if content has any tool_result blocks.
fn content_has_tool_results(content: &MessageContent) -> bool {
    match content {
        MessageContent::Blocks(blocks) => blocks.iter().any(content_block_is_tool_result),
        MessageContent::String(_) => false,
    }
}

/// Create a NormalizedUserMessage with the given content and is_meta flag.
fn create_normalized_user(
    content: MessageContent,
    is_meta: bool,
) -> NormalizedUserMessage {
    NormalizedUserMessage {
        message: content,
        extra: UserMessageExtra {
            is_meta: Some(is_meta),
            is_visible_in_transcript_only: None,
            is_virtual: None,
            is_compact_summary: None,
            summarize_metadata: None,
            tool_use_result: None,
            mcp_meta: None,
            uuid: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            image_paste_ids: None,
            source_tool_assistant_uuid: None,
            permission_mode: None,
            origin: None,
            parent_uuid: None,
        },
    }
}

// --------------------------------------------------------------------------
// Main function
// --------------------------------------------------------------------------

/// Ensure tool_use and tool_result blocks are properly paired in the message list.
///
/// This function runs before each API request to repair common issues:
/// 1. Dedup tool_use IDs across all assistant messages
/// 2. Strip orphaned server_tool_use / mcp_tool_use blocks (no matching result)
/// 3. Inject synthetic error tool_result for missing tool_results
/// 4. Strip orphaned tool_results (no matching tool_use)
/// 5. Dedup duplicate tool_result blocks with the same tool_use_id
/// 6. In strict mode, throw instead of repairing
/// 7. Insert placeholder text when stripping empties content arrays
///
/// # Arguments
/// * `messages` - The list of normalized messages (UserMessage | AssistantMessage)
///
/// # Returns
/// A new list of messages with all pairing issues repaired, or an error in strict mode.
pub fn ensure_tool_result_pairing(
    messages: &[NormalizedMessage],
) -> Result<Vec<NormalizedMessage>, StrictPairingError> {
    let strict_mode = get_strict_tool_result_pairing();
    let mut result: Vec<NormalizedMessage> = Vec::new();
    let mut repaired = false;

    // Cross-message tool_use ID tracking. The per-message seenToolUseIds below
    // only caught duplicates within a single assistant's content array (the
    // normalizeMessagesForAPI-merged case). When two assistants with DIFFERENT
    // message.id carry the same tool_use ID — e.g. orphan handler re-pushed an
    // assistant already present in mutableMessages with a fresh message.id, or
    // normalizeMessagesForAPI's backward walk broke on an intervening user
    // message — the dup lived in separate result entries and the API rejected
    // with "tool_use ids must be unique", deadlocking the session.
    let mut all_seen_tool_use_ids: HashSet<String> = HashSet::new();

    for i in 0..messages.len() {
        let msg = &messages[i];

        match msg {
            NormalizedMessage::Assistant(assistant) => {
                // assistant.message.content is Vec<serde_json::Value> (raw JSON)
                let content_vec = &assistant.message.content;

                // Collect server-side tool result IDs (blocks with tool_use_id in the same message)
                let server_result_ids: HashSet<String> = content_vec
                    .iter()
                    .filter_map(|block| json_tool_result_id(block))
                    .collect();

                // Dedupe tool_use blocks by ID. Checks against cross-message allSeenToolUseIds.
                // Also strip orphaned server-side tool use blocks.
                let mut seen_tool_use_ids: HashSet<String> = HashSet::new();
                let mut final_content: Vec<serde_json::Value> = Vec::new();

                for block in content_vec {
                    let block_type = block.get("type").and_then(|t| t.as_str());
                    let mut should_keep = true;

                    // Check for tool_use blocks
                    if block_type == Some("tool_use") {
                        if let Some(tool_id) = json_tool_use_id(block) {
                            if all_seen_tool_use_ids.contains(&tool_id) {
                                // Duplicate tool_use across messages — strip it
                                repaired = true;
                                should_keep = false;
                            } else {
                                all_seen_tool_use_ids.insert(tool_id.clone());
                                seen_tool_use_ids.insert(tool_id);
                            }
                        }
                    }

                    // Check for orphaned server tool uses
                    if should_keep
                        && json_is_orphaned_server_tool_use(block, &server_result_ids)
                    {
                        repaired = true;
                        should_keep = false;
                    }

                    if should_keep {
                        final_content.push(block.clone());
                    }
                }

                let assistant_content_changed = final_content.len() != content_vec.len();

                // If stripping orphaned server tool uses empties the content array,
                // insert a placeholder so the API doesn't reject empty assistant content.
                if final_content.is_empty() {
                    final_content.push(serde_json::json!({
                        "type": "text",
                        "text": "[Tool use interrupted]",
                    }));
                }

                // Build the modified assistant message if content changed
                let assistant_msg = if assistant_content_changed {
                    let mut new_content = final_content;
                    // If there's just one text placeholder block, store as string content
                    if new_content.len() == 1 {
                        if let Some(text) = new_content[0].get("text").and_then(|t| t.as_str())
                            && new_content[0].get("type").and_then(|t| t.as_str()) == Some("text")
                        {
                            let mut new_assistant = assistant.clone();
                            new_assistant.message.content = vec![new_content.remove(0)];
                            return if repaired && strict_mode {
                                let message_types = build_message_types(messages);
                                Err(StrictPairingError { message_types })
                            } else {
                                result.push(NormalizedMessage::Assistant(new_assistant));
                                // Process next-message pairing for the original content
                                let tool_use_ids: Vec<String> =
                                    seen_tool_use_ids.into_iter().collect();
                                process_next_message_pairing(
                                    &messages,
                                    i,
                                    &tool_use_ids,
                                    &mut result,
                                    &mut repaired,
                                    strict_mode,
                                );
                                return Ok(result);
                            };
                        }
                    }
                    let mut new_assistant = assistant.clone();
                    new_assistant.message.content = new_content;
                    NormalizedMessage::Assistant(new_assistant)
                } else {
                    msg.clone()
                };

                result.push(assistant_msg);

                // Collect tool_use IDs from this assistant message for next-message pairing
                let tool_use_ids: Vec<String> = seen_tool_use_ids.into_iter().collect();

                // Check the next message for matching tool_results and repair
                process_next_message_pairing(
                    &messages,
                    i,
                    &tool_use_ids,
                    &mut result,
                    &mut repaired,
                    strict_mode,
                );
            }
            NormalizedMessage::User(user) => {
                // A user message with tool_result blocks but NO preceding assistant
                // message in the output has orphaned tool_results.
                if result
                    .last()
                    .map_or(true, |m| !matches!(m, NormalizedMessage::Assistant(_)))
                {
                    if content_has_tool_results(&user.message) {
                        match &user.message {
                            MessageContent::Blocks(blocks) => {
                                let stripped: Vec<ContentBlock> = blocks
                                    .iter()
                                    .filter(|block| !content_block_is_tool_result(block))
                                    .cloned()
                                    .collect();

                                if stripped.len() != blocks.len() {
                                    // We stripped some orphaned tool_results
                                    repaired = true;

                                    if !stripped.is_empty() {
                                        let mut new_user = user.clone();
                                        new_user.message = MessageContent::Blocks(stripped);
                                        result.push(NormalizedMessage::User(new_user));
                                    } else if result.is_empty() {
                                        // If stripping emptied the message and nothing has been
                                        // pushed yet, keep a placeholder so the payload still
                                        // starts with a user message.
                                        let placeholder =
                                            NormalizedMessage::User(create_normalized_user(
                                                MessageContent::String(
                                                    NO_CONTENT_MESSAGE.to_string(),
                                                ),
                                                true,
                                            ));
                                        result.push(placeholder);
                                    }
                                    // If result is non-empty but stripped is empty, skip the user message
                                    continue;
                                }
                            }
                            MessageContent::String(_) => {
                                // Text content user message — no tool_results to strip
                            }
                        }
                    }
                }
                result.push(msg.clone());
            }
            NormalizedMessage::Progress(_)
            | NormalizedMessage::System(_)
            | NormalizedMessage::Attachment(_) => {
                // Non-user/assistant messages pass through unchanged
                result.push(msg.clone());
            }
        }
    }

    // After processing, check strict mode
    if repaired && strict_mode {
        let message_types = build_message_types(messages);
        return Err(StrictPairingError { message_types });
    }

    Ok(result)
}

/// Process the next user message for tool_result pairing.
/// This is extracted to avoid code duplication when the assistant content changes.
fn process_next_message_pairing(
    messages: &[NormalizedMessage],
    assistant_idx: usize,
    tool_use_ids: &[String],
    result: &mut Vec<NormalizedMessage>,
    repaired: &mut bool,
    _strict_mode: bool,
) {
    // Check the next message for matching tool_results
    let next_msg = messages.get(assistant_idx + 1);
    let mut existing_tool_result_ids: HashSet<String> = HashSet::new();
    let mut has_duplicate_tool_results = false;

    if let Some(NormalizedMessage::User(user)) = next_msg {
        match &user.message {
            MessageContent::Blocks(blocks) => {
                for block in blocks {
                    if let Some(tr_id) = content_block_tool_result_id(block) {
                        if existing_tool_result_ids.contains(&tr_id) {
                            has_duplicate_tool_results = true;
                        }
                        existing_tool_result_ids.insert(tr_id);
                    }
                }
            }
            MessageContent::String(_) => {}
        }
    }

    // Find missing tool_result IDs (forward direction: tool_use without tool_result)
    let tool_use_id_set: HashSet<String> = tool_use_ids.iter().cloned().collect();
    let missing_ids: Vec<String> = tool_use_ids
        .iter()
        .filter(|id| !existing_tool_result_ids.contains(*id))
        .cloned()
        .collect();

    // Find orphaned tool_result IDs (reverse direction: tool_result without tool_use)
    let orphaned_ids: Vec<String> = existing_tool_result_ids
        .iter()
        .filter(|id| !tool_use_id_set.contains(*id))
        .cloned()
        .collect();

    if missing_ids.is_empty() && orphaned_ids.is_empty() && !has_duplicate_tool_results {
        return;
    }

    *repaired = true;

    // Build synthetic error tool_result blocks for missing IDs
    // Note: ContentBlock::ToolResult expects content as Vec<ContentBlock>,
    // so we wrap the placeholder text in a text block.
    let synthetic_blocks: Vec<serde_json::Value> = missing_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": [{"type": "text", "text": SYNTHETIC_TOOL_RESULT_PLACEHOLDER}],
                "is_error": true,
            })
        })
        .collect();

    if let Some(NormalizedMessage::User(user)) = next_msg {
        // Next message is already a user message - patch it
        let mut content: Vec<serde_json::Value> = user_message_to_json_blocks(&user.message);

        // Strip orphaned tool_results and dedupe duplicate tool_result IDs
        if !orphaned_ids.is_empty() || has_duplicate_tool_results {
            let orphaned_set: HashSet<String> = orphaned_ids.into_iter().collect();
            let mut seen_tr_ids: HashSet<String> = HashSet::new();
            content = content
                .into_iter()
                .filter(|block| {
                    if json_is_tool_result(block) {
                        if let Some(tr_id) = json_tool_result_id(block) {
                            if orphaned_set.contains(&tr_id) {
                                return false;
                            }
                            if seen_tr_ids.contains(&tr_id) {
                                return false;
                            }
                            seen_tr_ids.insert(tr_id);
                        }
                    }
                    true
                })
                .collect();
        }

        let patched_content: Vec<serde_json::Value> =
            synthetic_blocks.into_iter().chain(content.into_iter()).collect();

        // If content is now empty after stripping orphans, insert a placeholder user message
        if !patched_content.is_empty() {
            // Convert patched JSON blocks back to ContentBlock format
            let patched_blocks: Vec<ContentBlock> = patched_content
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            let mut patched_user = user.clone();
            if !patched_blocks.is_empty() {
                patched_user.message = MessageContent::Blocks(patched_blocks);
            } else {
                patched_user.message = MessageContent::String(NO_CONTENT_MESSAGE.to_string());
            }
            let patched_next = NormalizedMessage::User(patched_user);
            result.push(patched_next);
        } else {
            // Content is empty after stripping orphaned tool_results.
            // Insert a placeholder user message to maintain role alternation.
            let placeholder_msg = NormalizedMessage::User(create_normalized_user(
                MessageContent::String(NO_CONTENT_MESSAGE.to_string()),
                true,
            ));
            result.push(placeholder_msg);
        }
    } else {
        // No user message follows - insert a synthetic user message (only if missing IDs)
        if !synthetic_blocks.is_empty() {
            // Convert synthetic blocks to ContentBlock format
            let synthetic_blocks_as_content: Vec<ContentBlock> = synthetic_blocks
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            let synthetic_user = NormalizedMessage::User(create_normalized_user(
                MessageContent::Blocks(synthetic_blocks_as_content),
                true,
            ));
            result.push(synthetic_user);
        }
    }
}

/// Build diagnostic message type strings for error reporting.
fn build_message_types(messages: &[NormalizedMessage]) -> String {
    messages
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            match m {
                NormalizedMessage::Assistant(assistant) => {
                    let tool_uses: Vec<String> = assistant
                        .message
                        .content
                        .iter()
                        .filter_map(|b| json_tool_use_id(b))
                        .collect();
                    let server_tool_uses: Vec<String> = assistant
                        .message
                        .content
                        .iter()
                        .filter_map(json_server_tool_use_id)
                        .collect();
                    let mut parts = vec![
                        format!("id={}", assistant.message.id),
                        format!("tool_uses=[{}]", tool_uses.join(",")),
                    ];
                    if !server_tool_uses.is_empty() {
                        parts.push(format!(
                            "server_tool_uses=[{}]",
                            server_tool_uses.join(",")
                        ));
                    }
                    format!("[{}] assistant({})", idx, parts.join(", "))
                }
                NormalizedMessage::User(user) => {
                    if let MessageContent::Blocks(blocks) = &user.message {
                        let tool_results: Vec<String> = blocks
                            .iter()
                            .filter_map(content_block_tool_result_id)
                            .collect();
                        if !tool_results.is_empty() {
                            return format!(
                                "[{}] user(tool_results=[{}])",
                                idx,
                                tool_results.join(",")
                            );
                        }
                    }
                    match m {
                        NormalizedMessage::User(_) => format!("[{}] user", idx),
                        NormalizedMessage::Progress(_) => format!("[{}] progress", idx),
                        NormalizedMessage::System(_) => format!("[{}] system", idx),
                        NormalizedMessage::Attachment(_) => format!("[{}] attachment", idx),
                        NormalizedMessage::Assistant(_) => unreachable!(),
                    }
                }
                NormalizedMessage::Progress(_) => format!("[{}] progress", idx),
                NormalizedMessage::System(_) => format!("[{}] system", idx),
                NormalizedMessage::Attachment(_) => format!("[{}] attachment", idx),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::messages::{
        AssistantMessageContent, AssistantMessageExtra, NormalizedAssistantMessage, Usage,
    };

    fn make_assistant_msg(id: &str, content: Vec<serde_json::Value>) -> NormalizedMessage {
        NormalizedMessage::Assistant(NormalizedAssistantMessage {
            message: AssistantMessageContent {
                id: id.to_string(),
                container: None,
                model: "claude-sonnet-4-20250514".to_string(),
                role: "assistant".to_string(),
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                message_type: "message".to_string(),
                usage: Some(Usage::default()),
                content,
                context_management: None,
            },
            extra: AssistantMessageExtra {
                request_id: None,
                api_error: None,
                error: None,
                error_details: None,
                is_api_error_message: Some(false),
                is_virtual: None,
                is_meta: None,
                advisor_model: None,
                uuid: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                parent_uuid: None,
            },
        })
    }

    fn make_normalized_user(content: Vec<ContentBlock>) -> NormalizedMessage {
        NormalizedMessage::User(NormalizedUserMessage {
            message: MessageContent::Blocks(content),
            extra: UserMessageExtra {
                is_meta: None,
                is_visible_in_transcript_only: None,
                is_virtual: None,
                is_compact_summary: None,
                summarize_metadata: None,
                tool_use_result: None,
                mcp_meta: None,
                uuid: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                image_paste_ids: None,
                source_tool_assistant_uuid: None,
                permission_mode: None,
                origin: None,
                parent_uuid: None,
            },
        })
    }

    fn make_tool_result_block(tool_use_id: &str, _content: &str) -> ContentBlock {
        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: None,
            is_error: Some(false),
        }
    }
}

/// Merges system-reminder text blocks into the last tool_result within a user message's content.
///
/// This is the Rust port of TypeScript's `smooshSystemReminderSiblings` from
/// `~/claudecode/openclaudecode/src/utils/messages.ts`. It collects text blocks
/// starting with `<system-reminder>` from user messages that contain tool_results,
/// and smooshes them into the LAST tool_result's content.
///
/// Returns the (possibly modified) message, or the original unchanged.
pub fn smoosh_system_reminder_siblings(
    messages: &[NormalizedMessage],
) -> Vec<NormalizedMessage> {
    messages
        .iter()
        .map(|msg| match msg {
            NormalizedMessage::User(user) => {
                let content = &user.message;
                let MessageContent::Blocks(blocks) = content else {
                    return msg.clone();
                };

                if !blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. })) {
                    return msg.clone();
                }

                let mut sr_texts: Vec<String> = Vec::new();
                let mut kept: Vec<ContentBlock> = Vec::new();
                for b in blocks {
                    if let ContentBlock::Text { text } = b {
                        if text.starts_with("<system-reminder>") {
                            sr_texts.push(text.clone());
                        } else {
                            kept.push(b.clone());
                        }
                    } else {
                        kept.push(b.clone());
                    }
                }

                if sr_texts.is_empty() {
                    return msg.clone();
                }

                // Smoosh into the LAST tool_result
                let last_tr_idx = kept.iter().rposition(|b| matches!(b, ContentBlock::ToolResult { .. }));
                let last_tr_idx = match last_tr_idx {
                    Some(idx) => idx,
                    None => return msg.clone(),
                };

                let smooshed = match &kept[last_tr_idx] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content: existing_content,
                        is_error,
                    } => smoosh_into_tool_result(tool_use_id, existing_content, is_error, &sr_texts),
                    _ => return msg.clone(),
                };

                match smooshed {
                    None => msg.clone(),
                    Some(new_block) => {
                        let mut new_content = kept.clone();
                        new_content[last_tr_idx] = new_block;
                        NormalizedMessage::User(NormalizedUserMessage {
                            message: MessageContent::Blocks(new_content),
                            extra: user.extra.clone(),
                        })
                    }
                }
            }
            _ => msg.clone(),
        })
        .collect()
}

/// Smoosh text blocks into a tool_result's content.
/// Returns None if the tool_result has a tool_reference (which would cause API errors).
fn smoosh_into_tool_result(
    tool_use_id: &str,
    existing_content: &Option<Vec<ContentBlock>>,
    is_error: &Option<bool>,
    blocks: &[String],
) -> Option<ContentBlock> {
    if blocks.is_empty() {
        return Some(ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: existing_content.clone(),
            is_error: *is_error,
        });
    }

    // Check for tool_reference — can't smoosh into tool_refs
    if let Some(ref existing) = existing_content {
        if existing.iter().any(|b| matches!(b, ContentBlock::ToolReference { .. })) {
            return None;
        }
    }

    // API constraint: is_error tool_results must contain only text blocks.
    // Filter out non-text blocks (like images) — they arrive as proper user
    // content anyway.
    let is_error = is_error == &Some(true);
    if is_error {
        let text_blocks: Vec<String> = blocks
            .iter()
            .filter(|t| !t.is_empty())
            .cloned()
            .collect();
        if text_blocks.is_empty() {
            return Some(ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: existing_content.clone(),
                is_error: *is_error,
            });
        }

        // Rebuild with only text blocks
        let new_content: Option<Vec<ContentBlock>> = Some(vec![ContentBlock::Text {
            text: text_blocks.join("\n\n"),
        }]);
        return Some(ContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: new_content,
            is_error: Some(true),
        });
    }

    let all_text = blocks.iter().all(|b| !b.is_empty());

    // Preserve string shape when existing was None/empty and all incoming are text
    if all_text && (existing_content.is_none() || matches!(existing_content, Some(v) if v.iter().all(|c| matches!(c, ContentBlock::Text { .. })))) {
        let existing_texts: Vec<String> = match existing_content {
            Some(ref v) => v.iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        if text.trim().is_empty() { None } else { Some(text.trim().to_string()) }
                    } else { None }
                })
                .collect(),
            None => Vec::new(),
        };

        let joined: Vec<String> = [existing_texts, blocks.iter().filter(|b| !b.is_empty()).cloned().collect()].concat();
        let text = joined.iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");

        if !text.is_empty() {
            return Some(ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: Some(vec![ContentBlock::Text { text }]),
                is_error: *is_error,
            });
        }
    }

    // General case: normalize to array, concat, merge adjacent text
    let base: Vec<ContentBlock> = match existing_content {
        None => vec![],
        Some(ref v) if v.is_empty() => vec![],
        Some(ref v) => v.clone(),
    };

    let merged: Vec<ContentBlock> = {
        let mut all: Vec<ContentBlock> = base;
        for b in blocks {
            let t = b.trim().to_string();
            if t.is_empty() { continue; }
            if let Some(last) = all.last_mut() {
                if let ContentBlock::Text { text: ref mut txt } = last {
                    *txt = format!("{}\n\n{}", txt, t);
                    continue;
                }
            }
            all.push(ContentBlock::Text { text: t });
        }
        all
    };

    Some(ContentBlock::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        content: Some(merged),
        is_error: *is_error,
    })
}

    #[test]
    fn test_empty_messages() {
        let messages: Vec<NormalizedMessage> = vec![];
        let result = ensure_tool_result_pairing(&messages).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_tool_uses_no_changes() {
        let messages = vec![
            make_normalized_user(vec![ContentBlock::Text {
                text: "hello".to_string(),
            }]),
            make_assistant_msg("msg-1", vec![serde_json::json!({"type": "text", "text": "hi"})]),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_tool_use_without_result_injects_synthetic() {
        let messages = vec![
            make_normalized_user(vec![ContentBlock::Text {
                text: "hello".to_string(),
            }]),
            make_assistant_msg(
                "msg-1",
                vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "Bash",
                    "input": {"command": "ls"}
                })],
            ),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // Should have 3 messages: user, assistant, synthetic user with tool_result
        assert_eq!(result.len(), 3);

        if let NormalizedMessage::User(synthetic) = &result[2] {
            match &synthetic.message {
                MessageContent::Blocks(blocks) => {
                    assert_eq!(blocks.len(), 1);
                    assert!(matches!(
                        &blocks[0],
                        ContentBlock::ToolResult { tool_use_id, is_error, .. }
                        if tool_use_id == "tool-1" && *is_error == Some(true)
                    ));
                }
                MessageContent::String(_) => panic!("Expected blocks"),
            }
        } else {
            panic!("Expected user message");
        }
    }

    #[test]
    fn test_tool_use_with_matching_result() {
        let tool_result = make_tool_result_block("tool-1", "file1 file2");

        let messages = vec![
            make_normalized_user(vec![ContentBlock::Text {
                text: "hello".to_string(),
            }]),
            make_assistant_msg(
                "msg-1",
                vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "Bash",
                    "input": {"command": "ls"}
                })],
            ),
            make_normalized_user(vec![tool_result]),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_duplicate_tool_use_across_assistant_messages() {
        let tool_result = make_tool_result_block("tool-1", "result");

        // Two assistant messages with the same tool_use ID
        let messages = vec![
            make_assistant_msg(
                "msg-1",
                vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "Bash",
                    "input": {"command": "ls"}
                })],
            ),
            make_normalized_user(vec![tool_result]),
            // Same tool_use_id in a later assistant (different message.id)
            make_assistant_msg(
                "msg-2",
                vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "Bash",
                    "input": {"command": "ls"}
                })],
            ),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // The second assistant's duplicate tool_use should be stripped
        if let NormalizedMessage::Assistant(asst) = &result[2] {
            let tool_uses: Vec<_> = asst
                .message
                .content
                .iter()
                .filter(|b| json_is_tool_use(b))
                .collect();
            assert!(tool_uses.is_empty(), "Duplicate tool_use should be stripped");
        }
    }

    #[test]
    fn test_orphaned_server_tool_use_stripped() {
        let messages = vec![make_assistant_msg(
            "msg-1",
            vec![serde_json::json!({
                "type": "server_tool_use",
                "id": "server-1",
                "name": "web_search",
                "input": {"query": "test"}
            })],
        )];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // The orphaned server_tool_use should be stripped, leaving just a placeholder
        if let NormalizedMessage::Assistant(asst) = &result[0] {
            let server_uses: Vec<_> = asst
                .message
                .content
                .iter()
                .filter(|b| {
                    b.get("type")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t == "server_tool_use" || t == "mcp_tool_use")
                })
                .collect();
            assert!(
                server_uses.is_empty(),
                "Orphaned server_tool_use should be stripped"
            );
        }
    }

    #[test]
    fn test_multiple_missing_tool_results() {
        let messages = vec![
            make_normalized_user(vec![ContentBlock::Text {
                text: "hello".to_string(),
            }]),
            make_assistant_msg(
                "msg-1",
                vec![
                    serde_json::json!({
                        "type": "tool_use",
                        "id": "tool-1",
                        "name": "Read",
                        "input": {"path": "/etc/hosts"}
                    }),
                    serde_json::json!({
                        "type": "tool_use",
                        "id": "tool-2",
                        "name": "Bash",
                        "input": {"command": "ls"}
                    }),
                ],
            ),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // Should have: user, assistant, synthetic user with 2 tool_results
        assert_eq!(result.len(), 3);

        if let NormalizedMessage::User(synthetic) = &result[2] {
            match &synthetic.message {
                MessageContent::Blocks(blocks) => {
                    assert_eq!(blocks.len(), 2);
                    for block in blocks {
                        if let ContentBlock::ToolResult { tool_use_id, is_error, .. } = block {
                            assert!(tool_use_id == "tool-1" || tool_use_id == "tool-2");
                            assert_eq!(*is_error, Some(true));
                        }
                    }
                }
                _ => panic!("Expected blocks"),
            }
        }
    }

    #[test]
    fn test_all_server_tool_uses_stripped_leaves_placeholder() {
        let messages = vec![make_assistant_msg(
            "msg-1",
            vec![serde_json::json!({
                "type": "server_tool_use",
                "id": "server-1",
                "name": "web_search",
                "input": {"query": "test"}
            })],
        )];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // After stripping, the content should have just a placeholder text block
        if let NormalizedMessage::Assistant(asst) = &result[0] {
            assert!(!asst.message.content.is_empty());
            // Should be a text block with "[Tool use interrupted]"
            let has_placeholder = asst
                .message
                .content
                .iter()
                .any(|b| {
                    b.get("type").and_then(|t| t.as_str()) == Some("text")
                        && b.get("text")
                            .and_then(|t| t.as_str())
                            == Some("[Tool use interrupted]")
                });
            assert!(has_placeholder, "Should have [Tool use interrupted] placeholder");
        }
    }

    #[test]
    fn test_orphaned_user_at_start() {
        // User message at index 0 with tool_result (no preceding assistant)
        let messages = vec![make_normalized_user(vec![make_tool_result_block(
            "orphan-1",
            "result",
        )])];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // Should have a placeholder message since the tool_result was stripped
        assert!(!result.is_empty());
    }

    #[test]
    fn test_tool_result_with_matching_tool_use_preserved() {
        let tool_result = make_tool_result_block("tool-1", "hello world");

        let messages = vec![
            make_normalized_user(vec![ContentBlock::Text {
                text: "hello".to_string(),
            }]),
            make_assistant_msg(
                "msg-1",
                vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "Bash",
                    "input": {"command": "echo hello"}
                })],
            ),
            make_normalized_user(vec![tool_result]),
        ];
        let result = ensure_tool_result_pairing(&messages).unwrap();
        assert_eq!(result.len(), 3);

        // Verify the tool_result is still in the user message
        if let NormalizedMessage::User(user) = &result[2] {
            match &user.message {
                MessageContent::Blocks(blocks) => {
                    assert_eq!(blocks.len(), 1);
                    assert!(matches!(&blocks[0], ContentBlock::ToolResult { tool_use_id, .. }
                        if tool_use_id == "tool-1"));
                }
                _ => panic!("Expected blocks"),
            }
        }
    }

    #[test]
    fn test_server_tool_use_with_result_preserved() {
        let server_tool_use = serde_json::json!({
            "type": "server_tool_use",
            "id": "server-1",
            "name": "web_search",
            "input": {"query": "test"}
        });
        let server_tool_result = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "server-1",
            "content": "search results",
        });

        let messages = vec![make_assistant_msg(
            "msg-1",
            vec![server_tool_use.clone(), server_tool_result],
        )];
        let result = ensure_tool_result_pairing(&messages).unwrap();

        // The server_tool_use has a matching result, so it should be preserved
        if let NormalizedMessage::Assistant(asst) = &result[0] {
            let server_uses: Vec<_> = asst
                .message
                .content
                .iter()
                .filter(|b| {
                    b.get("type")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t == "server_tool_use" || t == "mcp_tool_use")
                })
                .collect();
            assert_eq!(
                server_uses.len(),
                1,
                "Server tool_use with matching result should be preserved"
            );
        }
    }
}
