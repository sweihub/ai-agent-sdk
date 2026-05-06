// Source: /data/home/swei/claudecode/openclaudecode/src/utils/filePersistence/types.ts
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    /// Unique identifier for this message (used by session memory to track extraction boundary)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachments: Option<Vec<Attachment>>,
    /// Tool call ID for tool role messages (required by OpenAI API)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    /// Tool calls for assistant messages (required to pair with tool results)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Indicates if this message is an error (for tool results)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_error: Option<bool>,
    /// Indicates if this is a meta/system message (e.g., from prependUserContext)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_meta: Option<bool>,
    /// Indicates this assistant message was generated from an API error (not model-produced text)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_api_error_message: Option<bool>,
    /// Structured error details for API error messages (raw API error string)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_details: Option<String>,
    /// Unix epoch timestamp (milliseconds) when this message was created.
    /// Used by microcompact for time-based trigger and session storage.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timestamp: Option<u64>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            role: MessageRole::User,
            content: String::new(),
            uuid: None,
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
            is_api_error_message: None,
            error_details: None,
            timestamp: None,
        }
    }
}

/// A tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(default = "default_tool_call_type")]
    pub r#type: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

fn default_tool_call_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    #[default]
    User,
    Assistant,
    #[serde(rename = "tool")]
    Tool,
    System,
}

impl MessageRole {
    /// Convert the role to its string representation for API serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        }
    }
}

/// Attachments for messages - files, images, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Attachment {
    /// File attachment (at-mentioned file)
    File { path: String },
    /// Reference to a file previously read
    AlreadyReadFile { path: String, content: String },
    /// PDF reference
    PdfReference { path: String },
    /// Text file that was edited
    EditedTextFile { filename: String, snippet: String },
    /// Image file that was edited
    EditedImageFile { filename: String },
    /// Directory listing
    Directory {
        path: String,
        content: String,
        display_path: String,
    },
    /// Selected lines in IDE
    SelectedLinesInIde {
        ide_name: String,
        filename: String,
        start_line: u32,
        end_line: u32,
    },
    /// Memory file reference
    MemoryFile { path: String },
    /// Skill listing attachment
    SkillListing { skills: Vec<SkillInfo> },
    /// Invoked skills attachment
    InvokedSkills { skills: Vec<InvokedSkill> },
    /// Task status
    TaskStatus {
        task_id: String,
        description: String,
        status: String,
    },
    /// Plan file reference
    PlanFileReference { path: String },
    /// MCP tool resources
    McpResources { tools: Vec<String> },
    /// Deferred tools delta
    DeferredTools { tools: Vec<String> },
    /// Agent listing
    AgentListing { agents: Vec<String> },
    /// Custom attachment
    Custom {
        name: String,
        content: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokedSkill {
    pub name: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<Vec<IterationUsage>>,
}

/// Per-iteration usage from the Anthropic API (server-side tool loops)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IterationUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: ToolInputSchema,
    /// Optional annotations for tool classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// When true, this tool is deferred (requires ToolSearch to load)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub should_defer: Option<bool>,
    /// When true, this tool is never deferred (full schema always sent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_load: Option<bool>,
    /// When true, this is an MCP tool
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_mcp: Option<bool>,
    /// Short capability phrase for keyword search (3-10 words)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_hint: Option<String>,
    /// Optional aliases for backwards compatibility when a tool is renamed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// Human-readable name for display in the UI (e.g., "Update" vs "Edit")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_facing_name: Option<String>,
    /// Interrupt behavior: 'cancel' (stop on user interrupt) or 'block' (wait for completion)
    #[serde(rename = "interruptBehavior", default, skip_serializing_if = "Option::is_none")]
    pub interrupt_behavior: Option<String>,
}

impl Default for ToolDefinition {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            input_schema: ToolInputSchema::default(),
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }
}

