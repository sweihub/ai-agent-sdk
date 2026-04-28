//! Extract Memories - ported from ~/claudecode/openclaudecode/src/services/extractMemories/
//!
//! Extracts durable memories from the current session transcript
//! and writes them to the auto-memory directory.

use crate::AgentError;
use crate::constants::env::ai;
use crate::types::*;

/// Memory extraction configuration
#[derive(Debug, Clone)]
pub struct ExtractMemoriesConfig {
    /// Minimum messages before extraction
    pub min_messages: u32,
    /// Minimum tool calls before extraction
    pub min_tool_calls: u32,
    /// Whether to extract auto memories only
    pub auto_only: bool,
    /// Maximum memory entries to extract
    pub max_entries: u32,
}

impl Default for ExtractMemoriesConfig {
    fn default() -> Self {
        Self {
            min_messages: 10,
            min_tool_calls: 3,
            auto_only: false,
            max_entries: 50,
        }
    }
}

/// Memory entry extracted from conversation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    /// Entry key (file path relative to memory dir)
    pub key: String,
    /// Entry content
    pub content: String,
    /// Entry type (key_points, decisions, open_items, context)
    pub entry_type: MemoryEntryType,
    /// Whether this is an auto memory
    pub is_auto: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEntryType {
    KeyPoints,
    Decisions,
    OpenItems,
    Context,
}

/// Result of memory extraction
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractMemoriesResult {
    pub success: bool,
    pub entries: Vec<MemoryEntry>,
    pub error: Option<String>,
    pub messages_processed: u32,
    pub tool_calls_count: u32,
}

/// Pending extraction queue item
#[derive(Debug, Clone)]
pub struct PendingExtraction {
    pub session_id: String,
    pub messages: Vec<Message>,
    pub timestamp: u64,
}

