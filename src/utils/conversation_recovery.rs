// Source: ~/claudecode/openclaudecode/src/utils/conversationRecovery.ts

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Represents a recovered conversation state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationRecovery {
    /// Deserialized message history
    pub messages: Vec<serde_json::Value>,
    /// Turn interruption state detected during recovery
    pub turn_interruption_state: TurnInterruptionState,
    /// Session ID if available
    pub session_id: Option<String>,
}

/// Represents the state of a turn interruption detected during conversation recovery.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TurnInterruptionState {
    /// No interruption detected
    None,
    /// Session was interrupted mid-prompt with the interrupted message
    InterruptedPrompt { message: serde_json::Value },
}

/// Result of deserializing messages with interrupt detection.
#[derive(Clone, Debug)]
pub struct DeserializeResult {
    /// Filtered and normalized messages
    pub messages: Vec<serde_json::Value>,
    /// Detected turn interruption state
    pub turn_interruption_state: TurnInterruptionState,
}

/// No-response sentinel content used when a conversation needs to be API-valid.
pub const NO_RESPONSE_REQUESTED: &str = "(no response requested)";

/// Continuation message appended when a turn was interrupted mid-stream.
pub const CONTINUE_PROMPT: &str = "Continue from where you left off.";

/// Migrate legacy attachment types to current types for backward compatibility.
///
/// Transforms old `new_file` and `new_directory` attachment types to current `file`
/// and `directory` types, adding displayPath when missing.
fn migrate_legacy_attachment(message: &serde_json::Value) -> serde_json::Value {
    let msg_type = message.get("type").and_then(|v| v.as_str());
    if msg_type != Some("attachment") {
        return message.clone();
    }

    let mut msg = message.clone();
    let attachment = msg.get_mut("attachment");

    if let Some(attachment) = attachment {
        let att_type = attachment.get("type").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Transform legacy new_file type
        if att_type == Some("new_file".to_string()) {
            let filename: Option<String> =
                attachment.get("filename").and_then(|v| v.as_str()).map(|s| s.to_string());
            if let Some(fn_str) = filename {
                if let Some(obj) = attachment.as_object_mut() {
                    obj.insert("type".to_string(), serde_json::json!("file"));
                    obj.insert("displayPath".to_string(), serde_json::json!(fn_str));
                }
            }
            return msg;
        }

        // Transform legacy new_directory type
        if att_type == Some("new_directory".to_string()) {
            let path: Option<String> =
                attachment.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
            if let Some(p) = path {
                if let Some(obj) = attachment.as_object_mut() {
                    obj.insert("type".to_string(), serde_json::json!("directory"));
                    obj.insert("displayPath".to_string(), serde_json::json!(p));
                }
            }
            return msg;
        }

        // Backfill displayPath for attachments from old sessions
        if attachment.get("displayPath").is_none() {
            let path: Option<String> = attachment
                .get("filename")
                .or_else(|| attachment.get("path"))
                .or_else(|| attachment.get("skillDir"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(p) = path {
                if let Some(obj) = attachment.as_object_mut() {
                    obj.insert("displayPath".to_string(), serde_json::json!(p));
                }
            }
        }
    }

    msg
}

/// Strip invalid permissionMode values from deserialized user messages.
fn strip_invalid_permission_modes(messages: &mut Vec<serde_json::Value>) {
    let valid_modes: HashSet<&str> = ["default", "auto", "auto-accept", "auto-deny", "bypass"]
        .iter()
        .cloned()
        .collect();

    for msg in messages.iter_mut() {
        if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
            if msg_type == "user" {
                if let Some(msg_obj) = msg.as_object_mut() {
                    if let Some(pm) = msg_obj.get("permissionMode").and_then(|v| v.as_str()) {
                        if !valid_modes.contains(pm) {
                            msg_obj.remove("permissionMode");
                        }
                    }
                }
            }
        }
    }
}

/// Filter out unresolved tool uses from message history.
/// Removes assistant messages whose tool_use blocks have no matching tool_result,
/// and any synthetic messages that follow them.
fn filter_unresolved_tool_uses(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    // First pass: collect all tool_result IDs
    let mut tool_result_ids: HashSet<String> = HashSet::new();
    for msg in messages {
        if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
            if msg_type == "user" {
                if let Some(content) = msg.get("message").and_then(|v| v.get("content")) {
                    if let Some(blocks) = content.as_array() {
                        for block in blocks {
                            if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                                if t == "tool_result" {
                                    if let Some(id) =
                                        block.get("tool_use_id").and_then(|v| v.as_str())
                                    {
                                        tool_result_ids.insert(id.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Second pass: filter out assistant messages with unmatched tool_uses
    let mut result = Vec::new();
    for msg in messages {
        if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
            if msg_type == "assistant" {
                if let Some(content) = msg.get("message").and_then(|v| v.get("content")) {
                    if let Some(blocks) = content.as_array() {
                        let has_unresolved = blocks.iter().any(|block| {
                            if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                                if t == "tool_use" {
                                    if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                                        return !tool_result_ids.contains(id);
                                    }
                                }
                            }
                            false
                        });
                        if has_unresolved {
                            continue;
                        }
                    }
                }
            }
        }
        result.push(msg.clone());
    }
    result
}

/// Filter out orphaned thinking-only assistant messages that can cause API errors
/// during resume. These occur when streaming yields separate messages per content
/// block and interleaved user messages prevent proper merging by message.id.
fn filter_orphaned_thinking_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    // Remove assistant messages that contain only thinking/redacted_thinking content
    // with no text or tool_use blocks, as these orphans can cause API validation errors.
    messages
        .iter()
        .filter(|msg| {
            if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
                if msg_type == "assistant" {
                    if let Some(content) = msg.get("message").and_then(|v| v.get("content")) {
                        if let Some(blocks) = content.as_array() {
                            // If ALL blocks are thinking types, it's an orphan
                            let all_thinking = blocks.iter().all(|block| {
                                if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                                    t == "thinking" || t == "redacted_thinking"
                                } else {
                                    false
                                }
                            });
                            return !all_thinking || blocks.is_empty();
                        }
                    }
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Filter out assistant messages with only whitespace text content.
/// This can happen when model outputs "\n\n" before thinking, user cancels mid-stream.
fn filter_whitespace_only_assistant_messages(
    messages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter(|msg| {
            if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
                if msg_type == "assistant" {
                    if let Some(content) = msg.get("message").and_then(|v| v.get("content")) {
                        // Check if all text content blocks contain only whitespace
                        if let Some(blocks) = content.as_array() {
                            let all_whitespace = blocks.iter().all(|block| {
                                if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                                    if t == "text" {
                                        block
                                            .get("text")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.trim().is_empty())
                                            .unwrap_or(true)
                                    } else {
                                        true // Non-text blocks are OK
                                    }
                                } else {
                                    false
                                }
                            });
                            // If ALL text blocks are whitespace AND there are no non-text blocks, filter it
                            return !all_whitespace;
                        }
                    }
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Detects whether the conversation was interrupted mid-turn based on the last message.
///
/// An assistant as last message (after filtering unresolved tool_uses) is treated as
/// a completed turn because stop_reason is always null on persisted messages in the
/// streaming path.
fn detect_turn_interruption(messages: &[serde_json::Value]) -> TurnInterruptionState {
    if messages.is_empty() {
        return TurnInterruptionState::None;
    }

    // Find the last turn-relevant message, skipping system/progress and
    // synthetic API error assistant messages.
    let last_relevant = messages.iter().rev().find(|msg| {
        if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
            if msg_type == "system" || msg_type == "progress" {
                return false;
            }
            // Skip API error assistant messages
            if msg_type == "assistant" {
                if let Some(is_api_error) = msg.get("isApiErrorMessage").and_then(|v| v.as_bool())
                {
                    return !is_api_error;
                }
            }
            return true;
        }
        false
    });

    let Some(last_msg) = last_relevant else {
        return TurnInterruptionState::None;
    };

    let msg_type = last_msg.get("type").and_then(|v| v.as_str());

    match msg_type {
        Some("assistant") => {
            // In the streaming path, stop_reason is always null on persisted messages
            // because messages are recorded at content_block_stop time. After
            // filterUnresolvedToolUses has removed assistant messages with unmatched
            // tool_uses, an assistant as the last message means the turn most likely
            // completed normally.
            TurnInterruptionState::None
        }
        Some("user") => {
            // Check isMeta and isCompactSummary
            let is_meta = last_msg.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);
            let is_compact = last_msg
                .get("isCompactSummary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_meta || is_compact {
                return TurnInterruptionState::None;
            }

            // Check if this is a tool_result message
            if is_tool_use_result_message(last_msg) {
                // Brief mode drops the trailing assistant text block, so a completed
                // brief-mode turn legitimately ends on a tool_result.
                // Without this check, resume misclassifies every brief-mode session as interrupted.
                if is_terminal_tool_result(last_msg, messages) {
                    return TurnInterruptionState::None;
                }
                return TurnInterruptionState::None; // Interrupted mid-turn after tool result
            }

            // Plain text user prompt - CC hadn't started responding
            TurnInterruptionState::InterruptedPrompt {
                message: last_msg.clone(),
            }
        }
        Some("attachment") => {
            // Attachments are part of the user turn - the user provided context but
            // the assistant never responded.
            TurnInterruptionState::None // Treated as interrupted but without specific message
        }
        _ => TurnInterruptionState::None,
    }
}

/// Check if a user message is a tool_result message.
fn is_tool_use_result_message(msg: &serde_json::Value) -> bool {
    if let Some(msg_type) = msg.get("type").and_then(|v| v.as_str()) {
        if msg_type == "user" {
            if let Some(content) = msg.get("message").and_then(|v| v.get("content")) {
                if let Some(blocks) = content.as_array() {
                    return !blocks.is_empty()
                        && blocks.iter().all(|block| {
                            block
                                .get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "tool_result")
                                .unwrap_or(false)
                        });
                }
            }
        }
    }
    false
}

/// Check if a tool_result is from a terminal tool that completes a turn.
/// Walks back to find the assistant tool_use that this result belongs to.
fn is_terminal_tool_result(
    _result: &serde_json::Value,
    _messages: &[serde_json::Value],
) -> bool {
    // In the TS version, this checks for BRIEF_TOOL_NAME, LEGACY_BRIEF_TOOL_NAME,
    // and SEND_USER_FILE_TOOL_NAME. Since these are feature-flagged in TS,
    // we conservatively return false (no known terminal tools in the Rust build).
    false
}

/// Deserialize messages from serialized format, filtering unresolved tool uses,
/// orphaned thinking messages, and whitespace-only assistant messages.
/// Appends a synthetic assistant sentinel when the last message is from the user.
pub fn deserialize_messages(serialized_messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    deserialize_messages_with_interrupt_detection(serialized_messages).messages
}

/// Like deserialize_messages, but also detects whether the session was interrupted
/// mid-turn. Used by the SDK resume path to auto-continue interrupted turns after
/// a gateway-triggered restart.
pub fn deserialize_messages_with_interrupt_detection(
    serialized_messages: &[serde_json::Value],
) -> DeserializeResult {
    // Transform legacy attachment types before processing
    let migrated: Vec<serde_json::Value> = serialized_messages
        .iter()
        .map(migrate_legacy_attachment)
        .collect();

    // Strip invalid permissionMode values from deserialized user messages.
    let mut migrated = migrated;
    strip_invalid_permission_modes(&mut migrated);

    // Filter out unresolved tool uses
    let filtered_tool_uses = filter_unresolved_tool_uses(&migrated);

    // Filter out orphaned thinking-only assistant messages
    let filtered_thinking = filter_orphaned_thinking_messages(&filtered_tool_uses);

    // Filter out assistant messages with only whitespace text content
    let filtered_messages = filter_whitespace_only_assistant_messages(&filtered_thinking);

    // Detect turn interruption
    let internal_state = detect_turn_interruption(&filtered_messages);

    // Transform mid-turn interruptions into interrupted_prompt by appending
    // a synthetic continuation message
    let mut messages = filtered_messages;
    let turn_interruption_state = if matches!(
        internal_state,
        TurnInterruptionState::None // The TS "interrupted_turn" case maps to no specific message
    ) {
        // Check if the last message is a user message (plain text, not meta)
        if let Some(last) = messages.last() {
            if let Some(msg_type) = last.get("type").and_then(|v| v.as_str()) {
                if msg_type == "user" {
                    let is_meta = last.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);
                    let is_compact = last
                        .get("isCompactSummary")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if !is_meta && !is_compact {
                        // Append continuation message
                        let continuation = create_user_message(CONTINUE_PROMPT, true);
                        messages.push(continuation.clone());
                        return DeserializeResult {
                            messages,
                            turn_interruption_state: TurnInterruptionState::InterruptedPrompt {
                                message: continuation,
                            },
                        };
                    }
                }
            }
        }
        TurnInterruptionState::None
    } else {
        internal_state
    };

    // Append a synthetic assistant sentinel after the last user message so
    // the conversation is API-valid if no resume action is taken.
    if let Some(last) = messages.last() {
        if let Some(msg_type) = last.get("type").and_then(|v| v.as_str()) {
            if msg_type == "user" {
                let sentinel = create_assistant_message(NO_RESPONSE_REQUESTED);
                messages.push(sentinel);
            }
        }
    }

    DeserializeResult {
        messages,
        turn_interruption_state,
    }
}

/// Create a user message with the given content.
fn create_user_message(content: &str, is_meta: bool) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "message": {
            "content": content
        },
        "isMeta": is_meta
    })
}

/// Create an assistant message with the given text content.
fn create_assistant_message(content: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "assistant",
        "message": {
            "content": [{
                "type": "text",
                "text": content
            }]
        }
    })
}

/// Restore skill state from invoked_skills attachments in messages.
/// This ensures that skills are preserved across resume after compaction.
pub fn restore_skill_state_from_messages(messages: &[serde_json::Value]) {
    // In the TS version, this calls addInvokedSkill for each skill in the attachments.
    // The Rust implementation would need access to the global skill state.
    // This is a simplified placeholder that logs what would be restored.
    for message in messages {
        if let Some(msg_type) = message.get("type").and_then(|v| v.as_str()) {
            if msg_type == "attachment" {
                if let Some(attachment) = message.get("attachment") {
                    if let Some(att_type) = attachment.get("type").and_then(|v| v.as_str()) {
                        if att_type == "invoked_skills" {
                            if let Some(skills) = attachment.get("skills").and_then(|v| v.as_array())
                            {
                                for skill in skills {
                                    if let (Some(name), Some(path), Some(_content)) = (
                                        skill.get("name").and_then(|v| v.as_str()),
                                        skill.get("path").and_then(|v| v.as_str()),
                                        skill.get("content"),
                                    ) {
                                        log::debug!(
                                            "Restoring invoked skill: {} at {}",
                                            name,
                                            path
                                        );
                                    }
                                }
                            }
                        }
                        if att_type == "skill_listing" {
                            // A prior process already injected the skills-available reminder.
                            // Without this, every resume re-announces the same tokens.
                            log::debug!("Suppressing duplicate skill listing reminder");
                        }
                    }
                }
            }
        }
    }
}

/// Recover conversation from a session ID or most recent session.
/// Loads, deserializes, and processes messages for resume.
pub async fn recover_conversation(session_id: &str) -> Result<ConversationRecovery, String> {
    // Load messages from session storage
    let messages = load_conversation_state().unwrap_or_default();

    // Restore skill state from invoked_skills attachments before deserialization.
    restore_skill_state_from_messages(&messages);

    // Deserialize messages to handle unresolved tool uses and ensure proper format
    let deserialized = deserialize_messages_with_interrupt_detection(&messages);

    // Process session start hooks for resume
    let hook_messages = process_session_start_hooks("resume", Some(session_id), "default", None).await;

    // Append hook messages to the conversation
    let mut final_messages = deserialized.messages;
    for hook_msg in hook_messages {
        let json: serde_json::Value = serde_json::to_value(hook_msg).unwrap_or_default();
        final_messages.push(json);
    }

    Ok(ConversationRecovery {
        messages: final_messages,
        turn_interruption_state: deserialized.turn_interruption_state,
        session_id: if session_id.is_empty() {
            None
        } else {
            Some(session_id.to_string())
        },
    })
}

/// Save conversation state to disk.
/// Serializes messages to a JSON file for persistence.
pub fn save_conversation_state(messages: Vec<serde_json::Value>) -> Result<(), String> {
    let path = get_conversation_state_path();
    let json = serde_json::to_string_pretty(&messages)
        .map_err(|e| format!("Failed to serialize conversation state: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write conversation state: {}", e))?;
    log::debug!("Saved conversation state to {}", path.display());
    Ok(())
}

/// Load conversation state from disk.
/// Returns the most recently saved conversation state, or None if not found.
pub fn load_conversation_state() -> Option<Vec<serde_json::Value>> {
    let path = get_conversation_state_path();
    if !path.exists() {
        return None;
    }

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read conversation state: {}", e);
            return None;
        }
    };

    match serde_json::from_str(&content) {
        Ok(messages) => Some(messages),
        Err(e) => {
            log::error!("Failed to parse conversation state: {}", e);
            None
        }
    }
}

