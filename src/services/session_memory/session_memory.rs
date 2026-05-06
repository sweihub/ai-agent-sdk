// Source: ~/claudecode/openclaudecode/src/services/SessionMemory/sessionMemory.ts
//! Session memory extraction via post-sampling hook.
//!
//! Automatically maintains a markdown file with notes about the current conversation.
//! Runs periodically in the background using a forked subagent to extract key information
//! without interrupting the main conversation flow.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::constants::tools::FILE_EDIT_TOOL_NAME;
use crate::services::compact::auto_compact::is_auto_compact_enabled;
use crate::types::Message;
use crate::utils::file_state_cache::{
    DEFAULT_MAX_CACHE_SIZE_BYTES, FileStateCache, READ_FILE_STATE_CACHE_SIZE,
};
use crate::utils::forked_agent::{
    CanUseToolFn as ForkCanUseToolFn, PermissionDecision, QuerySource,
    create_cache_safe_params, run_forked_agent, SubagentContextOverrides,
};
use crate::utils::hooks::post_sampling_hooks::{
    PostSamplingHook, ReplHookContext, register_post_sampling_hook,
};

// Re-export utility functions for external callers
use super::session_memory_utils::*;
use super::prompts::*;

/// Module-level state for tracking the last memory extraction boundary.
static LAST_MEMORY_MESSAGE_UUID: Mutex<Option<String>> = Mutex::new(None);

/// Reset the last memory message UUID (for testing)
pub fn reset_last_memory_message_uuid() {
    *LAST_MEMORY_MESSAGE_UUID.lock().unwrap() = None;
}

// ============================================================================
// Token estimation
// ============================================================================

/// Estimate total token count from API [Message] array.
///
/// Uses a rough character-based estimation (4 chars per token).
/// This works with `crate::types::Message` (the API message struct).
fn estimate_message_token_count(messages: &[Message]) -> u64 {
    messages.iter().map(rough_token_estimate).sum()
}

/// Rough token estimate for a single API [Message].
fn rough_token_estimate(msg: &Message) -> u64 {
    let content_len = msg.content.len() as u64;
    let tool_calls_len = msg
        .tool_calls
        .as_ref()
        .map(|tc| tc.len() as u64 * 20)
        .unwrap_or(0);
    (content_len / 4 + tool_calls_len).max(1)
}

// ============================================================================
// Tool call counting
// ============================================================================

/// Count tool calls in assistant messages since a given index.
pub fn count_tool_calls_since(messages: &[Message], since_index: Option<usize>) -> usize {
    let start = since_index.map(|i| i + 1).unwrap_or(0);
    messages[start..]
        .iter()
        .filter(|m| m.role == crate::types::MessageRole::Assistant)
        .flat_map(|m| m.tool_calls.as_ref())
        .map(|tc| tc.len())
        .sum()
}

/// Count tool calls in assistant messages after the given UUID.
/// If `since_uuid` is None, counts all assistant tool calls.
fn count_tool_calls_since_uuid(messages: &[Message], since_uuid: Option<&str>) -> usize {
    let mut tool_call_count = 0;
    let mut found_start = since_uuid.is_none();

    for message in messages {
        if !found_start {
            if message.uuid.as_deref() == since_uuid {
                found_start = true;
            }
            continue;
        }
        if message.role == crate::types::MessageRole::Assistant {
            if let Some(ref tc) = message.tool_calls {
                tool_call_count += tc.len();
            }
        }
    }
    tool_call_count
}

/// Find the index of the last assistant message.
fn find_last_assistant_index(messages: &[Message]) -> Option<usize> {
    messages
        .iter()
        .rposition(|m| m.role == crate::types::MessageRole::Assistant)
}

// ============================================================================
// shouldExtractMemory
// ============================================================================

/// Check if session memory should be extracted based on current conversation state.
pub fn should_extract_memory(messages: &[Message]) -> bool {
    let current_token_count = estimate_message_token_count(messages);

    if !is_session_memory_initialized() {
        if !has_met_initialization_threshold(current_token_count) {
            return false;
        }
        mark_session_memory_initialized();
    }

    let has_met_token_threshold = has_met_update_threshold(current_token_count);

    // Count tool calls since last extraction (matches TS: countToolCallsSince(messages, lastMemoryMessageUuid))
    let last_uuid = LAST_MEMORY_MESSAGE_UUID.lock().unwrap().clone();
    let tool_calls_since = count_tool_calls_since_uuid(messages, last_uuid.as_deref());
    let has_met_tool_call_threshold =
        tool_calls_since >= get_tool_calls_between_updates() as usize;

    // Check if last assistant turn has tool calls
    let has_tool_calls_in_last_turn = messages.iter().rev().any(|m| {
        if m.role == crate::types::MessageRole::Assistant {
            return m.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);
        }
        false
    });

    // Trigger extraction when:
    // 1. Both thresholds are met (tokens AND tool calls), OR
    // 2. No tool calls in last turn AND token threshold is met
    (has_met_token_threshold && has_met_tool_call_threshold)
        || (has_met_token_threshold && !has_tool_calls_in_last_turn)
}