impl PendingExtraction {
    pub fn new(session_id: String, messages: Vec<Message>) -> Self {
        Self {
            session_id,
            messages,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Extract memories state
#[derive(Debug, Clone)]
pub struct ExtractMemoriesState {
    config: ExtractMemoriesConfig,
    pending_extractions: Vec<PendingExtraction>,
    is_extracting: bool,
    last_extraction_time: Option<u64>,
}

impl ExtractMemoriesState {
    pub fn new() -> Self {
        Self {
            config: ExtractMemoriesConfig::default(),
            pending_extractions: Vec::new(),
            is_extracting: false,
            last_extraction_time: None,
        }
    }

    pub fn with_config(config: ExtractMemoriesConfig) -> Self {
        Self {
            config,
            pending_extractions: Vec::new(),
            is_extracting: false,
            last_extraction_time: None,
        }
    }

    pub fn is_extracting(&self) -> bool {
        self.is_extracting
    }

    pub fn set_extracting(&mut self, extracting: bool) {
        self.is_extracting = extracting;
    }

    pub fn add_pending(&mut self, extraction: PendingExtraction) {
        self.pending_extractions.push(extraction);
    }

    pub fn pop_pending(&mut self) -> Option<PendingExtraction> {
        self.pending_extractions.pop()
    }

    pub fn pending_count(&self) -> usize {
        self.pending_extractions.len()
    }

    pub fn update_extraction_time(&mut self) {
        self.last_extraction_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    pub fn get_config(&self) -> &ExtractMemoriesConfig {
        &self.config
    }
}

impl Default for ExtractMemoriesState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helper Functions ───────────────────────────────────────────

/// Check if message is visible to the model (user or assistant)
fn is_model_visible_message(message: &Message) -> bool {
    matches!(message.role, MessageRole::User | MessageRole::Assistant)
}

/// Count model-visible messages since a given index
pub fn count_model_visible_messages_since(
    messages: &[Message],
    since_index: Option<usize>,
) -> usize {
    let start = since_index.unwrap_or(0);
    messages
        .iter()
        .skip(start)
        .filter(|m| is_model_visible_message(m))
        .count()
}

/// Count tool calls in messages
pub fn count_tool_calls(messages: &[Message]) -> usize {
    let mut count = 0;
    for message in messages {
        if message.role == MessageRole::Assistant {
            if let Some(ref tool_calls) = message.tool_calls {
                count += tool_calls.len();
            }
            // Also check content for tool use
            if message.content.contains("tool_use") {
                count += 1;
            }
        }
    }
    count
}

/// Check if extraction should run based on thresholds
pub fn should_extract_memories(messages: &[Message], config: &ExtractMemoriesConfig) -> bool {
    let visible_count = messages
        .iter()
        .filter(|m| is_model_visible_message(m))
        .count();
    let tool_call_count = count_tool_calls(messages);

    (visible_count as u32) >= config.min_messages
        && (tool_call_count as u32) >= config.min_tool_calls
}

// ─── Prompt Building ───────────────────────────────────────────

/// Build prompt for extracting auto-only memories
pub fn build_extract_auto_only_prompt() -> String {
    r#"Extract key information from this conversation for memory.

Focus on:
1. Key Points - Important facts, findings, or conclusions
2. Decisions Made - Any decisions or commitments
3. Open Items - Tasks or questions still pending

Provide your output as markdown that can be saved to memory files.
Keep it concise but informative.

Current conversation:"#
        .to_string()
}

/// Build prompt for extracting combined memories (auto + manual)
pub fn build_extract_combined_prompt() -> String {
    r#"Extract key information from this conversation for memory.

Focus on:
1. Key Points - Important facts, findings, or conclusions
2. Decisions Made - Any decisions or commitments
3. Open Items - Tasks or questions still pending
4. Context - Important background information that would help in future sessions

Provide your output as markdown files with clear headers for each category.
Keep it concise but informative.

Current conversation:"#
        .to_string()
}

// ─── Memory Parsing ─────────────────────────────────────────────

/// Parse extracted content into memory entries
pub fn parse_extracted_content(content: &str, is_auto: bool) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();

    // Simple parsing based on markdown headers
    let mut current_section = String::new();
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            // Save previous section
            if !current_content.trim().is_empty() {
                let entry_type = match current_section.to_lowercase().as_str() {
                    s if s.contains("key") => MemoryEntryType::KeyPoints,
                    s if s.contains("decision") => MemoryEntryType::Decisions,
                    s if s.contains("open") => MemoryEntryType::OpenItems,
                    s if s.contains("context") => MemoryEntryType::Context,
                    _ => MemoryEntryType::Context,
                };
                entries.push(MemoryEntry {
                    key: format!("{}.md", current_section.to_lowercase().replace(' ', "_")),
                    content: current_content.trim().to_string(),
                    entry_type,
                    is_auto,
                });
            }
            current_section = line.trim_start_matches("## ").to_string();
            current_content = String::new();
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Don't forget last section
    if !current_content.trim().is_empty() {
        let entry_type = match current_section.to_lowercase().as_str() {
            s if s.contains("key") => MemoryEntryType::KeyPoints,
            s if s.contains("decision") => MemoryEntryType::Decisions,
            s if s.contains("open") => MemoryEntryType::OpenItems,
            s if s.contains("context") => MemoryEntryType::Context,
            _ => MemoryEntryType::Context,
        };
        entries.push(MemoryEntry {
            key: format!("{}.md", current_section.to_lowercase().replace(' ', "_")),
            content: current_content.trim().to_string(),
            entry_type,
            is_auto,
        });
    }

    entries
}

// ─── Extraction Functions ───────────────────────────────────────

/// Execute memory extraction (placeholder - requires agent integration)
pub async fn execute_extract_memories(
    messages: Vec<Message>,
    config: ExtractMemoriesConfig,
) -> Result<ExtractMemoriesResult, AgentError> {
    // Check thresholds
    if !should_extract_memories(&messages, &config) {
        return Ok(ExtractMemoriesResult {
            success: true,
            entries: Vec::new(),
            error: None,
            messages_processed: messages.len() as u32,
            tool_calls_count: count_tool_calls(&messages) as u32,
        });
    }

    // In a full implementation, this would:
    // 1. Create a forked agent
    // 2. Send extraction prompt with conversation
    // 3. Parse response into memory entries
    // 4. Write to memory directory

    Ok(ExtractMemoriesResult {
        success: false,
        entries: Vec::new(),
        error: Some("Memory extraction requires agent integration".to_string()),
        messages_processed: messages.len() as u32,
        tool_calls_count: count_tool_calls(&messages) as u32,
    })
}

/// Drain pending extractions (placeholder)
pub async fn drain_pending_extractions(
    state: &mut ExtractMemoriesState,
) -> Result<Vec<ExtractMemoriesResult>, AgentError> {
    let mut results = Vec::new();

    while let Some(pending) = state.pop_pending() {
        let result = execute_extract_memories(pending.messages, state.get_config().clone()).await?;
        results.push(result);
        state.update_extraction_time();
    }

    Ok(results)
}

// ─── Auto Memory Tool Check ─────────────────────────────────────

/// Tool name constants (from TypeScript tools)
pub const TOOL_NAME_FILE_READ: &str = "Read";
pub const TOOL_NAME_FILE_WRITE: &str = "Write";
pub const TOOL_NAME_FILE_EDIT: &str = "Edit";
pub const TOOL_NAME_GLOB: &str = "Glob";
pub const TOOL_NAME_GREP: &str = "Grep";
pub const TOOL_NAME_BASH: &str = "Bash";
pub const TOOL_NAME_REPL: &str = "REPL";

/// Permission decision for tool use
#[derive(Debug, Clone)]
pub struct ToolPermission {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
}

/// Create can-use-tool function for auto memory operations
/// Allows Read/Grep/Glob unrestricted, read-only Bash, and Edit/Write only for auto-memory paths
pub fn create_auto_mem_can_use_tool(
    memory_dir: &str,
) -> impl Fn(&str, Option<&str>) -> ToolPermission + '_ {
    move |tool_name: &str, file_path: Option<&str>| {
        // Allow REPL
        if tool_name == TOOL_NAME_REPL {
            return ToolPermission {
                behavior: PermissionBehavior::Allow,
                message: None,
            };
        }

        // Allow Read/Grep/Glob unrestricted
        if matches!(
            tool_name,
            TOOL_NAME_FILE_READ | TOOL_NAME_GREP | TOOL_NAME_GLOB
        ) {
            return ToolPermission {
                behavior: PermissionBehavior::Allow,
                message: None,
            };
        }

        // For Bash, we need to check if it's read-only (would require additional context)
        // For now, deny Bash in auto-memory context unless it's explicitly read-only
        if tool_name == TOOL_NAME_BASH {
            return ToolPermission {
                behavior: PermissionBehavior::Deny,
                message: Some("Only read-only shell commands are permitted in this context (ls, find, grep, cat, stat, wc, head, tail, and similar)".to_string()),
            };
        }

        // Allow Edit/Write only for paths within auto-memory directory
        if tool_name == TOOL_NAME_FILE_EDIT || tool_name == TOOL_NAME_FILE_WRITE {
            if let Some(path) = file_path {
                if is_auto_mem_path_str(path, memory_dir) {
                    return ToolPermission {
                        behavior: PermissionBehavior::Allow,
                        message: None,
                    };
                }
            }
        }

        ToolPermission {
            behavior: PermissionBehavior::Deny,
            message: Some(format!(
                "only {}, {}, {}, read-only {}, and {}/{} within {} are allowed",
                TOOL_NAME_FILE_READ,
                TOOL_NAME_GREP,
                TOOL_NAME_GLOB,
                TOOL_NAME_BASH,
                TOOL_NAME_FILE_EDIT,
                TOOL_NAME_FILE_WRITE,
                memory_dir
            )),
        }
    }
}

/// Check if a path is within the auto-memory directory
fn is_auto_mem_path_str(absolute_path: &str, memory_dir: &str) -> bool {
    absolute_path.starts_with(memory_dir)
}

// ─── Message UUID Helpers ────────────────────────────────────────

/// Get UUID from message (messages should have id field for this to work)
#[allow(dead_code)]
pub fn get_message_uuid(_message: &Message) -> Option<&str> {
    // Message struct doesn't have uuid field - this is a limitation
    // In a full implementation, we'd add uuid to the Message type
    None
}

/// Count model-visible messages since a given UUID
/// Returns count of all model-visible messages if since_uuid is None
pub fn count_model_visible_messages_since_uuid(
    messages: &[Message],
    since_uuid: Option<&str>,
) -> usize {
    if since_uuid.is_none() {
        return messages
            .iter()
            .filter(|m| is_model_visible_message(m))
            .count();
    }

    let since_uuid = since_uuid.unwrap();
    let mut found_start = false;
    let mut n = 0;

    for message in messages {
        if !found_start {
            // Try to match by id or content hash as fallback
            if get_message_uuid(message) == Some(since_uuid) {
                found_start = true;
            }
            continue;
        }
        if is_model_visible_message(message) {
            n += 1;
        }
    }

    // If since_uuid was not found, fall back to counting all model-visible messages
    if !found_start {
        return messages
            .iter()
            .filter(|m| is_model_visible_message(m))
            .count();
    }

    n
}

// ─── Memory Write Detection ───────────────────────────────────────

/// Check if any assistant message after the cursor UUID contains a
/// Write/Edit tool_use block targeting an auto-memory path
pub fn has_memory_writes_since(
    messages: &[Message],
    since_uuid: Option<&str>,
    memory_dir: &str,
) -> bool {
    let mut found_start = since_uuid.is_none();

    for message in messages {
        if !found_start {
            if let Some(uuid) = get_message_uuid(message) {
                if uuid == since_uuid.unwrap() {
                    found_start = true;
                }
            }
            continue;
        }

        if message.role != MessageRole::Assistant {
            continue;
        }

        // Check for tool calls in the message
        if let Some(ref tool_calls) = message.tool_calls {
            for tool_call in tool_calls {
                let name = &tool_call.name;
                if name == TOOL_NAME_FILE_WRITE || name == TOOL_NAME_FILE_EDIT {
                    // Extract file_path from tool call arguments
                    if let Some(file_path) = extract_file_path_from_args(&tool_call.arguments) {
                        if is_auto_mem_path_str(&file_path, memory_dir) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Extract file_path from tool call arguments
fn extract_file_path_from_args(args: &serde_json::Value) -> Option<String> {
    if let Some(obj) = args.as_object() {
        if let Some(fp) = obj.get("file_path") {
            return fp.as_str().map(|s| s.to_string());
        }
    }
    None
}

/// Extract file_path from a tool_use block's input (for content blocks)
pub fn get_written_file_path(block: &serde_json::Value) -> Option<String> {
    let obj = block.as_object()?;

    // Check if it's a tool_use block
    if obj.get("type")?.as_str()? != "tool_use" {
        return None;
    }

    let name = obj.get("name")?.as_str()?;
    if name != TOOL_NAME_FILE_WRITE && name != TOOL_NAME_FILE_EDIT {
        return None;
    }

    let input = obj.get("input")?;
    let input_obj = input.as_object()?;

    let fp = input_obj.get("file_path")?;
    fp.as_str().map(|s| s.to_string())
}

/// Extract all written file paths from agent messages
pub fn extract_written_paths(agent_messages: &[Message]) -> Vec<String> {
    let mut paths = Vec::new();

    for message in agent_messages {
        if message.role != MessageRole::Assistant {
            continue;
        }

        // Check tool_calls
        if let Some(ref tool_calls) = message.tool_calls {
            for tool_call in tool_calls {
                let name = &tool_call.name;
                if name == TOOL_NAME_FILE_WRITE || name == TOOL_NAME_FILE_EDIT {
                    if let Some(fp) = extract_file_path_from_args(&tool_call.arguments) {
                        paths.push(fp);
                    }
                }
            }
        }
    }

    // Deduplicate paths
    paths.sort();
    paths.dedup();
    paths
}

// ─── Initialization & Closure-scoped State ───────────────────────

/// Initialize the memory extraction system
/// Creates a fresh closure that captures all mutable state
#[allow(dead_code)]
pub struct ExtractMemories {
    in_flight: std::sync::Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    last_memory_message_uuid: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    has_logged_gate_failure: std::sync::Arc<std::sync::Mutex<bool>>,
    in_progress: std::sync::Arc<std::sync::Mutex<bool>>,
    turns_since_last_extraction: std::sync::Arc<std::sync::Mutex<u32>>,
    pending_context: std::sync::Arc<std::sync::Mutex<Option<ExtractMemoriesContext>>>,
}

#[derive(Debug, Clone)]
pub struct ExtractMemoriesContext {
    pub messages: Vec<Message>,
    // Additional context fields would be here
}

impl ExtractMemories {
    pub fn new() -> Self {
        Self {
            in_flight: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            last_memory_message_uuid: std::sync::Arc::new(std::sync::Mutex::new(None)),
            has_logged_gate_failure: std::sync::Arc::new(std::sync::Mutex::new(false)),
            in_progress: std::sync::Arc::new(std::sync::Mutex::new(false)),
            turns_since_last_extraction: std::sync::Arc::new(std::sync::Mutex::new(0)),
            pending_context: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Check if extraction should run based on gate settings
    pub fn is_gate_enabled() -> bool {
        // Check feature flag - in Rust we'd integrate with a feature flags system
        // For now, default to enabled
        std::env::var(ai::DISABLE_EXTRACT_MEMORIES).ok() != Some("1".to_string())
    }

    /// Check if auto-memory is enabled
    pub fn is_auto_memory_enabled() -> bool {
        crate::memdir::paths::is_auto_memory_enabled()
    }

    /// Check if we're in remote mode
    pub fn is_remote_mode() -> bool {
        std::env::var(ai::REMOTE).ok() == Some("1".to_string())
    }

    /// Execute memory extraction
    pub async fn execute(&self, context: ExtractMemoriesContext) {
        // Quick early returns for common skip conditions
        // (In full implementation, check agentId etc)

        if !Self::is_gate_enabled() {
            return;
        }

        if !Self::is_auto_memory_enabled() {
            return;
        }

        if Self::is_remote_mode() {
            return;
        }

        // Check if already in progress
        {
            let in_progress = self.in_progress.lock().unwrap();
            if *in_progress {
                // Stash context for trailing run
                let mut pending = self.pending_context.lock().unwrap();
                *pending = Some(context);
                return;
            }
        }

        // Run the extraction
        self.run_extraction(context).await;
    }

    async fn run_extraction(&self, context: ExtractMemoriesContext) {
        // Mark in progress
        {
            let mut in_progress = self.in_progress.lock().unwrap();
            *in_progress = true;
        }

        // Get memory directory
        let memory_dir = crate::memdir::paths::get_auto_mem_path();
        let memory_dir_str = memory_dir.to_string_lossy().to_string();

        // Count new messages since last extraction
        let last_uuid = {
            let guard = self.last_memory_message_uuid.lock().unwrap();
            guard.clone()
        };
        let _new_message_count =
            count_model_visible_messages_since_uuid(&context.messages, last_uuid.as_deref());

        // Check for direct memory writes by the main agent
        if has_memory_writes_since(&context.messages, last_uuid.as_deref(), &memory_dir_str) {
            // Skip extraction - main agent already wrote memories
            if let Some(last_msg) = context.messages.last() {
                if let Some(uuid) = get_message_uuid(last_msg) {
                    let mut guard = self.last_memory_message_uuid.lock().unwrap();
                    *guard = Some(uuid.to_string());
                }
            }
        }

        // Throttle: only run extraction every N eligible turns
        {
            let mut turns = self.turns_since_last_extraction.lock().unwrap();
            *turns += 1;
            if *turns < 1 {
                // Default throttle to 1 - release lock and return
                {
                    let mut in_progress = self.in_progress.lock().unwrap();
                    *in_progress = false;
                }
                return;
            }
            *turns = 0;
        }

        // Note: Full implementation would:
        // 1. Scan existing memories
        // 2. Build extraction prompt
        // 3. Run forked agent
        // 4. Extract written paths
        // 5. Log events
        // 6. Update cursor

        // Mark as complete
        {
            let mut in_progress = self.in_progress.lock().unwrap();
            *in_progress = false;
        }
    }

    /// Drain all in-flight extractions
    pub async fn drain(&self, timeout_ms: Option<u64>) {
        let handles = {
            let mut guard = self.in_flight.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        let timeout = timeout_ms.unwrap_or(60_000);
        let timeout_duration = std::time::Duration::from_millis(timeout);

        for handle in handles {
            let _ = tokio::time::timeout(timeout_duration, handle).await;
        }
    }
}

impl Default for ExtractMemories {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_model_visible_message() {
        let user_msg = Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
        };
        let assistant_msg = Message {
            role: MessageRole::Assistant,
            content: "hi".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
        };
        let tool_msg = Message {
            role: MessageRole::Tool,
            content: "result".to_string(),
            attachments: None,
            tool_call_id: Some("call_1".to_string()),
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
        };

        assert!(is_model_visible_message(&user_msg));
        assert!(is_model_visible_message(&assistant_msg));
        assert!(!is_model_visible_message(&tool_msg));
    }

    #[test]
    fn test_count_model_visible_messages_since() {
        let messages = vec![
            Message {
                role: MessageRole::User,
                content: "hello".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            },
            Message {
                role: MessageRole::Assistant,
                content: "hi".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            },
            Message {
                role: MessageRole::User,
                content: "question".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            },
        ];

        assert_eq!(count_model_visible_messages_since(&messages, None), 3);
        // Note: since_uuid requires message.id which isn't available in current Message struct
        // This test verifies the basic case works
    }

    #[test]
    fn test_should_extract_memories() {
        let config = ExtractMemoriesConfig::default();

        let few_messages = vec![Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
        }];

        assert!(!should_extract_memories(&few_messages, &config));

        let enough_messages: Vec<Message> = (0..15)
            .map(|i| Message {
                role: if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                // Include tool_use in content for some assistant messages to trigger threshold
                content: if i % 3 == 1 {
                    format!("message {} tool_use", i)
                } else {
                    format!("message {}", i)
                },
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
            is_api_error_message: None,
            error_details: None,
            uuid: None,
            })
            .collect();

        assert!(should_extract_memories(&enough_messages, &config));
    }

    #[test]
    fn test_extract_memories_state() {
        let mut state = ExtractMemoriesState::new();
        assert!(!state.is_extracting());

        state.set_extracting(true);
        assert!(state.is_extracting());

        let extraction = PendingExtraction::new("session_1".to_string(), vec![]);
        state.add_pending(extraction);
        assert_eq!(state.pending_count(), 1);

        let popped = state.pop_pending();
        assert!(popped.is_some());
        assert_eq!(state.pending_count(), 0);
    }

    #[test]
    fn test_parse_extracted_content() {
        let content = r#"## Key Points
- First important point
- Second important point

## Decisions Made
- Decision one
- Decision two

## Open Items
- Task one
"#;

        let entries = parse_extracted_content(content, true);
        assert!(!entries.is_empty());

        let key_points = entries.iter().find(|e| e.key.contains("key_points"));
        assert!(key_points.is_some());
        assert!(
            key_points
                .unwrap()
                .content
                .contains("First important point")
        );
    }
}
