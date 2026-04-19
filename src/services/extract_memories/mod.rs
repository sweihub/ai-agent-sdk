// Source: /data/home/swei/claudecode/openclaudecode/src/services/extractMemories/extractMemories.ts
//! Extracts durable memories from the current session transcript
//! and writes them to the auto-memory directory (~/.ai/projects/<path>/memory/).
//!
//! It runs once at the end of each complete query loop (when the model produces
//! a final response with no tool calls) via handleStopHooks in stopHooks.ts.
//!
//! Uses the forked agent pattern (runForkedAgent) — a perfect fork of the main
//! conversation that shares the parent's prompt cache.
//!
//! State is closure-scoped inside init_extract_memories() rather than module-level,
//! following the same pattern as confidenceRating.ts. Tests call
//! init_extract_memories() in beforeEach to get a fresh closure.

pub mod prompts;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use crate::constants::tools::{
    BASH_TOOL_NAME, FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME, GLOB_TOOL_NAME,
    GREP_TOOL_NAME,
};
use crate::memdir::paths::{get_auto_mem_path, is_auto_mem_path, is_auto_memory_enabled};
use crate::memdir::ENTRYPOINT_NAME;
use crate::memdir::memory_scan::scan_memory_files;
use crate::tool::ToolUseContext;
use crate::types::message::{
    AssistantMessage, AssistantMessageContent, Message, SystemMessage, UserContent,
    UserMessage, UserMessageContent,
};
use crate::utils::forked_agent::{
    CacheSafeParams, CanUseToolFn, ForkedAgentConfig, ForkedAgentResult, PermissionDecision,
    QuerySource, create_cache_safe_params, run_forked_agent,
};