// ============================================================================
// setupSessionMemoryFile
// ============================================================================

/// Set up the session memory file (create if needed) and read current contents.
async fn setup_session_memory_file() -> Result<(String, String), String> {
    let memory_dir =
        crate::utils::permissions::filesystem::get_session_memory_dir();
    let memory_path =
        crate::utils::permissions::filesystem::get_session_memory_path();

    // Create directory
    std::fs::create_dir_all(&memory_dir)
        .map_err(|e| format!("Failed to create session memory directory: {e}"))?;

    // Create file if it doesn't exist
    let file_path = std::path::Path::new(&memory_path);
    if !file_path.exists() {
        let template = load_session_memory_template();
        std::fs::write(file_path, template.as_bytes())
            .map_err(|e| format!("Failed to create session memory file: {e}"))?;
    }

    // Read current contents
    let current_memory =
        std::fs::read_to_string(file_path).unwrap_or_default();

    log::debug!(
        "Session memory file read: {} characters",
        current_memory.len()
    );

    Ok((memory_path, current_memory))
}

// ============================================================================
// createMemoryFileCanUseTool
// ============================================================================

/// Creates a can_use_tool function that only allows Edit for the exact memory file.
pub fn create_memory_file_can_use_tool(
    memory_path: String,
) -> Arc<ForkCanUseToolFn> {
    Arc::new(move |tool_def: &serde_json::Value,
              input: &serde_json::Value,
              _context: Arc<crate::tool::ToolUseContext>,
              _assistant: Arc<crate::types::message::AssistantMessage>,
              _query_source: &str,
              _is_explicit: bool| {
            let memory_path = memory_path.clone();
            let tool_name = tool_def
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let is_edit = tool_name == FILE_EDIT_TOOL_NAME;
            let input_file_path = input
                .get("file_path")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string());
            Box::pin(async move {
                if is_edit {
                    if let Some(ref fp) = input_file_path {
                        if fp == memory_path.as_str() {
                            return Ok(PermissionDecision::Allow);
                        }
                    }
                }

                Ok(PermissionDecision::Deny {
                    reason: Some(format!(
                        "only {FILE_EDIT_TOOL_NAME} on {memory_path} is allowed"
                    )),
                })
            })
        },
    )
}

// ============================================================================
// updateLastSummarizedMessageIdIfSafe
// ============================================================================

fn update_last_summarized_message_id_if_safe(messages: &[Message]) {
    let has_tool_calls = messages.iter().rev().any(|m| {
        if m.role == crate::types::MessageRole::Assistant {
            return m.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);
        }
        false
    });

    // Only set last summarized message ID if last assistant turn has no tool calls
    // (avoids orphaned tool_results)
    if !has_tool_calls {
        if let Some(last_msg) = messages.last() {
            if let Some(ref uuid) = last_msg.uuid {
                set_last_summarized_message_id(Some(uuid.as_str()));
            }
        }
    }
}

// ============================================================================
// Core extraction
// ============================================================================