impl ToolDefinition {
    /// Create a new tool definition (annotations defaults to None)
    pub fn new(name: &str, description: &str, input_schema: ToolInputSchema) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }

    /// Build with deferred loading support
    pub fn with_deferred(mut self, should_defer: bool) -> Self {
        self.should_defer = Some(should_defer);
        self
    }

    /// Mark as always loaded (never deferred)
    pub fn with_always_load(mut self) -> Self {
        self.always_load = Some(true);
        self
    }

    /// Mark as MCP tool
    pub fn with_mcp(mut self) -> Self {
        self.is_mcp = Some(true);
        self
    }

    /// Set search hint for keyword search
    pub fn with_search_hint(mut self, hint: &str) -> Self {
        self.search_hint = Some(hint.to_string());
        self
    }

    /// Get interrupt behavior: 'cancel' (stop on user interrupt) or 'block' (wait for completion)
    /// Default: 'block'
    pub fn interrupt_behavior(&self) -> crate::tools::types::InterruptBehavior {
        match self.interrupt_behavior.as_deref() {
            Some("cancel") => crate::tools::types::InterruptBehavior::Cancel,
            _ => crate::tools::types::InterruptBehavior::Block,
        }
    }

    /// Backfill observable input before observers see it (hooks, events, transcript).
    /// Mutates in place to add legacy/derived fields. Must be idempotent.
    /// Default: no-op. Override via `with_interrupt_behavior` for tools that need it.
    pub fn backfill_observable_input(&self, _input: &mut serde_json::Value) {
        // Default no-op. Tools that need backfilling should set interrupt_behavior
        // or use the Tool trait's backfill_observable_input directly.
    }

    /// Check if tool can run concurrently (default: false)
    pub fn is_concurrency_safe(&self, _input: &serde_json::Value) -> bool {
        self.annotations
            .as_ref()
            .and_then(|a| a.concurrency_safe)
            .unwrap_or(false)
    }

    /// Check if tool only reads data (default: false)
    pub fn is_read_only(&self, _input: &serde_json::Value) -> bool {
        if let Some(ref a) = self.annotations {
            if let Some(ro) = a.read_only {
                return ro;
            }
        }
        // Default: tools that only read
        matches!(
            self.name.as_str(),
            "Read" | "Glob" | "Grep" | "Search" | "WebFetch" | "WebSearch"
        )
    }

    /// Check if tool performs destructive operations (default: false)
    pub fn is_destructive(&self, input: &serde_json::Value) -> bool {
        if let Some(ref a) = self.annotations {
            if let Some(d) = a.destructive {
                return d;
            }
        }
        // Default: check input for destructive commands
        let input_str = input.to_string();
        matches!(self.name.as_str(), "Bash" | "Write" | "Edit")
            && (input_str.contains("rm -rf")
                || input_str.contains("rm /")
                || input_str.contains("dd if=")
                || input_str.contains("format"))
    }

    /// Check if tool is idempotent (can be run multiple times safely)
    pub fn is_idempotent(&self) -> bool {
        self.annotations
            .as_ref()
            .and_then(|a| a.idempotent)
            .unwrap_or(false)
    }

    /// Get tool use summary for compact views
    pub fn get_use_summary(&self, input: &serde_json::Value) -> String {
        match self.name.as_str() {
            "Bash" => {
                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                    let truncated = if cmd.len() > 50 {
                        format!("{}...", &cmd[..50])
                    } else {
                        cmd.to_string()
                    };
                    format!("Bash: {}", truncated)
                } else {
                    "Bash".to_string()
                }
            }
            "Read" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    format!("Read: {}", path)
                } else {
                    "Read".to_string()
                }
            }
            "Write" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    format!("Write: {}", path)
                } else {
                    "Write".to_string()
                }
            }
            "Edit" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    format!("Edit: {}", path)
                } else {
                    "Edit".to_string()
                }
            }
            "Glob" => {
                if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                    format!("Glob: {}", pattern)
                } else {
                    "Glob".to_string()
                }
            }
            "Grep" => {
                if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                    format!("Grep: {}", pattern)
                } else {
                    "Grep".to_string()
                }
            }
            _ => self.name.clone(),
        }
    }
}

