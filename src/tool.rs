// Source: src/Tool.ts

//! Tool type definitions, traits, and utilities translated from the TypeScript SDK.
//!
//! This module defines the core tool system including:
//! - The `Tool` trait with all optional methods (mirroring the TS `Tool` type)
//! - Tool permission context and validation types
//! - Tool use context for runtime execution
//! - Progress tracking types
//! - Utility functions for tool lookup and matching
//! - The `build_tool` pattern for constructing tools with safe defaults

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Re-exports from sibling modules (avoid cycles, match TS re-export pattern)
// ---------------------------------------------------------------------------
pub use crate::types::hooks::HookProgress;
pub use crate::types::message::{
    AssistantMessage, AttachmentMessage, Message, ProgressMessage, SystemLocalCommandMessage,
    SystemMessage, UserMessage,
};
pub use crate::types::tools::ToolProgressData;
pub use crate::types::{ToolDefinition, ToolInputSchema};

// ---------------------------------------------------------------------------
// Tool input JSON schema
// ---------------------------------------------------------------------------

/// JSON schema for tool input, used when a tool specifies its schema directly
/// in JSON Schema format rather than converting from a Zod schema.
pub type ToolInputJsonSchema = serde_json::Value;

// ---------------------------------------------------------------------------
// Query chain tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryChainTracking {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    pub depth: u32,
}

// ---------------------------------------------------------------------------
// Validation result
// ---------------------------------------------------------------------------

/// Result of validating tool input.
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Input is valid.
    Valid,
    /// Input is invalid with an error message and code.
    Invalid {
        message: String,
        error_code: i64,
    },
}

// ---------------------------------------------------------------------------
// SetToolJSX callback
// ---------------------------------------------------------------------------

/// Callback to update tool JSX/UI rendering state.
/// In Rust this is a generic boxed async callback.
pub type SetToolJsxFn = Arc<
    dyn Fn(Option<SetToolJsxArgs>) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone)]
pub struct SetToolJsxArgs {
    pub should_hide_prompt_input: bool,
    pub should_continue_animation: bool,
    pub show_spinner: bool,
    pub is_local_jsx: bool,
    pub is_immediate: bool,
    /// Set to true to clear a local JSX command (e.g., from its onDone callback)
    pub clear_local_jsx: bool,
}

// ---------------------------------------------------------------------------
// Compact progress events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CompactProgressEvent {
    HooksStart {
        hook_type: String, // "pre_compact" | "post_compact" | "session_start"
    },
    CompactStart,
    CompactEnd,
}

// ---------------------------------------------------------------------------
// Tool permission context (re-export + extension)
// ---------------------------------------------------------------------------

pub use crate::types::permissions::ToolPermissionContext;

/// Creates an empty tool permission context (matches TS `getEmptyToolPermissionContext`).
pub fn get_empty_tool_permission_context() -> ToolPermissionContext {
    ToolPermissionContext {
        mode: "default".to_string(),
        additional_working_directories: HashMap::new(),
        always_allow_rules: crate::types::permissions::ToolPermissionRulesBySource {
            user_settings: None,
            project_settings: None,
            local_settings: None,
            flag_settings: None,
            policy_settings: None,
            cli_arg: None,
            command: None,
            session: None,
        },
        always_deny_rules: crate::types::permissions::ToolPermissionRulesBySource {
            user_settings: None,
            project_settings: None,
            local_settings: None,
            flag_settings: None,
            policy_settings: None,
            cli_arg: None,
            command: None,
            session: None,
        },
        always_ask_rules: crate::types::permissions::ToolPermissionRulesBySource {
            user_settings: None,
            project_settings: None,
            local_settings: None,
            flag_settings: None,
            policy_settings: None,
            cli_arg: None,
            command: None,
            session: None,
        },
        is_bypass_permissions_mode_available: false,
        stripped_dangerous_rules: None,
        should_avoid_permission_prompts: None,
        await_automated_checks_before_dialog: None,
        pre_plan_mode: None,
    }
}

// ---------------------------------------------------------------------------
// Tool use context
// ---------------------------------------------------------------------------