async fn do_session_memory_extraction(
    context: &ReplHookContext,
) -> Result<(), String> {
    mark_extraction_started();

    // Set up file system and read current state
    let (memory_path, current_memory) = setup_session_memory_file().await?;

    // Create extraction message
    let user_prompt = build_session_memory_update_prompt(
        &current_memory,
        &memory_path,
    );

    // Store memory path for future reference
    set_session_memory_path(memory_path.clone());

    // Create a ToolUseContext for cache-safe params.
    // The hook context only carries a lightweight ToolUseContext,
    // so we use a stub. The forked agent is currently a stub itself,
    // so passing empty messages is sufficient.
    let tool_use_context = crate::tool::ToolUseContext::stub();

    let cache_safe_params = create_cache_safe_params(
        context.system_prompt.join("\n"),
        context.user_context.clone(),
        context.system_context.clone(),
        Arc::new(tool_use_context),
        Vec::<crate::types::message::Message>::new(),
    );

    // Run session memory extraction using runForkedAgent
    let prompt_user_msg = crate::types::message::UserMessage {
        base: crate::types::message::MessageBase {
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            ..Default::default()
        },
        message_type: "user".to_string(),
        message: crate::types::message::UserMessageContent {
            content: crate::types::message::UserContent::Text(user_prompt),
            extra: std::collections::HashMap::new(),
        },
    };
    let prompt_messages =
        vec![crate::types::message::Message::User(prompt_user_msg)];

    // Create fresh file state cache for the subagent
    let fresh_cache = Arc::new(FileStateCache::new(
        READ_FILE_STATE_CACHE_SIZE,
        DEFAULT_MAX_CACHE_SIZE_BYTES,
    ));

    run_forked_agent(crate::utils::forked_agent::ForkedAgentConfig {
        prompt_messages,
        cache_safe_params,
        can_use_tool: create_memory_file_can_use_tool(memory_path.clone()),
        query_source: QuerySource("session_memory".to_string()),
        fork_label: "session_memory".to_string(),
        overrides: Some(SubagentContextOverrides {
            read_file_state: Some(fresh_cache),
            ..SubagentContextOverrides::default()
        }),
        max_output_tokens: None,
        max_turns: None,
        on_message: None,
        skip_transcript: true,
        skip_cache_write: true,
    })
    .await?;

    // Record the context size at extraction
    let token_count = estimate_message_token_count(&context.messages);
    record_extraction_token_count(token_count);

    update_last_summarized_message_id_if_safe(&context.messages);

    mark_extraction_completed();

    Ok(())
}

// ============================================================================
// Post-sampling hook (sequential wrapper)
// ============================================================================

/// Mutex to prevent concurrent session memory extractions.
static EXTRACTION_LOCK: Mutex<bool> = Mutex::new(false);

fn create_extraction_hook() -> PostSamplingHook {
    fn extract_session_memory_hook(
        ctx: ReplHookContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            // Only run on main REPL thread
            if ctx.query_source.as_deref() != Some("repl_main_thread") {
                return;
            }

            // Prevent concurrent extractions - acquire lock
            let locked = {
                let mut lock = match EXTRACTION_LOCK.lock() {
                    Ok(l) => l,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if *lock {
                    return;
                }
                *lock = true;
                true
            };

            if !locked {
                return;
            }

            let result = do_session_memory_extraction(&ctx).await;

            // Release lock
            if let Ok(mut l) = EXTRACTION_LOCK.lock() {
                *l = false;
            }

            if let Err(e) = result {
                log::warn!("Session memory extraction failed: {e}");
            }
        })
    }

    Arc::new(extract_session_memory_hook)
}

// ============================================================================
// initSessionMemory
// ============================================================================

/// Initialize session memory by registering the post-sampling hook.
pub fn init_session_memory() {
    if !is_auto_compact_enabled() {
        return;
    }

    register_post_sampling_hook(create_extraction_hook());
}

// ============================================================================
// get_session_memory
// ============================================================================

/// Read the current session memory notes from disk (sync).
pub fn get_session_memory() -> Option<String> {
    let memory_path = get_session_memory_path()?;
    let content = std::fs::read_to_string(&memory_path).ok()?;
    if content.is_empty() {
        return None;
    }
    Some(content)
}

/// Async version for callers that need an async Result.
pub async fn get_session_memory_content() -> Result<Option<String>, crate::AgentError> {
    match get_session_memory() {
        Some(content) => Ok(Some(content)),
        None => Ok(None),
    }
}

/// Initialize session memory file with template on disk.
pub async fn init_session_memory_file() -> Result<String, crate::AgentError> {
    let memory_dir =
        crate::utils::permissions::filesystem::get_session_memory_dir();
    let memory_path =
        crate::utils::permissions::filesystem::get_session_memory_path();

    tokio::fs::create_dir_all(&memory_dir)
        .await
        .map_err(crate::AgentError::Io)?;

    let file_path = std::path::Path::new(&memory_path);
    if !file_path.exists() {
        let template = load_session_memory_template();
        tokio::fs::write(&file_path, template)
            .await
            .map_err(crate::AgentError::Io)?;
    }

    set_session_memory_path(memory_path.clone());

    tokio::fs::read_to_string(&file_path)
        .await
        .map_err(crate::AgentError::Io)
}

// ============================================================================
// manuallyExtractSessionMemory
// ============================================================================

/// Result of a manual session memory extraction.
#[derive(Debug, Clone)]
pub struct ManualExtractionResult {
    pub success: bool,
    pub memory_path: Option<String>,
    pub error: Option<String>,
}