/// Tool annotations for classification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    /// Tool can run concurrently with other tools
    #[serde(rename = "concurrencySafe", skip_serializing_if = "Option::is_none")]
    pub concurrency_safe: Option<bool>,
    /// Tool only reads data (doesn't modify files/system)
    #[serde(rename = "readOnly", skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// Tool performs destructive operations
    #[serde(rename = "destructive", skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    /// Tool is idempotent (safe to run multiple times)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent: Option<bool>,
    /// Tool operates on open world (external URLs, etc.)
    #[serde(rename = "openWorld", skip_serializing_if = "Option::is_none")]
    pub open_world: Option<bool>,
}

impl ToolAnnotations {
    /// Create annotations for read-only tools
    pub fn read_only() -> Self {
        Self {
            read_only: Some(true),
            ..Default::default()
        }
    }

    /// Create annotations for destructive tools
    pub fn destructive() -> Self {
        Self {
            destructive: Some(true),
            ..Default::default()
        }
    }

    /// Create annotations for concurrent-safe tools
    pub fn concurrency_safe() -> Self {
        Self {
            concurrency_safe: Some(true),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: serde_json::Value,
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolContext {
    pub cwd: String,
    #[serde(skip)]
    pub abort_signal: std::sync::Arc<crate::utils::AbortSignal>,
}

// Skip Serialize on ToolContext because Arc<AbortSignal> doesn't implement Serialize
impl Serialize for ToolContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize only the cwd field, skipping abort_signal
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ToolContext", 1)?;
        state.serialize_field("cwd", &self.cwd)?;
        state.end()
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            cwd: String::new(),
            abort_signal: std::sync::Arc::new(crate::utils::AbortSignal::new(0)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    #[serde(rename = "type")]
    pub result_type: String,
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_persisted: Option<bool>,
}

impl Default for ToolResult {
    fn default() -> Self {
        Self {
            result_type: String::new(),
            tool_use_id: String::new(),
            content: String::new(),
            is_error: None,
            was_persisted: None,
        }
    }
}

/// Content block within a tool_result — either text or tool_reference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Plain text content
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
    },
    /// Reference to a deferred tool that the API should expand
    ToolReference {
        #[serde(rename = "type")]
        content_type: String,
        tool_name: String,
    },
}

impl ToolResultContent {
    pub fn text(text: &str) -> Self {
        Self::Text {
            content_type: "text".to_string(),
            text: text.to_string(),
        }
    }

    pub fn tool_reference(tool_name: &str) -> Self {
        Self::ToolReference {
            content_type: "tool_reference".to_string(),
            tool_name: tool_name.to_string(),
        }
    }
}

/// Tool result with structured content (supports tool_reference blocks)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultStructured {
    #[serde(rename = "type")]
    pub result_type: String,
    pub tool_use_id: String,
    pub content: Vec<ToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Thinking configuration for the API (matches TypeScript ThinkingConfig)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ThinkingConfig {
    /// Adaptive thinking - model decides the best thinking budget
    Adaptive,
    /// Enabled with a specific budget token count
    Enabled {
        #[serde(rename = "budgetTokens")]
        budget_tokens: u32,
    },
    /// Thinking disabled
    Disabled,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        // Default to adaptive thinking (matches TypeScript shouldEnableThinkingByDefault)
        ThinkingConfig::Adaptive
    }
}

/// Exit reasons from the query loop (matches TypeScript Terminal type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExitReason {
    /// Normal completion - no more tool calls needed
    Completed,
    /// Maximum turns reached
    MaxTurns { max_turns: u32, turn_count: u32 },
    /// Aborted during model streaming
    AbortedStreaming { reason: String },
    /// Aborted during tool execution
    AbortedTools { reason: String },
    /// Hook prevented continuation
    HookStopped,
    /// Stop hook prevented continuation
    StopHookPrevented,
    /// Context too long (prompt_too_long)
    PromptTooLong { error: Option<String> },
    /// Media error (image too large, etc.)
    ImageError { error: String },
    /// Model/runtime error
    ModelError { error: String },
    /// Token limit reached (blocking_limit)
    BlockingLimit,
    /// Token budget continuation ended early
    TokenBudgetExhausted { reason: String },
    /// Max output tokens reached (during generation)
    MaxTokens,
    /// USD budget exceeded
    MaxBudgetExceeded { max_budget_usd: f64 },
}