/// Get the path to the conversation state file.
fn get_conversation_state_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("ai-agent");
    path.push("conversation_state.json");
    path
}

/// Process session start hooks for resume/compaction.
/// Returns hook result messages to append to the conversation.
/// Matches TypeScript's processSessionStartHooks() in sessionStart.ts.
pub async fn process_session_start_hooks(
    source: &str,
    session_id: Option<&str>,
    model: &str,
    hook_registry: Option<&crate::hooks::HookRegistry>,
) -> Vec<crate::types::Message> {
    log::debug!(
        "Processing session start hooks: source={}, session={:?}, model={}",
        source,
        session_id,
        model
    );

    let registry = match hook_registry {
        Some(r) => r,
        None => return vec![],
    };

    if !registry.has_hooks("SessionStart") {
        return vec![];
    }

    let input = crate::hooks::HookInput {
        event: "SessionStart".to_string(),
        tool_name: None,
        tool_input: Some(serde_json::json!({
            "source": source,
            "session_id": session_id,
            "model": model,
        })),
        tool_output: None,
        tool_use_id: None,
        session_id: session_id.map(|s| s.to_string()),
        cwd: None,
        error: None,
        ..crate::hooks::HookInput::default()
    };

    let results = registry.execute("SessionStart", input).await;

    // Collect hook output as attachment messages
    results
        .iter()
        .filter_map(|r| r.message.as_ref())
        .filter(|m| !m.trim().is_empty())
        .map(|msg| crate::types::Message {
            role: crate::types::MessageRole::User,
            content: format!(
                "<session-start-hook>\n{}\n</session-start-hook>",
                msg
            ),
            is_meta: Some(true),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_empty_messages() {
        let result = deserialize_messages_with_interrupt_detection(&[]);
        assert!(result.messages.is_empty());
        assert!(matches!(result.turn_interruption_state, TurnInterruptionState::None));
    }

    #[test]
    fn test_deserialize_messages_with_assistant_last() {
        let messages = vec![
            serde_json::json!({
                "type": "user",
                "message": { "content": "Hello" }
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "content": [{ "type": "text", "text": "Hi there!" }] }
            }),
        ];

        let result = deserialize_messages_with_interrupt_detection(&messages);
        // Last message is assistant, so no interruption detected
        assert!(matches!(result.turn_interruption_state, TurnInterruptionState::None));
        // Should have original 2 messages (no sentinel appended)
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn test_deserialize_messages_with_user_last() {
        let messages = vec![serde_json::json!({
            "type": "user",
            "message": { "content": "Hello" }
        })];

        let result = deserialize_messages_with_interrupt_detection(&messages);
        // Last message is user, so interruption detected
        assert!(matches!(
            result.turn_interruption_state,
            TurnInterruptionState::InterruptedPrompt { .. }
        ));
        // Should have user + sentinel messages (no continuation for interrupted_prompt)
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn test_create_user_message() {
        let msg = create_user_message("test content", false);
        assert_eq!(msg.get("type").and_then(|v| v.as_str()), Some("user"));
    }

    #[test]
    fn test_create_assistant_message() {
        let msg = create_assistant_message("test response");
        assert_eq!(msg.get("type").and_then(|v| v.as_str()), Some("assistant"));
    }

    #[test]
    fn test_migrate_legacy_attachment_new_file() {
        let msg = serde_json::json!({
            "type": "attachment",
            "attachment": {
                "type": "new_file",
                "filename": "/path/to/file.txt"
            }
        });

        let migrated = migrate_legacy_attachment(&msg);
        let att_type = migrated.get("attachment").and_then(|v| v.get("type")).and_then(|v| v.as_str());
        assert_eq!(att_type, Some("file"));
    }

    #[test]
    fn test_filter_unresolved_tool_uses() {
        let messages = vec![
            serde_json::json!({
                "type": "assistant",
                "message": { "content": [{ "type": "tool_use", "id": "tool-1", "name": "Read" }] }
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "content": [{ "type": "text", "text": "Hello" }] }
            }),
        ];

        let filtered = filter_unresolved_tool_uses(&messages);
        // Assistant with unresolved tool_use should be filtered out
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_is_tool_use_result_message() {
        let msg = serde_json::json!({
            "type": "user",
            "message": { "content": [{ "type": "tool_result", "tool_use_id": "tool-1" }] }
        });
        assert!(is_tool_use_result_message(&msg));

        let msg2 = serde_json::json!({
            "type": "user",
            "message": { "content": "Hello" }
        });
        assert!(!is_tool_use_result_message(&msg2));
    }
}