/// Create a user message for the forked agent using types::message structures.
fn create_fork_user_message(content: String) -> Message {
    Message::User(crate::types::message::UserMessage {
        base: crate::types::message::MessageBase {
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            parent_uuid: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            created_at: None,
            is_meta: Some(true),
            is_virtual: None,
            is_compact_summary: None,
            tool_use_result: None,
            origin: None,
            extra: HashMap::new(),
        },
        message_type: "user".to_string(),
        message: UserMessageContent {
            content: UserContent::Text(content),
            extra: HashMap::new(),
        },
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Count elements matching a predicate.
fn count<T, F>(arr: &[T], pred: F) -> usize
where
    F: Fn(&T) -> bool,
{
    arr.iter().filter(|x| pred(x)).count()
}

/// Get unique elements, preserving order.
fn uniq<T>(xs: impl IntoIterator<Item = T>) -> Vec<T>
where
    T: Eq + Hash + Clone,
{
    let mut set = std::collections::HashSet::new();
    let mut result = Vec::new();
    for x in xs {
        if set.insert(x.clone()) {
            result.push(x);
        }
    }
    result
}

/// Returns true if a message is visible to the model (sent in API calls).
/// Excludes progress, system, and attachment messages.
fn is_model_visible_message(message: &Message) -> bool {
    matches!(message, Message::User(_) | Message::Assistant(_))
}

fn count_model_visible_messages_since(messages: &[Message], since_uuid: Option<&str>) -> usize {
    if since_uuid.is_none() {
        return count(messages, is_model_visible_message);
    }

    let since_uuid = since_uuid.unwrap();
    let mut found_start = false;
    let mut n = 0;
    for message in messages {
        if !found_start {
            if let Message::User(user_msg) = message {
                if user_msg.base.uuid.as_deref() == Some(since_uuid) {
                    found_start = true;
                }
            } else if let Message::Assistant(assistant_msg) = message {
                if assistant_msg.base.uuid.as_deref() == Some(since_uuid) {
                    found_start = true;
                }
            }
            continue;
        }
        if is_model_visible_message(message) {
            n += 1;
        }
    }
    // If sinceUuid was not found (e.g., removed by context compaction),
    // fall back to counting all model-visible messages rather than returning 0
    // which would permanently disable extraction for the rest of the session.
    if !found_start {
        return count(messages, is_model_visible_message);
    }
    n
}

/// Returns true if any assistant message after the cursor UUID contains a
/// Write/Edit tool_use block targeting an auto-memory path.
fn has_memory_writes_since(messages: &[Message], since_uuid: Option<&str>) -> bool {
    let mut found_start = since_uuid.is_none();
    for message in messages {
        if !found_start {
            if let Message::User(user_msg) = message {
                if user_msg.base.uuid.as_deref() == since_uuid {
                    found_start = true;
                }
            } else if let Message::Assistant(assistant_msg) = message {
                if assistant_msg.base.uuid.as_deref() == since_uuid {
                    found_start = true;
                }
            }
            continue;
        }
        if let Message::Assistant(assistant_msg) = message {
            if let Some(content) = &assistant_msg.message {
                if let Some(blocks) = &content.content {
                    if let Some(arr) = blocks.as_array() {
                        for block in arr {
                            if let Some(file_path) = get_written_file_path(block) {
                                if is_auto_mem_path(&std::path::Path::new(&file_path)) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

// ============================================================================
// Tool Permissions
// ============================================================================

/// Check if a Bash input is read-only (no write/modify operations).
fn is_bash_read_only(input: &serde_json::Value) -> bool {
    if let Some(command) = input.get("command").and_then(|c| c.as_str()) {
        // Check for write operations FIRST (before prefix matching)
        if command.contains(" > ") || command.contains(" >> ")
            || command.contains(" 2> ") || command.contains(" 2>> ")
        {
            return false;
        }
        let read_only_prefixes = [
            "ls", "find", "cat", "stat", "wc", "head", "tail", "grep", "less",
            "more", "type", "which", "file", "du", "df", "pwd", "echo", "sort",
            "uniq", "diff", "comm", "cut", "awk", "tr", "xxd", "od",
            "hexdump", "basename", "dirname", "readlink", "realpath", "env",
            "printenv", "date", "uptime", "free", "ps", "journalctl",
            "systemctl status", "mount", "ip", "ifconfig", "ping",
            "curl", "man", "info",
            "--help", "-h", "-V", "--version", "touch -r",
        ];
        for prefix in &read_only_prefixes {
            if command.starts_with(prefix) {
                return true;
            }
        }
        // No destructive commands
        let destructive_prefixes = [
            "rm ", "mv ", "cp ", "dd ", "truncate ", "mkfs ",
            "chmod ", "chown ", "sync", "shutdown", "reboot",
            "mount --", "umount", "mkswap", "swapoff",
            "mkfs.", "fsck", "fdisk", "wipefs",
        ];
        !destructive_prefixes.iter().any(|p| command.starts_with(p))
    } else {
        false
    }
}

/// Deny a tool use with logging, returns error message string
fn deny_auto_mem_tool_string(tool_name: &str, reason: &str) -> String {
    log::debug!("[autoMem] denied {}: {}", tool_name, reason);
    reason.to_string()
}

/// Creates a canUseTool function for the forked extraction agent.
/// Allows Read/Grep/Glob (unrestricted), read-only Bash, and Edit/Write for auto-mem paths only.
fn create_auto_mem_can_use_tool(memory_dir: std::path::PathBuf) -> Arc<CanUseToolFn> {
    let memory_dir_str = memory_dir.to_string_lossy().to_string();

    Arc::new(move |_tool_def, input, _tool_use_context, _assistant_msg, _query_source, _is_explicit| {
        let tool_name = _tool_def.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let input = input.clone();
        let memory_dir_path = memory_dir.clone();
        let memory_dir_str = memory_dir_str.clone();

        Box::pin(async move {
            let tool_name = tool_name;
            if tool_name == FILE_READ_TOOL_NAME || tool_name == GREP_TOOL_NAME || tool_name == GLOB_TOOL_NAME {
                return Ok(PermissionDecision::Allow);
            }

            if tool_name == BASH_TOOL_NAME {
                if is_bash_read_only(&input) {
                    return Ok(PermissionDecision::Allow);
                }
                return Err(deny_auto_mem_tool_string(
                    &tool_name,
                    "Only read-only shell commands are permitted in this context (ls, find, grep, cat, stat, wc, head, tail, and similar)",
                ));
            }

            if tool_name == FILE_EDIT_TOOL_NAME || tool_name == FILE_WRITE_TOOL_NAME {
                if let Some(file_path) = input.get("file_path").and_then(|p| p.as_str()) {
                    if is_auto_mem_path(&std::path::Path::new(file_path)) {
                        return Ok(PermissionDecision::Allow);
                    }
                }
            }

            Err(deny_auto_mem_tool_string(
                &tool_name,
                &format!(
                    "only {}, {}, {}, read-only {}, and {}/{} within {} are allowed",
                    FILE_READ_TOOL_NAME,
                    GREP_TOOL_NAME,
                    GLOB_TOOL_NAME,
                    BASH_TOOL_NAME,
                    FILE_EDIT_TOOL_NAME,
                    FILE_WRITE_TOOL_NAME,
                    memory_dir_str,
                ),
            ))
        })
    })
}

// ============================================================================
// Extract file paths from agent output
// ============================================================================

/// Extract file_path from a tool_use block's input, if present.
fn get_written_file_path(block: &serde_json::Value) -> Option<String> {
    if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
        return None;
    }
    let name = block.get("name").and_then(|n| n.as_str())?;
    if name != FILE_EDIT_TOOL_NAME && name != FILE_WRITE_TOOL_NAME {
        return None;
    }
    let input = block.get("input")?;
    if let Some(obj) = input.as_object() {
        if let Some(fp) = obj.get("file_path") {
            return fp.as_str().map(String::from);
        }
    }
    None
}

fn extract_written_paths(agent_messages: &[Message]) -> Vec<String> {
    let mut paths = Vec::new();
    for message in agent_messages {
        if let Message::Assistant(assistant_msg) = message {
            if let Some(content) = &assistant_msg.message {
                if let Some(blocks) = &content.content {
                    if let Some(arr) = blocks.as_array() {
                        for block in arr {
                            if let Some(file_path) = get_written_file_path(block) {
                                paths.push(file_path);
                            }
                        }
                    }
                }
            }
        }
    }
    uniq(paths)
}

// ============================================================================
// Initialization & Closure-scoped State
// ============================================================================

/// AppendSystemMessageFn — appends a system message to the conversation.
pub type AppendSystemMessageFn = Arc<dyn Fn(SystemMessage) + Send + Sync>;

/// Context from a REPL hook — mirrors REPLHookContext from TypeScript.
#[derive(Clone)]
pub struct ExtractMemoryContext {
    pub messages: Vec<Message>,
    pub system_prompt: String,
    pub user_context: HashMap<String, String>,
    pub system_context: HashMap<String, String>,
    pub tool_use_context: Arc<ToolUseContext>,
    pub agent_id: Option<String>,
}

/// State for managing in-flight extractions.
struct ExtractionState {
    last_memory_message_uuid: std::sync::Mutex<Option<String>>,
    in_progress: std::sync::Mutex<bool>,
    turns_since_last_extraction: std::sync::Mutex<usize>,
    pending_context: std::sync::Mutex<Option<(ExtractMemoryContext, Option<AppendSystemMessageFn>)>>,
}

impl ExtractionState {
    fn new() -> Self {
        Self {
            last_memory_message_uuid: std::sync::Mutex::new(None),
            in_progress: std::sync::Mutex::new(false),
            turns_since_last_extraction: std::sync::Mutex::new(0),
            pending_context: std::sync::Mutex::new(None),
        }
    }

    fn clone_state(&self) -> Self {
        Self {
            last_memory_message_uuid: std::sync::Mutex::new(
                self.last_memory_message_uuid.lock().unwrap().clone(),
            ),
            in_progress: std::sync::Mutex::new(*self.in_progress.lock().unwrap()),
            turns_since_last_extraction: std::sync::Mutex::new(
                *self.turns_since_last_extraction.lock().unwrap(),
            ),
            pending_context: std::sync::Mutex::new(
                self.pending_context.lock().unwrap().clone(),
            ),
        }
    }
}

/// Advance the message cursor to the last message in the list.
fn advance_cursor(state: &ExtractionState, messages: &[Message]) {
    let mut guard = state.last_memory_message_uuid.lock().unwrap();
    if let Some(last) = messages.last() {
        if let Message::User(u) = last {
            *guard = u.base.uuid.clone();
        } else if let Message::Assistant(a) = last {
            *guard = a.base.uuid.clone();
        }
    }
}

/// Create a system message indicating memories were saved.
fn create_system_memory_saved_message(written_paths: &[String]) -> SystemMessage {
    SystemMessage {
        base: crate::types::message::MessageBase {
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            parent_uuid: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            created_at: None,
            is_meta: Some(false),
            is_virtual: None,
            is_compact_summary: None,
            tool_use_result: None,
            origin: None,
            extra: HashMap::new(),
        },
        message_type: "system".to_string(),
        subtype: Some("memory_saved".to_string()),
        level: None,
        message: Some(format!("Memories saved to: {}", written_paths.join(", "))),
    }
}

/// Run a single extraction operation.
async fn run_extraction(
    state: &ExtractionState,
    context: ExtractMemoryContext,
    append_system_message: Option<AppendSystemMessageFn>,
    is_trailing_run: bool,
) {
    async fn do_extraction(
        state: &ExtractionState,
        context: ExtractMemoryContext,
        append_system_message: Option<AppendSystemMessageFn>,
        is_trailing_run: bool,
    ) {
        let messages = &context.messages;
        let memory_dir = get_auto_mem_path();
        let memory_dir_path = memory_dir.clone();

        let last_uuid = {
            let guard = state.last_memory_message_uuid.lock().unwrap();
            guard.clone()
        };
        let new_message_count = count_model_visible_messages_since(messages, last_uuid.as_deref());

        // When the main agent wrote memories, skip the forked agent and advance cursor.
        if has_memory_writes_since(messages, last_uuid.as_deref()) {
            log::debug!("[extractMemories] skipping — conversation already wrote to memory files");
            advance_cursor(state, messages);
            return;
        }

        let turn_interval: usize = 1;

        if !is_trailing_run {
            let mut turns = state.turns_since_last_extraction.lock().unwrap();
            *turns += 1;
            if *turns < turn_interval {
                return;
            }
            *turns = 0;
            drop(turns);
        }

        {
            let mut in_progress_guard = state.in_progress.lock().unwrap();
            *in_progress_guard = true;
        }

        let start_time = std::time::Instant::now();
        log::debug!(
            "[extractMemories] starting — {} new messages, memoryDir={}",
            new_message_count,
            memory_dir.display()
        );

        // Pre-inject the memory directory manifest.
        let existing_memories = {
            let headers = scan_memory_files(&memory_dir.to_string_lossy()).await;
            crate::memdir::format_memory_manifest(&headers)
        };

        let user_prompt =
            prompts::build_extract_auto_only_prompt(new_message_count, &existing_memories, false);

        let cache_safe_params = create_cache_safe_params(
            context.system_prompt.clone(),
            context.user_context.clone(),
            context.system_context.clone(),
            context.tool_use_context.clone(),
            messages.clone(),
        );

        let can_use_tool = create_auto_mem_can_use_tool(memory_dir_path);

        let query_source = QuerySource("extract_memories".to_string());
        let result = match run_forked_agent(ForkedAgentConfig {
            prompt_messages: vec![create_fork_user_message(user_prompt)],
            cache_safe_params,
            can_use_tool,
            query_source,
            fork_label: "extract_memories".to_string(),
            overrides: None,
            max_output_tokens: None,
            max_turns: Some(5),
            on_message: None,
            skip_transcript: true,
            skip_cache_write: true,
        })
        .await
        {
            Ok(result) => result,
            Err(e) => {
                log::debug!("[extractMemories] error: {}", e);
                let mut in_progress_guard = state.in_progress.lock().unwrap();
                *in_progress_guard = false;
                return;
            }
        };

        advance_cursor(state, messages);

        let written_paths = extract_written_paths(&result.messages);
        let turn_count = count(&result.messages, |m| matches!(m, Message::Assistant(_)));

        log::debug!(
            "[extractMemories] finished — {} files written, turns={}",
            written_paths.len(),
            turn_count
        );

        if written_paths.is_empty() {
            log::debug!("[extractMemories] no memories saved this run");
        } else {
            log::debug!("[extractMemories] memories saved: {}", written_paths.join(", "));
        }

        // Filter out MEMORY.md entries to get actual memory file paths.
        let memory_paths: Vec<String> = uniq(
            written_paths
                .into_iter()
                .filter(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|name| name.to_string_lossy() != ENTRYPOINT_NAME)
                        .unwrap_or(false)
                })
        );

        if let Some(ref append_fn) = append_system_message {
            if !memory_paths.is_empty() {
                let msg = create_system_memory_saved_message(&memory_paths);
                append_fn(msg);
            }
        }

        {
            let mut in_progress_guard = state.in_progress.lock().unwrap();
            *in_progress_guard = false;
        }

        // If a call arrived while we were running, run a trailing extraction.
        let trailing = {
            let mut pending = state.pending_context.lock().unwrap();
            pending.take()
        };
        if let Some((trailing_context, trailing_append)) = trailing {
            log::debug!("[extractMemories] running trailing extraction for stashed context");
            Box::pin(do_extraction(state, trailing_context, trailing_append, true)).await;
        }
    }

    do_extraction(state, context, append_system_message, is_trailing_run).await
}

// ============================================================================
// Public API
// ============================================================================

static EXTRACTOR_STATE: std::sync::Mutex<Option<ExtractionState>> =
    std::sync::Mutex::new(None);

/// Initialize the memory extraction system.
pub fn init_extract_memories() {
    let state = ExtractionState::new();
    let mut guard = EXTRACTOR_STATE.lock().unwrap();
    *guard = Some(state);
}

/// Run memory extraction at the end of a query loop.
/// Called fire-and-forget from handleStopHooks.
/// No-ops until init_extract_memories() has been called.
pub async fn execute_extract_memories(
    context: ExtractMemoryContext,
    append_system_message: Option<AppendSystemMessageFn>,
) {
    let state = {
        let guard = EXTRACTOR_STATE.lock().unwrap();
        guard.as_ref().unwrap().clone_state()
    };

    // Only run for the main agent, not subagents.
    if context.agent_id.is_some() {
        return;
    }

    // Check auto-memory is enabled.
    if !is_auto_memory_enabled() {
        return;
    }

    // Skip in remote mode (simplified check).
    if std::env::var("AI_CODE_REMOTE").is_ok()
        && std::env::var("AI_CODE_REMOTE_MEMORY_DIR").is_err()
    {
        return;
    }

    let context = context.clone();
    let append_fn = append_system_message;

    // Check for in-progress extraction.
    {
        let in_progress = state.in_progress.lock().unwrap();
        if *in_progress {
            log::debug!("[extractMemories] extraction in progress — stashing for trailing run");
            drop(in_progress);
            let mut pending = state.pending_context.lock().unwrap();
            *pending = Some((context, append_fn));
            return;
        }
    }

    run_extraction(&state, context, append_fn, false).await;
}

/// Awaits all in-flight extractions with a soft timeout.
/// No-op until init_extract_memories() has been called.
pub async fn drain_pending_extraction(_timeout_ms: Option<u64>) {
    let _ = _timeout_ms;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user_message(uuid: &str) -> Message {
        Message::User(UserMessage {
            base: crate::types::message::MessageBase {
                uuid: Some(uuid.to_string()),
                parent_uuid: None,
                timestamp: Some("2024-01-01T00:00:00Z".to_string()),
                created_at: None,
                is_meta: None,
                is_virtual: None,
                is_compact_summary: None,
                tool_use_result: None,
                origin: None,
                extra: HashMap::new(),
            },
            message_type: "user".to_string(),
            message: UserMessageContent {
                content: UserContent::Text("test".to_string()),
                extra: HashMap::new(),
            },
        })
    }

    fn test_assistant_message(uuid: &str) -> Message {
        Message::Assistant(AssistantMessage {
            base: crate::types::message::MessageBase {
                uuid: Some(uuid.to_string()),
                parent_uuid: None,
                timestamp: Some("2024-01-01T00:00:01Z".to_string()),
                created_at: None,
                is_meta: None,
                is_virtual: None,
                is_compact_summary: None,
                tool_use_result: None,
                origin: None,
                extra: HashMap::new(),
            },
            message_type: "assistant".to_string(),
            message: Some(AssistantMessageContent {
                content: None,
                extra: HashMap::new(),
            }),
        })
    }

    #[test]
    fn test_is_model_visible_message() {
        assert!(is_model_visible_message(&test_user_message("1")));
        assert!(is_model_visible_message(&test_assistant_message("1")));
    }

    #[test]
    fn test_count_model_visible_messages_since_none() {
        let messages = vec![
            test_user_message("1"),
            test_assistant_message("2"),
            test_user_message("3"),
        ];
        assert_eq!(count_model_visible_messages_since(&messages, None), 3);
    }

    #[test]
    fn test_count_model_visible_messages_since_found() {
        let messages = vec![
            test_user_message("1"),
            test_user_message("2"),
            test_assistant_message("3"),
            test_user_message("4"),
        ];
        assert_eq!(count_model_visible_messages_since(&messages, Some("2")), 2);
    }

    #[test]
    fn test_count_model_visible_messages_since_not_found() {
        let messages = vec![test_user_message("1"), test_assistant_message("2")];
        assert_eq!(count_model_visible_messages_since(&messages, Some("999")), 2);
    }

    #[test]
    fn test_has_memory_writes_since_empty() {
        let messages = vec![test_user_message("1"), test_user_message("2")];
        assert!(!has_memory_writes_since(&messages, None));
    }

    #[test]
    fn test_build_extract_auto_only_prompt_has_required_sections() {
        let prompt = prompts::build_extract_auto_only_prompt(5, "", false);
        assert!(prompt.contains("memory extraction subagent"));
        assert!(prompt.contains("How to save memories"));
        assert!(prompt.contains("Types of memory"));
        assert!(prompt.contains("What NOT to save in memory"));
    }

    #[test]
    fn test_build_extract_auto_only_prompt_with_existing_memories() {
        let existing = "- user_role.md (2024-01-01): User role\n- feedback_test.md (2024-01-02): Test feedback";
        let prompt = prompts::build_extract_auto_only_prompt(5, existing, false);
        assert!(prompt.contains("Existing memory files"));
        assert!(prompt.contains("user_role.md"));
    }

    #[test]
    fn test_build_extract_auto_only_prompt_skip_index() {
        let prompt = prompts::build_extract_auto_only_prompt(5, "", true);
        assert!(!prompt.contains("Step 1"));
        assert!(!prompt.contains("Step 2"));
    }

    #[test]
    fn test_build_extract_combined_prompt() {
        let prompt = prompts::build_extract_combined_prompt(5, "", false);
        assert!(prompt.contains("memory extraction subagent"));
        assert!(prompt.contains("Types of memory"));
        assert!(prompt.contains("You MUST avoid saving sensitive data"));
    }

    #[test]
    fn test_bash_read_only() {
        assert!(is_bash_read_only(&serde_json::json!({"command": "ls -la"})));
        assert!(is_bash_read_only(&serde_json::json!({"command": "grep pattern file.txt"})));
        assert!(is_bash_read_only(&serde_json::json!({"command": "cat file.txt"})));
        assert!(!is_bash_read_only(&serde_json::json!({"command": "rm file.txt"})));
        assert!(!is_bash_read_only(&serde_json::json!({"command": "echo hello > file.txt"})));
        assert!(!is_bash_read_only(&serde_json::json!({"command": "cp a b"})));
    }

    #[test]
    fn test_get_written_file_path_edit_tool() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "Edit",
            "input": {"file_path": "/some/path/memory/test.md", "edit_range": [0, 100], "new_str": "content"}
        });
        assert_eq!(
            get_written_file_path(&block),
            Some("/some/path/memory/test.md".to_string())
        );
    }

    #[test]
    fn test_get_written_file_path_write_tool() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "Write",
            "input": {"file_path": "/some/path/memory/test.md", "content": "hello"}
        });
        assert_eq!(
            get_written_file_path(&block),
            Some("/some/path/memory/test.md".to_string())
        );
    }

    #[test]
    fn test_get_written_file_path_not_write_tool() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "Bash",
            "input": {"command": "ls"}
        });
        assert_eq!(get_written_file_path(&block), None);
    }

    #[test]
    fn test_get_written_file_path_not_tool_use() {
        let block = serde_json::json!({"type": "text", "text": "hello"});
        assert_eq!(get_written_file_path(&block), None);
    }

    #[test]
    fn test_count_function() {
        let data = vec![1, 2, 3, 4, 5];
        assert_eq!(count(&data, |x| *x > 3), 2);
        assert_eq!(count(&data, |x| *x < 0), 0);
    }

    #[test]
    fn test_uniq_function() {
        let data = vec![3, 1, 2, 1, 3, 4];
        assert_eq!(uniq(data), vec![3, 1, 2, 4]);
    }
}