impl Default for ExitReason {
    fn default() -> Self {
        ExitReason::Completed
    }
}

/// Compact progress event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompactProgressEvent {
    #[serde(rename = "hooks_start")]
    HooksStart {
        #[serde(rename = "hookType")]
        hook_type: CompactHookType,
    },
    #[serde(rename = "compact_start")]
    CompactStart,
    #[serde(rename = "compact_end")]
    CompactEnd {
        /// Human-readable summary emitted to TUI/CLI after successful compaction,
        /// e.g. "Conversation compacted: 120.3k → 8.2k tokens (93%)"
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Compact hook types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactHookType {
    PreCompact,
    PostCompact,
    SessionStart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub text: String,
    pub usage: TokenUsage,
    pub num_turns: u32,
    pub duration_ms: u64,
    /// Why the query loop terminated
    pub exit_reason: ExitReason,
}

/// Agent event types for streaming updates (matches TypeScript behavior)
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Tool is about to be executed
    ToolStart {
        tool_name: String,
        tool_call_id: String,
        input: serde_json::Value,
        /// Human-readable display name (e.g., "Update" for edits, "Create" for new files).
        /// Overrides `tool_name` for display in the TUI.
        display_name: Option<String>,
        /// Short summary for compact views (e.g., file path for FileEdit).
        summary: Option<String>,
        /// Activity description for spinner display (e.g., "Updating file.rs").
        activity_description: Option<String>,
    },
    /// Tool execution completed
    ToolComplete {
        tool_name: String,
        tool_call_id: String,
        result: ToolResult,
        /// Human-readable display name (e.g., "Update(file.rs) (1 +, 1 -)").
        display_name: Option<String>,
        /// Rendered result message from Tool::render_tool_result_message.
        rendered_result: Option<String>,
    },
    /// Tool execution failed
    ToolError {
        tool_name: String,
        tool_call_id: String,
        error: String,
    },
    /// LLM is thinking (started a new turn)
    Thinking { turn: u32 },
    /// Final response ready
    Done { result: QueryResult },
    /// Message started (streaming begins)
    MessageStart { message_id: String },
    /// Content block started (matches TypeScript StreamEvent content_block_start)
    ContentBlockStart { index: u32, block_type: String },
    /// Content block delta (matches TypeScript StreamEvent content_block_delta)
    ContentBlockDelta { index: u32, delta: ContentDelta },
    /// Content block stopped (matches TypeScript StreamEvent content_block_stop)
    ContentBlockStop { index: u32 },
    /// Message stopped (streaming ends)
    MessageStop,
    /// Request started (before API call) - matches TypeScript 'stream_request_start'
    RequestStart,
    /// Request completed (API response received, streaming finished)
    /// Matches TypeScript 'stream_request_end' — useful for TUI spinner management.
    StreamRequestEnd,
    /// Rate limit status change — notifies TUI/CLI when a rate limit is hit or cleared
    RateLimitStatus {
        /// true if currently rate-limited, false if rate limit has cleared
        is_rate_limited: bool,
        /// Optional retry-after seconds (if the server provided it)
        retry_after_secs: Option<f64>,
    },
    /// Max turns reached - matches TypeScript 'max_turns_reached' attachment
    MaxTurnsReached { max_turns: u32, turn_count: u32 },
    /// Tombstone event for orphaned messages on streaming fallback
    /// (matches TypeScript 'tombstone' event)
    Tombstone { message: String },
    /// Compact progress event (hooks_start, compact_start, compact_end)
    /// Matches TypeScript ToolUseContext.onCompactProgress
    Compact { event: CompactProgressEvent },
    /// Actual API token usage from message_delta event
    /// Emitted after all content_block_stop events, before message_stop
    TokenUsage {
        usage: TokenUsage,
        cost: f64,
    },
    /// API retry progress — emitted during 429/529 retry backoff
    /// Matches TypeScript's 'api_retry' subtype yielded by QueryEngine
    /// from createSystemAPIErrorMessage in withRetry.ts
    ApiRetry {
        /// Current retry attempt (1-based)
        attempt: u32,
        /// Maximum retries configured
        max_retries: u32,
        /// Delay in milliseconds before next retry
        retry_delay_ms: u64,
        /// HTTP error status code that triggered the retry
        error_status: Option<u16>,
        /// Categorized error type (e.g., "rate_limit", "server_error")
        error: String,
    },
}