/// Context passed to tool calls during execution, providing access to
/// configuration, state, callbacks, and other runtime infrastructure.
///
/// This mirrors the TypeScript `ToolUseContext` type.
pub struct ToolUseContext {
    pub options: ToolUseContextOptions,
    /// Abort signal (placeholder - wired to the consumer's AbortController).
    pub abort_signal: Option<()>,
    pub read_file_state: Option<Arc<dyn std::any::Any + Send + Sync>>,
    pub get_app_state: Box<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync>,
    pub set_app_state:
        Box<dyn Fn(Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>) + Send + Sync>,
    pub set_app_state_for_tasks: Option<
        Box<dyn Fn(Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>) + Send + Sync>,
    >,
    pub handle_elicitation: Option<
        Arc<
            dyn Fn(String, serde_json::Value, ())
                    -> std::pin::Pin<Box<dyn Future<Output = serde_json::Value> + Send>>
                + Send
                + Sync,
        >,
    >,
    pub set_tool_jsx: Option<SetToolJsxFn>,
    pub add_notification: Option<Arc<dyn Fn(serde_json::Value) + Send + Sync>>,
    pub append_system_message: Option<
        Box<dyn Fn(SystemMessage) + Send + Sync>,
    >,
    pub send_os_notification: Option<
        Box<dyn Fn(String, String) + Send + Sync>,
    >,
    pub nested_memory_attachment_triggers: Option<Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    pub loaded_nested_memory_paths: Option<Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    pub dynamic_skill_dir_triggers: Option<Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    pub discovered_skill_names: Option<Arc<std::sync::Mutex<std::collections::HashSet<String>>>>,
    pub user_modified: bool,
    pub set_in_progress_tool_use_ids:
        Box<dyn Fn(Box<dyn Fn(&std::collections::HashSet<String>) -> std::collections::HashSet<String>>) + Send + Sync>,
    pub set_has_interruptible_tool_in_progress: Option<Box<dyn Fn(bool) + Send + Sync>>,
    pub set_response_length:
        Box<dyn Fn(Box<dyn Fn(usize) -> usize>) + Send + Sync>,
    pub push_api_metrics_entry: Option<Box<dyn Fn(u64) + Send + Sync>>,
    pub set_stream_mode: Option<Box<dyn Fn(String) + Send + Sync>>,
    pub on_compact_progress: Option<Box<dyn Fn(CompactProgressEvent) + Send + Sync>>,
    pub set_sdk_status: Option<Box<dyn Fn(String) + Send + Sync>>,
    pub open_message_selector: Option<Box<dyn Fn() + Send + Sync>>,
    pub update_file_history_state:
        Box<dyn Fn(Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>) + Send + Sync>,
    pub update_attribution_state:
        Box<dyn Fn(Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>) + Send + Sync>,
    pub set_conversation_id: Option<Box<dyn Fn(String) + Send + Sync>>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub require_can_use_tool: bool,
    pub messages: Vec<Message>,
    pub file_reading_limits: Option<FileReadingLimits>,
    pub glob_limits: Option<GlobLimits>,
    pub tool_decisions: Option<
        Arc<std::sync::Mutex<HashMap<String, ToolDecisionEntry>>>,
    >,
    pub query_tracking: Option<QueryChainTracking>,
    pub request_prompt: Option<
        Arc<dyn Fn(String, Option<String>) -> Box<dyn Fn(serde_json::Value) -> std::pin::Pin<Box<dyn Future<Output = serde_json::Value> + Send>> + Send> + Send + Sync>,
    >,
    pub tool_use_id: Option<String>,
    pub critical_system_reminder_experimental: Option<String>,
    pub preserve_tool_use_results: bool,
    pub local_denial_tracking: Option<Arc<std::sync::Mutex<DenialTrackingState>>>,
    pub content_replacement_state: Option<Arc<dyn std::any::Any + Send + Sync>>,
    pub rendered_system_prompt: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadingLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(rename = "maxSizeBytes", skip_serializing_if = "Option::is_none")]
    pub max_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobLimits {
    #[serde(rename = "maxResults", skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecisionEntry {
    pub source: String,
    pub decision: String, // "accept" | "reject"
    pub timestamp: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DenialTrackingState {
    pub count: u32,
}

#[derive(Clone)]
pub struct ToolUseContextOptions {
    pub commands: Vec<serde_json::Value>,
    pub debug: bool,
    pub main_loop_model: String,
    pub tools: Vec<ToolDefinition>,
    pub verbose: bool,
    pub thinking_config: Option<serde_json::Value>,
    pub mcp_clients: Vec<serde_json::Value>,
    pub mcp_resources: HashMap<String, Vec<serde_json::Value>>,
    pub is_non_interactive_session: bool,
    pub agent_definitions: AgentDefinitionsResult,
    pub max_budget_usd: Option<f64>,
    pub custom_system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub query_source: Option<String>,
    #[allow(clippy::type_complexity)]
    pub refresh_tools: Option<Arc<dyn Fn() -> Vec<ToolDefinition> + Send + Sync>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinitionsResult {
    #[serde(rename = "activeAgents")]
    pub active_agents: Vec<serde_json::Value>,
    #[serde(rename = "allAgents")]
    pub all_agents: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Tool progress
// ---------------------------------------------------------------------------

/// Progress associated with a specific tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress<P = ToolProgressData> {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    pub data: P,
}

/// Filter out hook_progress messages from a list of progress messages.
///
/// Mirrors `filterToolProgressMessages` from the TypeScript SDK.
pub fn filter_tool_progress_messages(
    progress_messages: &[ProgressMessage],
) -> Vec<ProgressMessage> {
    progress_messages
        .iter()
        .filter(|msg| {
            msg.progress
                .as_ref()
                .and_then(|d| d.get("kind"))
                .and_then(|k| k.as_str())
                != Some("hook_progress")
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Tool result
// ---------------------------------------------------------------------------

/// Result returned by a tool call.
///
/// Mirrors the TypeScript `ToolResult<T>` type.
pub struct ToolResult<T = serde_json::Value> {
    pub data: T,
    pub new_messages: Option<Vec<ToolResultMessage>>,
    /// context_modifier is only honored for tools that aren't concurrency safe.
    pub context_modifier: Option<Arc<dyn Fn(&ToolUseContext) -> ToolUseContext + Send + Sync>>,
    /// MCP protocol metadata (structuredContent, _meta) to pass through to SDK consumers
    pub mcp_meta: Option<McpMeta>,
}

/// Messages that can be produced as part of a tool result.
pub enum ToolResultMessage {
    User(UserMessage),
    Assistant(AssistantMessage),
    Attachment(AttachmentMessage),
    System(SystemMessage),
}

/// MCP metadata passed through to SDK consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<HashMap<String, serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// Tool call progress callback
// ---------------------------------------------------------------------------

/// Callback type for reporting progress during tool execution.
pub type ToolCallProgressFn<P = ToolProgressData> =
    Arc<dyn Fn(ToolProgress<P>) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

// ---------------------------------------------------------------------------
// Tool matching utilities
// ---------------------------------------------------------------------------

/// Checks if a tool matches the given name (primary name or alias).
///
/// Mirrors `toolMatchesName` from the TypeScript SDK.
pub fn tool_matches_name(name: &str, aliases: Option<&[String]>, target: &str) -> bool {
    name == target || aliases.is_some_and(|a| a.iter().any(|alias| alias == target))
}

/// Finds a tool by name or alias from a list of tool definitions.
///
/// Mirrors `findToolByName` from the TypeScript SDK.
pub fn find_tool_by_name<'a>(
    tools: &'a [ToolDefinition],
    name: &str,
) -> Option<&'a ToolDefinition> {
    tools.iter().find(|t| tool_matches_name(&t.name, None, name))
}

// ---------------------------------------------------------------------------
// Tool result block param
// ---------------------------------------------------------------------------

/// Maps a tool result to the Anthropic SDK ToolResultBlockParam shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlockParam {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(rename = "tool_use_id")]
    pub tool_use_id: String,
    pub content: Vec<ContentBlockParam>,
    #[serde(rename = "is_error", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockParam {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

// ---------------------------------------------------------------------------
// The Tool trait
// ---------------------------------------------------------------------------

/// A tool that can be called by the agent.
///
/// This trait mirrors the TypeScript `Tool` type with all its optional methods.
/// Default implementations are provided for all optional methods so that tool
/// implementations only need to define the core methods.
///
/// # Required methods
/// - `name` - The tool's name
/// - `input_schema` - JSON schema for tool input
/// - `call` - Execute the tool
/// - `description` - Generate a description for the tool
/// - `prompt` - Generate the system prompt fragment for this tool
/// - `user_facing_name` - Human-readable name
/// - `to_auto_classifier_input` - Compact representation for the security classifier
/// - `map_tool_result_to_tool_result_block_param` - Convert result to SDK format
/// - `render_tool_use_message` - Render the tool use message (text representation)
///
/// # Optional methods (with defaults)
/// All other methods have sensible defaults and can be omitted.
pub trait Tool: Send + Sync {
    // ---- Core required methods ----

    /// The tool's primary name.
    fn name(&self) -> &str;

    /// Optional aliases for backwards compatibility when a tool is renamed.
    fn aliases(&self) -> Option<&[String]> {
        None
    }

    /// One-line capability phrase used by ToolSearch for keyword matching.
    /// 3-10 words, no trailing period.
    fn search_hint(&self) -> Option<&str> {
        None
    }

    /// JSON schema for tool input.
    fn input_schema(&self) -> ToolInputSchema;

    /// Optional JSON schema (used directly instead of converting from Zod).
    fn input_json_schema(&self) -> Option<ToolInputJsonSchema> {
        None
    }

    /// Output schema for the tool result.
    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Execute the tool with the given input and context.
    fn call(
        &self,
        args: serde_json::Value,
        context: Arc<ToolUseContext>,
        can_use_tool: Arc<
            dyn Fn(
                    &ToolDefinition,
                    &serde_json::Value,
                    Arc<ToolUseContext>,
                    Arc<AssistantMessage>,
                    &str,
                    bool,
                ) -> std::pin::Pin<
                    Box<dyn Future<Output = Result<PermissionDecision, crate::error::AgentError>> + Send>,
                > + Send
                + Sync,
        >,
        parent_message: Arc<AssistantMessage>,
        on_progress: Option<Arc<dyn Fn(ToolProgress) + Send + Sync>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<ToolResult, crate::error::AgentError>> + Send + '_>>;

    /// Generate a description for this tool given specific input.
    fn description(
        &self,
        input: serde_json::Value,
        is_non_interactive_session: bool,
        tool_permission_context: &ToolPermissionContext,
        tools: &[ToolDefinition],
    ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>>;

    // ---- Permission / validation ----

    /// Validates tool input before execution.
    /// Returns `ValidationResult::Valid` or `ValidationResult::Invalid`.
    fn validate_input(
        &self,
        _input: serde_json::Value,
        _context: Arc<ToolUseContext>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ValidationResult> + Send + '_>> {
        Box::pin(async { ValidationResult::Valid })
    }

    /// Determines if the user is asked for permission. Only called after
    /// `validate_input` passes.
    fn check_permissions(
        &self,
        input: serde_json::Value,
        _context: Arc<ToolUseContext>,
    ) -> std::pin::Pin<Box<dyn Future<Output = PermissionResult> + Send + '_>> {
        Box::pin(async move {
            let updated_input = input
                .as_object()
                .map(|o| o.clone().into_iter().collect::<HashMap<String, serde_json::Value>>());
            PermissionResult::Allow {
                updated_input,
                user_modified: None,
            }
        })
    }

    /// Prepare a matcher for hook `if` conditions (permission-rule patterns).
    /// Called once per hook-input pair; returns a closure called per hook pattern.
    fn prepare_permission_matcher(
        &self,
        _input: serde_json::Value,
    ) -> Option<Arc<dyn Fn(&str) -> bool + Send + Sync>> {
        None
    }

    // ---- Tool properties ----

    /// Whether the tool is currently enabled. Defaults to `true`.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Whether the tool is safe to run concurrently. Defaults to `false` (fail-closed).
    fn is_concurrency_safe(&self, _input: serde_json::Value) -> bool {
        false
    }

    /// Whether the tool is read-only. Defaults to `false` (assume writes).
    fn is_read_only(&self, _input: serde_json::Value) -> bool {
        false
    }

    /// Whether the tool performs irreversible operations (delete, overwrite, send).
    /// Defaults to `false`.
    fn is_destructive(&self, _input: serde_json::Value) -> bool {
        false
    }

    /// Whether two inputs are equivalent (for deduplication).
    fn inputs_equivalent(&self, _a: serde_json::Value, _b: serde_json::Value) -> bool {
        false
    }

    /// Maximum size in characters for tool result before it gets persisted to disk.
    fn max_result_size_chars(&self) -> usize {
        usize::MAX
    }

    /// When true, enables strict mode for this tool.
    fn strict(&self) -> bool {
        false
    }

    /// Whether this tool is deferred (sent with defer_loading: true) and requires
    /// ToolSearch to be used before it can be called.
    fn should_defer(&self) -> bool {
        false
    }

    /// When true, this tool is never deferred.
    fn always_load(&self) -> bool {
        false
    }

    /// For MCP tools: the server and tool names.
    fn mcp_info(&self) -> Option<McpToolInfo> {
        None
    }

    /// Whether this is an MCP tool.
    fn is_mcp(&self) -> bool {
        false
    }

    /// Whether this is an LSP tool.
    fn is_lsp(&self) -> bool {
        false
    }

    // ---- Interrupt behavior ----

    /// What should happen when the user submits a new message while this tool
    /// is running.
    /// - `"cancel"` - stop the tool and discard its result
    /// - `"block"` - keep running; the new message waits
    ///
    /// Defaults to `"block"` when not implemented.
    fn interrupt_behavior(&self) -> &str {
        "block"
    }

    // ---- UI rendering methods ----

    /// Returns information about whether this tool use is a search or read
    /// operation that should be collapsed into a condensed display in the UI.
    fn is_search_or_read_command(&self, _input: serde_json::Value) -> SearchOrReadInfo {
        SearchOrReadInfo {
            is_search: false,
            is_read: false,
            is_list: false,
        }
    }

    /// Whether this tool operates in an open-world context.
    fn is_open_world(&self, _input: serde_json::Value) -> bool {
        false
    }

    /// Whether this tool requires user interaction.
    fn requires_user_interaction(&self) -> bool {
        false
    }

    /// Called on copies of tool_use input before observers see it.
    /// Mutate in place to add legacy/derived fields. Must be idempotent.
    fn backfill_observable_input(&self, _input: &mut serde_json::Value) {}

    /// Get the file path this tool operates on (for tools that work on files).
    fn get_path(&self, _input: serde_json::Value) -> Option<String> {
        None
    }

    /// Human-readable name for display.
    fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        self.name().to_string()
    }

    /// Background color key for the theme.
    fn user_facing_name_background_color(
        &self,
        _input: Option<&serde_json::Value>,
    ) -> Option<String> {
        None
    }

    /// Whether this tool is a transparent wrapper (delegates rendering to progress handler).
    fn is_transparent_wrapper(&self) -> bool {
        false
    }

    /// Returns a short string summary for compact views.
    fn get_tool_use_summary(&self, _input: Option<&serde_json::Value>) -> Option<String> {
        None
    }

    /// Returns a present-tense activity description for spinner display.
    fn get_activity_description(&self, _input: Option<&serde_json::Value>) -> Option<String> {
        None
    }

    /// Compact representation for the auto-mode security classifier.
    fn to_auto_classifier_input(&self, _input: serde_json::Value) -> serde_json::Value {
        serde_json::Value::String(String::new())
    }

    /// Convert tool result to SDK ToolResultBlockParam.
    fn map_tool_result_to_tool_result_block_param(
        &self,
        content: serde_json::Value,
        tool_use_id: &str,
    ) -> ToolResultBlockParam;

    /// Render the tool result message. Optional - omitted means nothing renders
    /// (same as returning null).
    fn render_tool_result_message(
        &self,
        _content: serde_json::Value,
        _progress_messages: &[ProgressMessage],
        _options: ToolResultRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Flattened text of what render_tool_result_message shows in transcript mode.
    fn extract_search_text(&self, _out: serde_json::Value) -> String {
        String::new()
    }

    /// Render the tool use message.
    fn render_tool_use_message(
        &self,
        input: serde_json::Value,
        options: ToolUseRenderOptions,
    ) -> String;

    /// Returns true when the non-verbose rendering is truncated.
    fn is_result_truncated(&self, _output: serde_json::Value) -> bool {
        false
    }

    /// Render an optional tag after the tool use message.
    fn render_tool_use_tag(&self, _input: serde_json::Value) -> Option<String> {
        None
    }

    /// Render progress message while the tool runs.
    fn render_tool_use_progress_message(
        &self,
        _progress_messages: &[ProgressMessage],
        _options: ToolProgressRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Render queued message.
    fn render_tool_use_queued_message(&self) -> Option<String> {
        None
    }

    /// Render rejection message. Falls back to default if omitted.
    fn render_tool_use_rejected_message(
        &self,
        _input: serde_json::Value,
        _options: ToolRejectedRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Render error message. Falls back to default if omitted.
    fn render_tool_use_error_message(
        &self,
        _result: serde_json::Value,
        _options: ToolErrorRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Render multiple parallel instances as a group.
    fn render_grouped_tool_uses(
        &self,
        _tool_uses: Vec<GroupedToolUse>,
        _options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Render a single tool use within a group.
    fn render_grouped_tool_use(
        &self,
        _param: serde_json::Value,
        _is_resolved: bool,
        _is_error: bool,
        _is_in_progress: bool,
        _progress_messages: &[ProgressMessage],
        _result: Option<serde_json::Value>,
        _options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Render multiple tool uses as a group (non-verbose mode only).
    fn render_grouped_tool_use_fallback(
        &self,
        _tool_uses: Vec<GroupedToolUse>,
        _options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        None
    }

    /// Generate the system prompt fragment for this tool.
    fn prompt(
        &self,
        get_tool_permission_context: Arc<
            dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = ToolPermissionContext> + Send>>
                + Send
                + Sync,
        >,
        tools: &[ToolDefinition],
        agents: &[serde_json::Value],
        allowed_agent_types: Option<&[String]>,
    ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// Supporting types for UI rendering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SearchOrReadInfo {
    pub is_search: bool,
    pub is_read: bool,
    pub is_list: bool,
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub server_name: String,
    pub tool_name: String,
}

#[derive(Debug, Clone)]
pub struct ToolResultRenderOptions {
    pub style: Option<String>,
    pub theme: String,
    pub tools: Vec<ToolDefinition>,
    pub verbose: bool,
    pub is_transcript_mode: bool,
    pub is_brief_only: bool,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ToolUseRenderOptions {
    pub theme: String,
    pub verbose: bool,
    pub commands: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct ToolProgressRenderOptions {
    pub tools: Vec<ToolDefinition>,
    pub verbose: bool,
    pub terminal_size: Option<TerminalSize>,
    pub in_progress_tool_call_count: Option<usize>,
    pub is_transcript_mode: bool,
}

#[derive(Debug, Clone)]
pub struct TerminalSize {
    pub columns: usize,
    pub rows: usize,
}

#[derive(Debug, Clone)]
pub struct ToolRejectedRenderOptions {
    pub columns: usize,
    pub messages: Vec<Message>,
    pub style: Option<String>,
    pub theme: String,
    pub tools: Vec<ToolDefinition>,
    pub verbose: bool,
    pub progress_messages_for_message: Vec<ProgressMessage>,
    pub is_transcript_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ToolErrorRenderOptions {
    pub progress_messages_for_message: Vec<ProgressMessage>,
    pub tools: Vec<ToolDefinition>,
    pub verbose: bool,
    pub is_transcript_mode: bool,
}

#[derive(Debug, Clone)]
pub struct GroupedToolUse {
    pub param: serde_json::Value,
    pub is_resolved: bool,
    pub is_error: bool,
    pub is_in_progress: bool,
    pub progress_messages: Vec<ProgressMessage>,
    pub result: Option<GroupedToolUseResult>,
}

#[derive(Debug, Clone)]
pub struct GroupedToolUseResult {
    pub param: serde_json::Value,
    pub output: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct GroupedToolUseRenderOptions {
    pub should_animate: bool,
    pub tools: Vec<ToolDefinition>,
}

// ---------------------------------------------------------------------------
// Permission result (mirrors TypeScript PermissionResult shape used by tools)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionResult {
    Allow {
        #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
        #[serde(rename = "userModified", skip_serializing_if = "Option::is_none")]
        user_modified: Option<bool>,
    },
    Deny {
        message: String,
        #[serde(rename = "toolUseID", skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    Ask {
        message: String,
        #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
    },
    Passthrough {
        message: String,
    },
}

impl PermissionResult {
    /// Helper to create an allow result with the given input.
    pub fn allow(input: HashMap<String, serde_json::Value>) -> Self {
        PermissionResult::Allow {
            updated_input: Some(input),
            user_modified: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Permission decision (used by can_use_tool callback)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow {
        #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
        updated_input: Option<HashMap<String, serde_json::Value>>,
    },
    Deny {
        message: String,
    },
    Ask {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Tools type alias
// ---------------------------------------------------------------------------

/// A collection of tools. Use this type instead of `Vec<ToolDefinition>` to
/// make it easier to track where tool sets are assembled, passed, and filtered
/// across the codebase.
pub type Tools = Vec<ToolDefinition>;

// ---------------------------------------------------------------------------
// build_tool pattern (translated from TypeScript `buildTool`)
// ---------------------------------------------------------------------------

/// Default values for tool methods, mirroring the TypeScript `TOOL_DEFAULTS`.
///
/// Defaults (fail-closed where it matters):
/// - `is_enabled` -> `true`
/// - `is_concurrency_safe` -> `false` (assume not safe)
/// - `is_read_only` -> `false` (assume writes)
/// - `is_destructive` -> `false`
/// - `check_permissions` -> `{ behavior: "allow", updatedInput }` (defer to general permission system)
/// - `to_auto_classifier_input` -> `""` (skip classifier - security-relevant tools must override)
/// - `user_facing_name` -> tool's name
pub struct ToolDefaults {
    pub is_enabled: bool,
    pub is_concurrency_safe: bool,
    pub is_read_only: bool,
    pub is_destructive: bool,
    pub check_permissions:
        Arc<dyn Fn(serde_json::Value) -> std::pin::Pin<Box<dyn Future<Output = PermissionResult> + Send>> + Send + Sync>,
    pub to_auto_classifier_input:
        Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>,
    pub user_facing_name: Arc<dyn Fn() -> String + Send + Sync>,
}

impl Default for ToolDefaults {
    fn default() -> Self {
        ToolDefaults {
            is_enabled: true,
            is_concurrency_safe: false,
            is_read_only: false,
            is_destructive: false,
            check_permissions: Arc::new(|input: serde_json::Value| {
                Box::pin(async move {
                    PermissionResult::Allow {
                        updated_input: Some(
                            input
                                .as_object()
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .collect(),
                        ),
                        user_modified: None,
                    }
                })
            }),
            to_auto_classifier_input: Arc::new(|_input: serde_json::Value| {
                serde_json::Value::String(String::new())
            }),
            user_facing_name: Arc::new(|| String::new()),
        }
    }
}

/// Builder for constructing a `Tool` implementation with default values filled in.
///
/// This mirrors the TypeScript `buildTool` function. Tool implementations should
/// use this builder so that defaults live in one place and callers never need to
/// handle missing methods.
///
/// # Example
/// ```ignore
/// let tool = ToolBuilder::new("my_tool")
///     .input_schema(ToolInputSchema { ... })
///     .description_fn(|input, _, _, _| {
///         Box::pin(async move { format!("My tool with input: {:?}", input) })
///     })
///     .call_fn(|args, ctx, can_use, parent, on_progress| {
///         Box::pin(async move {
///             Ok(ToolResult {
///                 data: serde_json::json!({ "result": "ok" }),
///                 new_messages: None,
///                 context_modifier: None,
///                 mcp_meta: None,
///             })
///         })
///     })
///     .is_read_only(true)
///     .build();
/// ```
pub struct ToolBuilder {
    name: String,
    aliases: Option<Vec<String>>,
    search_hint: Option<String>,
    input_schema: Option<ToolInputSchema>,
    input_json_schema: Option<ToolInputJsonSchema>,
    output_schema: Option<serde_json::Value>,
    call_fn: Option<
        Arc<
            dyn Fn(
                    serde_json::Value,
                    Arc<ToolUseContext>,
                    Arc<
                        dyn Fn(
                                &ToolDefinition,
                                &serde_json::Value,
                                Arc<ToolUseContext>,
                                Arc<AssistantMessage>,
                                &str,
                                bool,
                            ) -> std::pin::Pin<
                                Box<dyn Future<Output = Result<PermissionDecision, crate::error::AgentError>> + Send>,
                            > + Send
                            + Sync,
                    >,
                    Arc<AssistantMessage>,
                    Option<Arc<dyn Fn(ToolProgress) + Send + Sync>>,
                ) -> std::pin::Pin<
                    Box<dyn Future<Output = Result<ToolResult, crate::error::AgentError>> + Send>,
                > + Send
                + Sync,
        >,
    >,
    description_fn: Option<
        Arc<
            dyn Fn(
                    serde_json::Value,
                    bool,
                    &ToolPermissionContext,
                    &[ToolDefinition],
                ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send>>
                + Send
                + Sync,
        >,
    >,
    prompt_fn: Option<
        Arc<
            dyn Fn(
                    Arc<
                        dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = ToolPermissionContext> + Send>>
                            + Send
                            + Sync,
                    >,
                    &[ToolDefinition],
                    &[serde_json::Value],
                    Option<&[String]>,
                ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send>>
                + Send
                + Sync,
        >,
    >,
    validate_input_fn: Option<
        Arc<
            dyn Fn(serde_json::Value, Arc<ToolUseContext>)
                    -> std::pin::Pin<Box<dyn Future<Output = ValidationResult> + Send>>
                + Send
                + Sync,
        >,
    >,
    check_permissions_fn: Option<
        Arc<
            dyn Fn(serde_json::Value, Arc<ToolUseContext>)
                    -> std::pin::Pin<Box<dyn Future<Output = PermissionResult> + Send>>
                + Send
                + Sync,
        >,
    >,
    prepare_permission_matcher_fn: Option<
        Arc<dyn Fn(serde_json::Value) -> Arc<dyn Fn(&str) -> bool + Send + Sync> + Send + Sync>,
    >,
    is_enabled: bool,
    is_concurrency_safe_fn: Option<Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>>,
    is_read_only_fn: Option<Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>>,
    is_destructive_fn: Option<Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>>,
    inputs_equivalent_fn:
        Option<Arc<dyn Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync>>,
    max_result_size_chars: usize,
    strict: bool,
    should_defer: bool,
    always_load: bool,
    mcp_info: Option<McpToolInfo>,
    is_mcp: bool,
    is_lsp: bool,
    interrupt_behavior: String,
    is_search_or_read_fn:
        Option<Arc<dyn Fn(serde_json::Value) -> SearchOrReadInfo + Send + Sync>>,
    is_open_world_fn: Option<Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>>,
    requires_user_interaction: bool,
    backfill_observable_input_fn: Option<Arc<dyn Fn(&mut serde_json::Value) + Send + Sync>>,
    get_path_fn: Option<Arc<dyn Fn(serde_json::Value) -> Option<String> + Send + Sync>>,
    user_facing_name_fn:
        Option<Arc<dyn Fn(Option<&serde_json::Value>) -> String + Send + Sync>>,
    user_facing_name_background_color_fn:
        Option<Arc<dyn Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync>>,
    is_transparent_wrapper: bool,
    get_tool_use_summary_fn:
        Option<Arc<dyn Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync>>,
    get_activity_description_fn:
        Option<Arc<dyn Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync>>,
    to_auto_classifier_input_fn:
        Option<Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>>,
    map_tool_result_fn: Option<
        Arc<
            dyn Fn(serde_json::Value, &str) -> ToolResultBlockParam
                + Send
                + Sync,
        >,
    >,
    render_tool_result_message_fn: Option<
        Arc<
            dyn Fn(
                    serde_json::Value,
                    &[ProgressMessage],
                    ToolResultRenderOptions,
                ) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    extract_search_text_fn: Option<Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>>,
    render_tool_use_message_fn: Option<
        Arc<dyn Fn(serde_json::Value, ToolUseRenderOptions) -> String + Send + Sync>,
    >,
    is_result_truncated_fn: Option<Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>>,
    render_tool_use_tag_fn:
        Option<Arc<dyn Fn(serde_json::Value) -> Option<String> + Send + Sync>>,
    render_tool_use_progress_message_fn: Option<
        Arc<
            dyn Fn(&[ProgressMessage], ToolProgressRenderOptions) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    render_tool_use_queued_message_fn: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,
    render_tool_use_rejected_message_fn: Option<
        Arc<
            dyn Fn(serde_json::Value, ToolRejectedRenderOptions) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    render_tool_use_error_message_fn: Option<
        Arc<
            dyn Fn(serde_json::Value, ToolErrorRenderOptions) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    render_grouped_tool_uses_fn: Option<
        Arc<
            dyn Fn(Vec<GroupedToolUse>, GroupedToolUseRenderOptions) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    render_grouped_tool_use_fn: Option<
        Arc<
            dyn Fn(
                    serde_json::Value,
                    bool,
                    bool,
                    bool,
                    &[ProgressMessage],
                    Option<serde_json::Value>,
                    GroupedToolUseRenderOptions,
                ) -> Option<String>
                + Send
                + Sync,
        >,
    >,
    render_grouped_tool_use_fallback_fn: Option<
        Arc<
            dyn Fn(Vec<GroupedToolUse>, GroupedToolUseRenderOptions) -> Option<String>
                + Send
                + Sync,
        >,
    >,
}

impl ToolBuilder {
    /// Create a new builder with the tool name (required).
    pub fn new(name: &str) -> Self {
        ToolBuilder {
            name: name.to_string(),
            aliases: None,
            search_hint: None,
            input_schema: None,
            input_json_schema: None,
            output_schema: None,
            call_fn: None,
            description_fn: None,
            prompt_fn: None,
            validate_input_fn: None,
            check_permissions_fn: None,
            prepare_permission_matcher_fn: None,
            is_enabled: true,
            is_concurrency_safe_fn: None,
            is_read_only_fn: None,
            is_destructive_fn: None,
            inputs_equivalent_fn: None,
            max_result_size_chars: usize::MAX,
            strict: false,
            should_defer: false,
            always_load: false,
            mcp_info: None,
            is_mcp: false,
            is_lsp: false,
            interrupt_behavior: "block".to_string(),
            is_search_or_read_fn: None,
            is_open_world_fn: None,
            requires_user_interaction: false,
            backfill_observable_input_fn: None,
            get_path_fn: None,
            user_facing_name_fn: None,
            user_facing_name_background_color_fn: None,
            is_transparent_wrapper: false,
            get_tool_use_summary_fn: None,
            get_activity_description_fn: None,
            to_auto_classifier_input_fn: None,
            map_tool_result_fn: None,
            render_tool_result_message_fn: None,
            extract_search_text_fn: None,
            render_tool_use_message_fn: None,
            is_result_truncated_fn: None,
            render_tool_use_tag_fn: None,
            render_tool_use_progress_message_fn: None,
            render_tool_use_queued_message_fn: None,
            render_tool_use_rejected_message_fn: None,
            render_tool_use_error_message_fn: None,
            render_grouped_tool_uses_fn: None,
            render_grouped_tool_use_fn: None,
            render_grouped_tool_use_fallback_fn: None,
        }
    }

    pub fn aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = Some(aliases);
        self
    }

    pub fn search_hint(mut self, hint: &str) -> Self {
        self.search_hint = Some(hint.to_string());
        self
    }

    pub fn input_schema(mut self, schema: ToolInputSchema) -> Self {
        self.input_schema = Some(schema);
        self
    }

    pub fn input_json_schema(mut self, schema: ToolInputJsonSchema) -> Self {
        self.input_json_schema = Some(schema);
        self
    }

    pub fn output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    pub fn call_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(
                serde_json::Value,
                Arc<ToolUseContext>,
                Arc<
                    dyn Fn(
                            &ToolDefinition,
                            &serde_json::Value,
                            Arc<ToolUseContext>,
                            Arc<AssistantMessage>,
                            &str,
                            bool,
                        ) -> std::pin::Pin<
                            Box<dyn Future<Output = Result<PermissionDecision, crate::error::AgentError>> + Send>,
                        > + Send
                        + Sync,
                >,
                Arc<AssistantMessage>,
                Option<Arc<dyn Fn(ToolProgress) + Send + Sync>>,
            ) -> std::pin::Pin<Box<dyn Future<Output = Result<ToolResult, crate::error::AgentError>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.call_fn = Some(Arc::new(f));
        self
    }

    pub fn description_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(
                serde_json::Value,
                bool,
                &ToolPermissionContext,
                &[ToolDefinition],
            ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.description_fn = Some(Arc::new(f));
        self
    }

    pub fn prompt_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(
                Arc<
                    dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = ToolPermissionContext> + Send>>
                        + Send
                        + Sync,
                >,
                &[ToolDefinition],
                &[serde_json::Value],
                Option<&[String]>,
            ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.prompt_fn = Some(Arc::new(f));
        self
    }

    pub fn validate_input_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, Arc<ToolUseContext>)
                -> std::pin::Pin<Box<dyn Future<Output = ValidationResult> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.validate_input_fn = Some(Arc::new(f));
        self
    }

    pub fn check_permissions_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, Arc<ToolUseContext>)
                -> std::pin::Pin<Box<dyn Future<Output = PermissionResult> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.check_permissions_fn = Some(Arc::new(f));
        self
    }

    pub fn prepare_permission_matcher_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> Arc<dyn Fn(&str) -> bool + Send + Sync> + Send + Sync + 'static,
    {
        self.prepare_permission_matcher_fn = Some(Arc::new(f));
        self
    }

    pub fn is_enabled(mut self, enabled: bool) -> Self {
        self.is_enabled = enabled;
        self
    }

    pub fn is_concurrency_safe_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.is_concurrency_safe_fn = Some(Arc::new(f));
        self
    }

    pub fn is_read_only_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.is_read_only_fn = Some(Arc::new(f));
        self
    }

    pub fn is_destructive_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.is_destructive_fn = Some(Arc::new(f));
        self
    }

    pub fn inputs_equivalent_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.inputs_equivalent_fn = Some(Arc::new(f));
        self
    }

    pub fn max_result_size_chars(mut self, size: usize) -> Self {
        self.max_result_size_chars = size;
        self
    }

    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    pub fn should_defer(mut self, defer: bool) -> Self {
        self.should_defer = defer;
        self
    }

    pub fn always_load(mut self, always: bool) -> Self {
        self.always_load = always;
        self
    }

    pub fn mcp_info(mut self, server_name: &str, tool_name: &str) -> Self {
        self.mcp_info = Some(McpToolInfo {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
        });
        self
    }

    pub fn is_mcp(mut self, is_mcp: bool) -> Self {
        self.is_mcp = is_mcp;
        self
    }

    pub fn is_lsp(mut self, is_lsp: bool) -> Self {
        self.is_lsp = is_lsp;
        self
    }

    pub fn interrupt_behavior(mut self, behavior: &str) -> Self {
        self.interrupt_behavior = behavior.to_string();
        self
    }

    pub fn is_search_or_read_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> SearchOrReadInfo + Send + Sync + 'static,
    {
        self.is_search_or_read_fn = Some(Arc::new(f));
        self
    }

    pub fn is_open_world_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.is_open_world_fn = Some(Arc::new(f));
        self
    }

    pub fn requires_user_interaction(mut self, requires: bool) -> Self {
        self.requires_user_interaction = requires;
        self
    }

    pub fn backfill_observable_input_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut serde_json::Value) + Send + Sync + 'static,
    {
        self.backfill_observable_input_fn = Some(Arc::new(f));
        self
    }

    pub fn get_path_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> Option<String> + Send + Sync + 'static,
    {
        self.get_path_fn = Some(Arc::new(f));
        self
    }

    pub fn user_facing_name_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Option<&serde_json::Value>) -> String + Send + Sync + 'static,
    {
        self.user_facing_name_fn = Some(Arc::new(f));
        self
    }

    pub fn user_facing_name_background_color_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync + 'static,
    {
        self.user_facing_name_background_color_fn = Some(Arc::new(f));
        self
    }

    pub fn is_transparent_wrapper(mut self, is_transparent: bool) -> Self {
        self.is_transparent_wrapper = is_transparent;
        self
    }

    pub fn get_tool_use_summary_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync + 'static,
    {
        self.get_tool_use_summary_fn = Some(Arc::new(f));
        self
    }

    pub fn get_activity_description_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync + 'static,
    {
        self.get_activity_description_fn = Some(Arc::new(f));
        self
    }

    pub fn to_auto_classifier_input_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
    {
        self.to_auto_classifier_input_fn = Some(Arc::new(f));
        self
    }

    pub fn map_tool_result_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, &str) -> ToolResultBlockParam + Send + Sync + 'static,
    {
        self.map_tool_result_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_result_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, &[ProgressMessage], ToolResultRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_tool_result_message_fn = Some(Arc::new(f));
        self
    }

    pub fn extract_search_text_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> String + Send + Sync + 'static,
    {
        self.extract_search_text_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, ToolUseRenderOptions) -> String + Send + Sync + 'static,
    {
        self.render_tool_use_message_fn = Some(Arc::new(f));
        self
    }

    pub fn is_result_truncated_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> bool + Send + Sync + 'static,
    {
        self.is_result_truncated_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_tag_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> Option<String> + Send + Sync + 'static,
    {
        self.render_tool_use_tag_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_progress_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(&[ProgressMessage], ToolProgressRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_tool_use_progress_message_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_queued_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Option<String> + Send + Sync + 'static,
    {
        self.render_tool_use_queued_message_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_rejected_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, ToolRejectedRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_tool_use_rejected_message_fn = Some(Arc::new(f));
        self
    }

    pub fn render_tool_use_error_message_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value, ToolErrorRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_tool_use_error_message_fn = Some(Arc::new(f));
        self
    }

    pub fn render_grouped_tool_uses_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Vec<GroupedToolUse>, GroupedToolUseRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_grouped_tool_uses_fn = Some(Arc::new(f));
        self
    }

    pub fn render_grouped_tool_use_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(
                serde_json::Value,
                bool,
                bool,
                bool,
                &[ProgressMessage],
                Option<serde_json::Value>,
                GroupedToolUseRenderOptions,
            ) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_grouped_tool_use_fn = Some(Arc::new(f));
        self
    }

    pub fn render_grouped_tool_use_fallback_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(Vec<GroupedToolUse>, GroupedToolUseRenderOptions) -> Option<String>
            + Send
            + Sync
            + 'static,
    {
        self.render_grouped_tool_use_fallback_fn = Some(Arc::new(f));
        self
    }

    /// Build the tool, applying defaults for any unset fields.
    ///
    /// This mirrors the TypeScript runtime behavior:
    /// `{ ...TOOL_DEFAULTS, userFacingName: () => def.name, ...def }`
    pub fn build(self) -> Box<dyn Tool> {
        let name = self.name.clone();
        Box::new(BuiltTool { inner: self })
    }
}

/// A fully constructed tool with all defaults applied.
/// Returned by `ToolBuilder::build()`.
struct BuiltTool {
    inner: ToolBuilder,
}

impl Tool for BuiltTool {
    fn name(&self) -> &str {
        &self.inner.name
    }

    fn aliases(&self) -> Option<&[String]> {
        self.inner.aliases.as_deref()
    }

    fn search_hint(&self) -> Option<&str> {
        self.inner.search_hint.as_deref()
    }

    fn input_schema(&self) -> ToolInputSchema {
        self.inner.input_schema.clone().unwrap_or_else(|| {
            ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
            }
        })
    }

    fn input_json_schema(&self) -> Option<ToolInputJsonSchema> {
        self.inner.input_json_schema.clone()
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        self.inner.output_schema.clone()
    }

    fn call(
        &self,
        args: serde_json::Value,
        context: Arc<ToolUseContext>,
        can_use_tool: Arc<
            dyn Fn(
                    &ToolDefinition,
                    &serde_json::Value,
                    Arc<ToolUseContext>,
                    Arc<AssistantMessage>,
                    &str,
                    bool,
                ) -> std::pin::Pin<
                    Box<dyn Future<Output = Result<PermissionDecision, crate::error::AgentError>> + Send>,
                > + Send
                + Sync,
        >,
        parent_message: Arc<AssistantMessage>,
        on_progress: Option<Arc<dyn Fn(ToolProgress) + Send + Sync>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<ToolResult, crate::error::AgentError>> + Send + '_>> {
        if let Some(f) = &self.inner.call_fn {
            let f = Arc::clone(f);
            let can_use_tool = Arc::clone(&can_use_tool);
            let context = Arc::clone(&context);
            let parent_message = Arc::clone(&parent_message);
            let on_progress = on_progress.clone();
            Box::pin(async move {
                f(args, context, can_use_tool, parent_message, on_progress).await
            })
        } else {
            Box::pin(async {
                Err(crate::error::AgentError::Tool(
                    format!("Tool '{}' has no call implementation", self.inner.name),
                ))
            })
        }
    }

    fn description(
        &self,
        input: serde_json::Value,
        is_non_interactive_session: bool,
        tool_permission_context: &ToolPermissionContext,
        tools: &[ToolDefinition],
    ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>> {
        if let Some(f) = &self.inner.description_fn {
            let f = Arc::clone(f);
            let tools = tools.to_vec();
            let tpc = tool_permission_context.clone();
            Box::pin(async move {
                f(input, is_non_interactive_session, &tpc, &tools).await
            })
        } else {
            Box::pin(async move {
                format!("Tool: {}", self.inner.name)
            })
        }
    }

    fn validate_input(
        &self,
        input: serde_json::Value,
        context: Arc<ToolUseContext>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ValidationResult> + Send + '_>> {
        if let Some(f) = &self.inner.validate_input_fn {
            let f = Arc::clone(f);
            let context = Arc::clone(&context);
            Box::pin(async move { f(input, context).await })
        } else {
            Box::pin(async { ValidationResult::Valid })
        }
    }

    fn check_permissions(
        &self,
        input: serde_json::Value,
        context: Arc<ToolUseContext>,
    ) -> std::pin::Pin<Box<dyn Future<Output = PermissionResult> + Send + '_>> {
        if let Some(f) = &self.inner.check_permissions_fn {
            let f = Arc::clone(f);
            let context = Arc::clone(&context);
            Box::pin(async move { f(input, context).await })
        } else {
            Box::pin(async move {
                PermissionResult::Allow {
                    updated_input: input.as_object().map(|o| o.clone().into_iter().collect()),
                    user_modified: None,
                }
            })
        }
    }

    fn prepare_permission_matcher(
        &self,
        input: serde_json::Value,
    ) -> Option<Arc<dyn Fn(&str) -> bool + Send + Sync>> {
        self.inner
            .prepare_permission_matcher_fn
            .as_ref()
            .map(|f| f(input))
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled
    }

    fn is_concurrency_safe(&self, input: serde_json::Value) -> bool {
        self.inner
            .is_concurrency_safe_fn
            .as_ref()
            .map_or(false, |f| f(input))
    }

    fn is_read_only(&self, input: serde_json::Value) -> bool {
        self.inner
            .is_read_only_fn
            .as_ref()
            .map_or(false, |f| f(input))
    }

    fn is_destructive(&self, input: serde_json::Value) -> bool {
        self.inner
            .is_destructive_fn
            .as_ref()
            .map_or(false, |f| f(input))
    }

    fn inputs_equivalent(&self, a: serde_json::Value, b: serde_json::Value) -> bool {
        self.inner
            .inputs_equivalent_fn
            .as_ref()
            .map_or(false, |f| f(a, b))
    }

    fn max_result_size_chars(&self) -> usize {
        self.inner.max_result_size_chars
    }

    fn strict(&self) -> bool {
        self.inner.strict
    }

    fn should_defer(&self) -> bool {
        self.inner.should_defer
    }

    fn always_load(&self) -> bool {
        self.inner.always_load
    }

    fn mcp_info(&self) -> Option<McpToolInfo> {
        self.inner.mcp_info.clone()
    }

    fn is_mcp(&self) -> bool {
        self.inner.is_mcp
    }

    fn is_lsp(&self) -> bool {
        self.inner.is_lsp
    }

    fn interrupt_behavior(&self) -> &str {
        &self.inner.interrupt_behavior
    }

    fn is_search_or_read_command(&self, input: serde_json::Value) -> SearchOrReadInfo {
        self.inner
            .is_search_or_read_fn
            .as_ref()
            .map_or(SearchOrReadInfo {
                is_search: false,
                is_read: false,
                is_list: false,
            }, |f| f(input))
    }

    fn is_open_world(&self, input: serde_json::Value) -> bool {
        self.inner
            .is_open_world_fn
            .as_ref()
            .map_or(false, |f| f(input))
    }

    fn requires_user_interaction(&self) -> bool {
        self.inner.requires_user_interaction
    }

    fn backfill_observable_input(&self, input: &mut serde_json::Value) {
        if let Some(f) = &self.inner.backfill_observable_input_fn {
            f(input);
        }
    }

    fn get_path(&self, input: serde_json::Value) -> Option<String> {
        self.inner.get_path_fn.as_ref().and_then(|f| f(input))
    }

    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        self.inner
            .user_facing_name_fn
            .as_ref()
            .map(|f| f(input))
            .unwrap_or_else(|| self.inner.name.clone())
    }

    fn user_facing_name_background_color(&self, input: Option<&serde_json::Value>) -> Option<String> {
        self.inner
            .user_facing_name_background_color_fn
            .as_ref()
            .and_then(|f| f(input))
    }

    fn is_transparent_wrapper(&self) -> bool {
        self.inner.is_transparent_wrapper
    }

    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        self.inner
            .get_tool_use_summary_fn
            .as_ref()
            .and_then(|f| f(input))
    }

    fn get_activity_description(&self, input: Option<&serde_json::Value>) -> Option<String> {
        self.inner
            .get_activity_description_fn
            .as_ref()
            .and_then(|f| f(input))
    }

    fn to_auto_classifier_input(&self, input: serde_json::Value) -> serde_json::Value {
        self.inner
            .to_auto_classifier_input_fn
            .as_ref()
            .map_or(serde_json::Value::String(String::new()), |f| f(input))
    }

    fn map_tool_result_to_tool_result_block_param(
        &self,
        content: serde_json::Value,
        tool_use_id: &str,
    ) -> ToolResultBlockParam {
        if let Some(f) = &self.inner.map_tool_result_fn {
            f(content, tool_use_id)
        } else {
            ToolResultBlockParam {
                block_type: "tool_result".to_string(),
                tool_use_id: tool_use_id.to_string(),
                content: vec![ContentBlockParam::Text {
                    text: content.to_string(),
                }],
                is_error: None,
            }
        }
    }

    fn render_tool_result_message(
        &self,
        content: serde_json::Value,
        progress_messages: &[ProgressMessage],
        options: ToolResultRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_tool_result_message_fn
            .as_ref()
            .and_then(|f| f(content, progress_messages, options))
    }

    fn extract_search_text(&self, out: serde_json::Value) -> String {
        self.inner
            .extract_search_text_fn
            .as_ref()
            .map_or(String::new(), |f| f(out))
    }

    fn render_tool_use_message(
        &self,
        input: serde_json::Value,
        options: ToolUseRenderOptions,
    ) -> String {
        if let Some(f) = &self.inner.render_tool_use_message_fn {
            f(input, options)
        } else {
            format!("[Tool: {}]", self.inner.name)
        }
    }

    fn is_result_truncated(&self, output: serde_json::Value) -> bool {
        self.inner
            .is_result_truncated_fn
            .as_ref()
            .map_or(false, |f| f(output))
    }

    fn render_tool_use_tag(&self, input: serde_json::Value) -> Option<String> {
        self.inner
            .render_tool_use_tag_fn
            .as_ref()
            .and_then(|f| f(input))
    }

    fn render_tool_use_progress_message(
        &self,
        progress_messages: &[ProgressMessage],
        options: ToolProgressRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_tool_use_progress_message_fn
            .as_ref()
            .and_then(|f| f(progress_messages, options))
    }

    fn render_tool_use_queued_message(&self) -> Option<String> {
        self.inner
            .render_tool_use_queued_message_fn
            .as_ref()
            .and_then(|f| f())
    }

    fn render_tool_use_rejected_message(
        &self,
        input: serde_json::Value,
        options: ToolRejectedRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_tool_use_rejected_message_fn
            .as_ref()
            .and_then(|f| f(input, options))
    }

    fn render_tool_use_error_message(
        &self,
        result: serde_json::Value,
        options: ToolErrorRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_tool_use_error_message_fn
            .as_ref()
            .and_then(|f| f(result, options))
    }

    fn render_grouped_tool_uses(
        &self,
        tool_uses: Vec<GroupedToolUse>,
        options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_grouped_tool_uses_fn
            .as_ref()
            .and_then(|f| f(tool_uses, options))
    }

    fn render_grouped_tool_use(
        &self,
        param: serde_json::Value,
        is_resolved: bool,
        is_error: bool,
        is_in_progress: bool,
        progress_messages: &[ProgressMessage],
        result: Option<serde_json::Value>,
        options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_grouped_tool_use_fn
            .as_ref()
            .and_then(|f| f(param, is_resolved, is_error, is_in_progress, progress_messages, result, options))
    }

    fn render_grouped_tool_use_fallback(
        &self,
        tool_uses: Vec<GroupedToolUse>,
        options: GroupedToolUseRenderOptions,
    ) -> Option<String> {
        self.inner
            .render_grouped_tool_use_fallback_fn
            .as_ref()
            .and_then(|f| f(tool_uses, options))
    }

    fn prompt(
        &self,
        get_tool_permission_context: Arc<
            dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = ToolPermissionContext> + Send>>
                + Send
                + Sync,
        >,
        tools: &[ToolDefinition],
        agents: &[serde_json::Value],
        allowed_agent_types: Option<&[String]>,
    ) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>> {
        if let Some(f) = &self.inner.prompt_fn {
            let f = Arc::clone(f);
            let tools = tools.to_vec();
            let agents = agents.to_vec();
            let allowed_agent_types = allowed_agent_types.map(|s| s.to_vec());
            Box::pin(async move {
                f(get_tool_permission_context, &tools, &agents, allowed_agent_types.as_deref()).await
            })
        } else {
            Box::pin(async move {
                format!("Use the {} tool.", self.inner.name)
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