/// Manually trigger session memory extraction, bypassing threshold checks.
pub async fn manually_extract_session_memory(
    messages: &[Message],
    _tool_use_context: Arc<crate::tool::ToolUseContext>,
) -> ManualExtractionResult {
    if messages.is_empty() {
        return ManualExtractionResult {
            success: false,
            memory_path: None,
            error: Some("No messages to summarize".to_string()),
        };
    }

    mark_extraction_started();

    let context = ReplHookContext {
        messages: messages.to_vec(),
        system_prompt: vec![],
        user_context: HashMap::new(),
        system_context: HashMap::new(),
        tool_use_context: Arc::new(
            crate::utils::hooks::can_use_tool::ToolUseContext {
                session_id: "manual_extraction".to_string(),
                cwd: None,
                is_non_interactive_session: false,
                options: None,
            },
        ),
        query_source: Some("manual_extraction".to_string()),
        query_message_count: Some(messages.len()),
    };

    match do_session_memory_extraction(&context).await {
        Ok(()) => ManualExtractionResult {
            success: true,
            memory_path: get_session_memory_path(),
            error: None,
        },
        Err(e) => {
            mark_extraction_completed();
            ManualExtractionResult {
                success: false,
                memory_path: None,
                error: Some(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_assistant_message(
        tool_calls: Option<Vec<crate::types::ToolCall>>,
    ) -> Message {
        Message {
            role: crate::types::MessageRole::Assistant,
            content: "thinking...".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            timestamp: None,
        }
    }

    fn make_user_message(text: &str) -> Message {
        Message {
            role: crate::types::MessageRole::User,
            content: text.to_string(),
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
    fn test_rough_token_estimate() {
        let msg = make_user_message("hello world");
        let est = rough_token_estimate(&msg);
        assert!(est > 0);
    }

    #[test]
    fn test_token_count_no_tool_calls() {
        let messages = vec![
            make_user_message("hello"),
            make_assistant_message(None),
        ];
        let count = estimate_message_token_count(&messages);
        assert!(count > 0);
    }

    #[test]
    fn test_token_count_with_tool_calls() {
        let tool_call = crate::types::ToolCall {
            id: "tc_1".to_string(),
            r#type: "function".to_string(),
            name: "Edit".to_string(),
            arguments: serde_json::json!({"file_path": "/tmp/test.rs"}),
        };
        let messages = vec![
            make_user_message("hello"),
            make_assistant_message(Some(vec![tool_call])),
        ];
        let count = estimate_message_token_count(&messages);
        assert!(count > 0);
    }

    #[test]
    fn test_count_tool_calls_since() {
        let tool_call = crate::types::ToolCall {
            id: "tc_1".to_string(),
            r#type: "function".to_string(),
            name: "Edit".to_string(),
            arguments: serde_json::json!({}),
        };
        let messages = vec![
            make_user_message("hello"),
            make_assistant_message(Some(vec![tool_call.clone(), tool_call.clone()])),
            make_user_message("result"),
            make_assistant_message(Some(vec![tool_call])),
        ];
        assert_eq!(count_tool_calls_since(&messages, None), 3);
        assert_eq!(count_tool_calls_since(&messages, Some(2)), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_should_extract_memory_initial_threshold() {
        super::reset_session_memory_state();
        reset_last_memory_message_uuid();

        let small_messages = vec![make_user_message("hi")];
        assert!(!should_extract_memory(&small_messages));
    }

    #[test]
    fn test_has_tool_calls_in_last_turn() {
        let tool_call = crate::types::ToolCall {
            id: "tc_1".to_string(),
            r#type: "function".to_string(),
            name: "Edit".to_string(),
            arguments: serde_json::json!({}),
        };

        let messages = vec![
            make_user_message("hello"),
            make_assistant_message(Some(vec![tool_call])),
        ];
        let has_calls = messages.iter().rev().any(|m| {
            if m.role == crate::types::MessageRole::Assistant {
                return m.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);
            }
            false
        });
        assert!(has_calls);

        let messages = vec![
            make_user_message("hello"),
            make_assistant_message(None),
        ];
        let has_calls = messages.iter().rev().any(|m| {
            if m.role == crate::types::MessageRole::Assistant {
                return m.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);
            }
            false
        });
        assert!(!has_calls);
    }

    #[test]
    #[serial_test::serial]
    fn test_reset_last_memory_message_uuid() {
        *LAST_MEMORY_MESSAGE_UUID.lock().unwrap() =
            Some("test-uuid".to_string());
        reset_last_memory_message_uuid();
        assert!(LAST_MEMORY_MESSAGE_UUID.lock().unwrap().is_none());
    }
}