/// Content delta types for streaming
#[derive(Debug, Clone)]
pub enum ContentDelta {
    /// Text content delta
    Text { text: String },
    /// Thinking content delta (internal reasoning)
    Thinking { text: String },
    /// Tool use input delta (streaming tool arguments)
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        is_complete: bool,
    },
}

// --------------------------------------------------------------------------
// MCP Types
// --------------------------------------------------------------------------

/// MCP server configuration (union type)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerConfig {
    Stdio(McpStdioConfig),
    Sse(McpSseConfig),
    Http(McpHttpConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStdioConfig {
    #[serde(default = "default_stdio_type")]
    pub transport_type: Option<String>,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

fn default_stdio_type() -> Option<String> {
    Some("stdio".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSseConfig {
    pub transport_type: String,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpConfig {
    pub transport_type: String,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// MCP connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpConnectionStatus {
    Connected,
    Disconnected,
    Error,
}

/// MCP tool representation from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<serde_json::Value>,
}

// --------------------------------------------------------------------------
// Tool Types (translated from Tool.ts)
// --------------------------------------------------------------------------

/// Query chain tracking for nested agent calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryChainTracking {
    pub chain_id: String,
    pub depth: u32,
}

/// Validation result for tool input
/// Source: /data/home/swei/claudecode/openclaudecode/src/Tool.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result")]
pub enum ValidationResult {
    /// Validation passed
    #[serde(rename = "true")]
    Valid,
    /// Validation failed with error details
    Invalid {
        /// Error message describing the validation failure
        message: String,
        /// Error code for the validation failure
        #[serde(rename = "errorCode")]
        error_code: i32,
    },
}

impl ValidationResult {
    /// Create a valid validation result
    pub fn valid() -> Self {
        ValidationResult::Valid
    }

    /// Create an invalid validation result
    pub fn invalid(message: String, error_code: i32) -> Self {
        ValidationResult::Invalid {
            message,
            error_code,
        }
    }

    /// Check if the validation passed
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }
}

/// Tool permission mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    Default,
    Auto,
    #[serde(rename = "auto-accept")]
    AutoAccept,
    #[serde(rename = "auto-deny")]
    AutoDeny,
    Bypass,
}

/// Additional working directory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalWorkingDirectory {
    pub path: String,
    #[serde(rename = "permissionMode")]
    pub permission_mode: Option<PermissionMode>,
}

/// Permission result from permission checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResult {
    pub behavior: PermissionBehavior,
    #[serde(rename = "updatedInput")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Permission behavior types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// Additional tool permission rules by source
pub type ToolPermissionRulesBySource = HashMap<String, Vec<String>>;

/// Tool permission context - full context for permission checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    #[serde(rename = "additionalWorkingDirectories")]
    pub additional_working_directories: HashMap<String, AdditionalWorkingDirectory>,
    #[serde(rename = "alwaysAllowRules")]
    pub always_allow_rules: ToolPermissionRulesBySource,
    #[serde(rename = "alwaysDenyRules")]
    pub always_deny_rules: ToolPermissionRulesBySource,
    #[serde(rename = "alwaysAskRules")]
    pub always_ask_rules: ToolPermissionRulesBySource,
    #[serde(rename = "isBypassPermissionsModeAvailable")]
    pub is_bypass_permissions_mode_available: bool,
    #[serde(
        rename = "isAutoModeAvailable",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_auto_mode_available: Option<bool>,
    #[serde(
        rename = "strippedDangerousRules",
        skip_serializing_if = "Option::is_none"
    )]
    pub stripped_dangerous_rules: Option<ToolPermissionRulesBySource>,
    #[serde(
        rename = "shouldAvoidPermissionPrompts",
        skip_serializing_if = "Option::is_none"
    )]
    pub should_avoid_permission_prompts: Option<bool>,
    #[serde(
        rename = "awaitAutomatedChecksBeforeDialog",
        skip_serializing_if = "Option::is_none"
    )]
    pub await_automated_checks_before_dialog: Option<bool>,
    #[serde(rename = "prePlanMode", skip_serializing_if = "Option::is_none")]
    pub pre_plan_mode: Option<PermissionMode>,
}

impl Default for ToolPermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            additional_working_directories: HashMap::new(),
            always_allow_rules: HashMap::new(),
            always_deny_rules: HashMap::new(),
            always_ask_rules: HashMap::new(),
            is_bypass_permissions_mode_available: false,
            is_auto_mode_available: None,
            stripped_dangerous_rules: None,
            should_avoid_permission_prompts: None,
            await_automated_checks_before_dialog: None,
            pre_plan_mode: None,
        }
    }
}

/// Create empty tool permission context
pub fn get_empty_tool_permission_context() -> ToolPermissionContext {
    ToolPermissionContext::default()
}

/// Tool input JSON schema (for MCP tools)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputJSONSchema {
    #[serde(flatten)]
    pub properties: serde_json::Value,
    #[serde(rename = "type")]
    pub schema_type: String,
}

// --------------------------------------------------------------------------
// Tool Progress Types (translated from types/tools.ts)
// --------------------------------------------------------------------------

/// Bash tool progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashProgress {
    #[serde(rename = "shell")]
    pub shell: Option<String>,
    #[serde(rename = "command")]
    pub command: Option<String>,
}

/// REPL tool progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplProgress {
    #[serde(rename = "input")]
    pub input: Option<String>,
    #[serde(rename = "toolName")]
    pub tool_name: Option<String>,
    #[serde(rename = "toolCallId")]
    pub tool_call_id: Option<String>,
}

/// MCP tool progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProgress {
    #[serde(rename = "serverName")]
    pub server_name: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "progress")]
    pub progress: Option<serde_json::Value>,
}

/// Web search progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchProgress {
    #[serde(rename = "query")]
    pub query: String,
    #[serde(rename = "currentStep")]
    pub current_step: Option<String>,
}

/// Task output progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutputProgress {
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "output")]
    pub output: Option<String>,
}

/// Skill tool progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolProgress {
    #[serde(rename = "skill")]
    pub skill: String,
    #[serde(rename = "step")]
    pub step: Option<String>,
}

/// Agent tool progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolProgress {
    #[serde(rename = "description")]
    pub description: String,
    #[serde(rename = "subagentType")]
    pub subagent_type: Option<String>,
}

/// Tool progress data - enum of all progress types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolProgressData {
    #[serde(rename = "bash_progress")]
    BashProgress(BashProgress),
    #[serde(rename = "repl_progress")]
    ReplProgress(ReplProgress),
    #[serde(rename = "mcp_progress")]
    McpProgress(McpProgress),
    #[serde(rename = "web_search_progress")]
    WebSearchProgress(WebSearchProgress),
    #[serde(rename = "task_output_progress")]
    TaskOutputProgress(TaskOutputProgress),
    #[serde(rename = "skill_progress")]
    SkillProgress(SkillToolProgress),
    #[serde(rename = "agent_progress")]
    AgentProgress(AgentToolProgress),
}

/// Tool progress with tool use ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress<P: Clone + serde::Serialize> {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    pub data: P,
}

/// Filter progress messages to only tool progress (not hook progress)
pub fn filter_tool_progress_messages(
    progress_messages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    progress_messages
        .iter()
        .filter(|msg| {
            let data_type = msg.get("data").and_then(|d| d.get("type"));
            data_type.map(|t| t != "hook_progress").unwrap_or(true)
        })
        .cloned()
        .collect()
}
