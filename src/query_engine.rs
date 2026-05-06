// Source: ~/claudecode/openclaudecode/src/QueryEngine.ts (query lifecycle + submitMessage)
// Also translates: ~/claudecode/openclaudecode/src/query.ts (streaming, SSE, tool execution loop)
// Note: The TypeScript QueryEngine delegates to query() for the actual API call loop.
// This Rust port combines both into a single QueryEngine struct.
#![allow(dead_code)]

use crate::compact::{
    self, get_auto_compact_threshold, get_compact_prompt, get_effective_context_window_size,
};
use crate::error::AgentError;
use crate::hooks::{HookInput, HookRegistry};
use crate::services::compact::microcompact::truncate_tool_result_content;
use crate::services::api::errors::{sanitize_html_error, error_to_api_message, get_error_message_if_refusal, is_media_size_error};
use crate::services::streaming::{
    STALL_THRESHOLD_MS, StallStats, StreamWatchdog, StreamingResult, StreamingToolExecutor,
    calculate_streaming_cost, cleanup_stream, get_nonstreaming_fallback_timeout_ms,
    is_404_stream_creation_error, is_429_only_error, is_529_error, is_api_timeout_error,
    is_auth_error, is_nonstreaming_fallback_disabled, is_stale_connection_error,
    is_user_abort_error, parse_max_tokens_context_overflow, release_stream_resources,
    validate_stream_completion, FallbackTriggeredError, MAX_529_RETRIES, FLOOR_OUTPUT_TOKENS,
};
use crate::tool::Tool as ToolTrait;
use crate::tool::{ProgressMessage, ToolResultRenderOptions};
use crate::tools::orchestration::{self, ToolMessageUpdate};
use crate::types::*;
use crate::utils::http::get_user_agent;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::sleep as sleep_tokio;

/// Emit an ApiRetry event to notify callers of retry progress.
/// Matches TypeScript's createSystemAPIErrorMessage → api_retry subtype.
fn emit_api_retry_event(
    on_event: Option<&(dyn Fn(AgentEvent) + Send + Sync)>,
    attempt: u32,
    max_retries: u32,
    retry_delay_ms: u64,
    error_status: Option<u16>,
    error: &str,
) {
    if let Some(cb) = on_event {
        cb(AgentEvent::ApiRetry {
            attempt,
            max_retries,
            retry_delay_ms,
            error_status,
            error: error.to_string(),
        });
    }
}

/// Emit a Done event with a pre-result session storage flush.
/// Matches TypeScript's flushSessionStorage() before each result yield in QueryEngine.ts.
fn emit_done_event(
    on_event: &Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    result: QueryResult,
) {
    let _ = crate::utils::session_storage::flush_session_storage();
    if let Some(cb) = on_event {
        cb(AgentEvent::Done { result });
    }
}

/// Format token count for human-readable display (e.g., "120.3k", "1.2m")
fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Return an empty JSON object value to use as default for tool call arguments
pub(crate) fn empty_json_value() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Strip thinking tags from content (remove "<think>" and "</think>" blocks)
/// Matches TypeScript's thinking removal logic
pub(crate) fn strip_thinking(content: &str) -> String {
    // Find and remove thinking blocks while preserving content between them
    // This handles UTF-8 correctly because we use string operations
    let mut result = String::new();
    let mut in_thinking = false;
    let mut i = 0;

    while i < content.len() {
        // Check for thinking start - must be at a valid char boundary
        if content[i..].starts_with("<think>") {
            in_thinking = true;
            i += "<think>".len();
        } else if content[i..].starts_with("</think>") {
            in_thinking = false;
            i += "</think>".len();
        } else if !in_thinking {
            // We're outside thinking block, add the character
            // Use char indices to avoid boundary issues
            if let Some(ch) = content[i..].chars().next() {
                result.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        } else {
            // We're inside thinking block, skip
            // Move to next character boundary
            if let Some(ch) = content[i..].chars().next() {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    result.trim().to_string()
}

/// Parse Anthropic API usage info
fn parse_anthropic_usage(usage: &serde_json::Value) -> TokenUsage {
    let iterations = usage.get("iterations").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|it| {
            Some(IterationUsage {
                input_tokens: it.get("input_tokens").and_then(|v| v.as_u64())?,
                output_tokens: it.get("output_tokens").and_then(|v| v.as_u64())?,
            })
        }).collect()
    });
    TokenUsage {
        input_tokens: usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64()),
        cache_read_input_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64()),
        iterations,
    }
}

/// Tracks auto-compaction state across iterations
#[derive(Debug, Clone, Default)]
pub struct AutoCompactTracking {
    /// Whether a compaction happened in the previous turn
    pub compacted: bool,
    /// Unique ID per turn (for analytics)
    pub turn_id: String,
    /// Counter for turns since previous compact
    pub turn_counter: u32,
    /// Consecutive auto-compact failure count (circuit breaker)
    pub consecutive_failures: u32,
}

/// Rendered metadata for a tool execution, computed from Tool trait methods
#[derive(Debug, Clone)]
pub struct ToolRenderMetadata {
    pub user_facing_name: String,
    pub tool_use_summary: Option<String>,
    pub activity_description: Option<String>,
}

/// Render function closures stored alongside a tool for display hooks
type UserFacingNameFn = Arc<dyn Fn(Option<&serde_json::Value>) -> String + Send + Sync>;
type GetToolUseSummaryFn = Arc<dyn Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync>;
type GetActivityDescriptionFn =
    Arc<dyn Fn(Option<&serde_json::Value>) -> Option<String> + Send + Sync>;
type RenderToolResultFn = Arc<
    dyn Fn(&serde_json::Value, &[ProgressMessage], &ToolResultRenderOptions) -> Option<String>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct ToolRenderFns {
    pub user_facing_name: UserFacingNameFn,
    pub get_tool_use_summary: Option<GetToolUseSummaryFn>,
    pub get_activity_description: Option<GetActivityDescriptionFn>,
    pub render_tool_result_message: Option<RenderToolResultFn>,
}

impl ToolRenderFns {
    /// Render a tool's result using the stored render closure.
    /// The caller provides the tools vector for the ToolResultRenderOptions.
    pub fn render(&self, content: &str, tools: &[crate::types::ToolDefinition]) -> Option<String> {
        let content_value: serde_json::Value = serde_json::from_str(content).ok()?;
        let progress_messages: Vec<ProgressMessage> = vec![];
        let options = ToolResultRenderOptions {
            style: None,
            theme: "dark".to_string(),
            tools: tools.to_vec(),
            verbose: false,
            is_transcript_mode: false,
            is_brief_only: false,
            input: None,
        };
        let render_fn = self.render_tool_result_message.as_ref()?;
        render_fn(&content_value, &progress_messages, &options)
    }
}

#[allow(dead_code)]
pub struct QueryEngine {
    pub(crate) config: QueryEngineConfig,
    pub(crate) messages: Vec<crate::types::Message>,
    turn_count: u32,
    total_usage: TokenUsage,
    total_cost: f64,
    http_client: reqwest::Client,
    /// Tool executors: name -> async function
    tool_executors: Mutex<HashMap<String, Arc<ToolExecutor>>>,
    /// Tool render metadata: name -> closures for computing display info and rendering results
    tool_render_fns: Mutex<HashMap<String, ToolRenderFns>>,
    /// Tool backfill functions: name -> function that mutates input for observers
    tool_backfill_fns: Mutex<HashMap<String, Arc<dyn Fn(&mut serde_json::Value) + Send + Sync>>>,
    /// Hook registry for PreToolUse/PostToolUse hooks
    hook_registry: Arc<Mutex<Option<HookRegistry>>>,
    /// Auto-compaction tracking state
    auto_compact_tracking: AutoCompactTracking,
    /// Track permission denials for SDK reporting (matches TypeScript)
    permission_denials: Vec<PermissionDenial>,
    /// Last stop_reason from assistant messages
    last_stop_reason: Option<String>,
    /// Recovery state for max_output_tokens
    max_output_tokens_recovery_count: u32,
    /// Recovery state for reactive compaction
    has_attempted_reactive_compact: bool,
    /// Count of consecutive empty response retries (for transient API failures)
    empty_response_retries: u32,
    /// Override for max_tokens during recovery
    max_output_tokens_override: Option<u32>,
    /// Whether a stop hook is currently active (prevents re-triggering)
    stop_hook_active: bool,
    /// Transition reason - why the previous iteration continued (for testing/analytics)
    transition: Option<String>,
    /// Pending tool use summary from previous turn (Haiku-generated)
    pending_tool_use_summary: Option<String>,
    /// Abort controller for interrupting the query engine loop
    abort_controller: crate::utils::AbortController,
    /// Token budget tracker (TOKEN_BUDGET feature)
    budget_tracker: crate::token_budget::BudgetTracker,
    /// Output tokens consumed in the current turn (for TOKEN_BUDGET)
    turn_tokens: u64,
    /// Memory paths already loaded by parent agents
    loaded_nested_memory_paths: std::collections::HashSet<String>,
    /// Content replacement state for aggregate tool result budget enforcement
    content_replacement_state: Option<crate::services::compact::ContentReplacementState>,
    /// When the current query started (for duration_ms in AgentEvent::Done)
    start_time: Option<std::time::Instant>,
    /// task_budget.remaining tracking across compaction boundaries.
    /// Decremented by pre-compact final context after each compaction.
    task_budget_remaining: Option<u64>,
    /// Structured output retry count (for MAX_STRUCTURED_OUTPUT_RETRIES limit)
    structured_output_retries: u32,
    /// Whether the orphaned permission has been handled this engine lifetime.
    /// Matches TypeScript's hasHandledOrphanedPermission flag.
    has_handled_orphaned_permission: bool,
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;
type ToolExecutor = dyn Fn(serde_json::Value, &ToolContext) -> BoxFuture<Result<ToolResult, AgentError>>
    + Send
    + Sync;

/// Permission denial tracking for SDK reporting
#[derive(Debug, Clone, Default)]
pub struct PermissionDenial {
    pub tool_name: String,
    pub tool_use_id: String,
    pub tool_input: serde_json::Value,
}

/// Orphaned permission state for session resume.
/// When a session is resumed from a point where a tool-use was waiting
/// on a permission decision, this struct carries the pre-stored decision
/// so the query engine can inject the synthetic result.
#[derive(Debug, Clone)]
pub struct OrphanedPermission {
    pub tool_use_id: String,
    pub assistant_message: Message,
    pub permission_result: crate::permission::PermissionResult,
}

pub struct QueryEngineConfig {
    pub cwd: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub tools: Vec<ToolDefinition>,
    pub system_prompt: Option<String>,
    pub max_turns: u32,
    pub max_budget_usd: Option<f64>,
    pub max_tokens: u32,
    /// Fallback model to use when primary model fails (e.g., rate limit)
    pub fallback_model: Option<String>,
    /// User context (additional context to prepend to user messages)
    /// Matches TypeScript's prependUserContext
    pub user_context: HashMap<String, String>,
    /// System context (additional context to append to system prompt)
    pub system_context: HashMap<String, String>,
    /// Permission check function - called BEFORE tool execution
    /// Returns PermissionResult::Allow, ::Deny, ::Ask, or ::Passthrough
    pub can_use_tool:
        Option<std::sync::Arc<dyn Fn(ToolDefinition, serde_json::Value) -> crate::permission::PermissionResult + Send + Sync>>,
    /// Callback for agent events (tool start/complete/error, thinking, done)
    pub on_event: Option<std::sync::Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    /// Thinking configuration for the API
    /// Defaults to Adaptive if not specified
    pub thinking: Option<crate::types::api_types::ThinkingConfig>,
    /// External abort controller for interrupting the query engine loop.
    /// If provided, this will be used instead of creating a new one.
    pub abort_controller: Option<std::sync::Arc<crate::utils::AbortController>>,
    /// Token budget target in tokens (TOKEN_BUDGET feature).
    /// When set, the query loop continues until 90% of this budget is consumed,
    /// or diminishing returns are detected.
    pub token_budget: Option<f64>,
    /// Optional agent ID for subagent identification. Token budget is skipped for subagents.
    pub agent_id: Option<String>,
    /// Optional session state manager for tracking agent lifecycle states
    pub session_state: Option<std::sync::Arc<crate::session_state::SessionStateManager>>,
    /// Memory paths already loaded by parent agents
    pub loaded_nested_memory_paths: std::collections::HashSet<String>,
    /// API task_budget (distinct from tokenBudget +500k auto-continue feature).
    /// `total` is the budget for the whole agentic turn.
    pub task_budget: Option<TaskBudget>,
    /// Orphaned permission for session-resume scenarios.
    /// When present, the query engine injects the assistant message and a
    /// synthetic tool-result reflecting the stored permission decision before
    /// the main tool-call loop begins.
    pub orphaned_permission: Option<OrphanedPermission>,
}

#[derive(Debug, Clone)]
pub struct TaskBudget {
    pub total: u64,
}

impl Default for QueryEngineConfig {
    fn default() -> Self {
        Self {
            cwd: String::new(),
            model: String::new(),
            api_key: None,
            base_url: None,
            tools: vec![],
            system_prompt: None,
            max_turns: 10,
            max_budget_usd: None,
            max_tokens: 16384,
            fallback_model: None,
            user_context: HashMap::new(),
            system_context: HashMap::new(),
            can_use_tool: None,
            on_event: None,
            thinking: None,
            abort_controller: None,
            token_budget: None,
            agent_id: None,
            session_state: None,
            loaded_nested_memory_paths: std::collections::HashSet::new(),
            task_budget: None,
            orphaned_permission: None,
        }
    }
}

impl QueryEngine {
    pub fn new(mut config: QueryEngineConfig) -> Self {
        let loaded_memory_paths = config.loaded_nested_memory_paths.clone();
        let abort_controller = config.abort_controller.take().map_or_else(
            || crate::utils::create_abort_controller_default(),
            |arc| (*arc).clone(),
        );
        Self {
            config,
            messages: vec![],
            turn_count: 0,
            total_usage: TokenUsage::default(),
            total_cost: 0.0,
            http_client: reqwest::Client::new(),
            tool_executors: Mutex::new(HashMap::new()),
            tool_render_fns: Mutex::new(HashMap::new()),
            tool_backfill_fns: Mutex::new(HashMap::new()),
            hook_registry: Arc::new(Mutex::new(None)),
            auto_compact_tracking: AutoCompactTracking::default(),
            permission_denials: Vec::new(),
            last_stop_reason: None,
            max_output_tokens_recovery_count: 0,
            has_attempted_reactive_compact: false,
            max_output_tokens_override: None,
            stop_hook_active: false,
            transition: None,
            pending_tool_use_summary: None,
            empty_response_retries: 0,
            abort_controller,
            budget_tracker: crate::token_budget::BudgetTracker::new(),
            turn_tokens: 0,
            loaded_nested_memory_paths: loaded_memory_paths,
            content_replacement_state: Some(
                crate::services::compact::create_content_replacement_state(),
            ),
            start_time: None,
            task_budget_remaining: None,
            structured_output_retries: 0,
            has_handled_orphaned_permission: false,
        }
    }

    /// Register a tool executor (without metadata).
    /// For tools with rendering metadata, use `register_tool_with_render` instead.
    pub fn register_tool<F>(&mut self, name: String, executor: F)
    where
        F: Fn(serde_json::Value, &ToolContext) -> BoxFuture<Result<ToolResult, AgentError>>
            + Send
            + Sync
            + 'static,
    {
        self.tool_executors
            .lock()
            .unwrap()
            .insert(name, Arc::new(executor));
    }

    /// Register a backfill function for a tool.
    /// The function mutates a clone of the tool input before it's seen by hooks/events/transcripts.
    /// The original input is still passed to the tool executor (preserves prompt cache).
    pub fn register_tool_backfill<F>(&mut self, name: String, backfill_fn: F)
    where
        F: Fn(&mut serde_json::Value) + Send + Sync + 'static,
    {
        self.tool_backfill_fns
            .lock()
            .unwrap()
            .insert(name, Arc::new(backfill_fn));
    }

    /// Register a tool executor with render metadata for display hooks.
    /// This enables user_facing_name, get_tool_use_summary, and render_tool_result_message
    /// to be called during event emission in execute_tool.
    pub fn register_tool_with_render<F>(
        &mut self,
        name: String,
        executor: F,
        render_fns: ToolRenderFns,
    ) where
        F: Fn(serde_json::Value, &ToolContext) -> BoxFuture<Result<ToolResult, AgentError>>
            + Send
            + Sync
            + 'static,
    {
        self.tool_executors
            .lock()
            .unwrap()
            .insert(name.clone(), Arc::new(executor));
        self.tool_render_fns
            .lock()
            .unwrap()
            .insert(name, render_fns);
    }

    /// Set initial messages (for continuing a conversation)
    /// Interrupt the running query engine. This will abort the current
    /// tool execution loop and stop any in-flight API requests.
    pub fn interrupt(&self) {
        self.abort_controller.abort(None);
    }

    pub fn set_messages(&mut self, messages: Vec<crate::types::Message>) {
        self.messages = messages;
    }

    /// Separate tools into upfront (sent immediately) and deferred (loaded via ToolSearch).
    /// Returns (upfront_tools, deferred_tools).
    /// This matches the TypeScript's isDeferredTool() logic.
    pub(crate) fn separate_tools_for_request(&self) -> (Vec<ToolDefinition>, Vec<ToolDefinition>) {
        use crate::tools::deferred_tools::{extract_discovered_tool_names, is_deferred_tool};

        let mut upfront = Vec::new();
        let mut deferred = Vec::new();

        for tool in &self.config.tools {
            if is_deferred_tool(tool) {
                deferred.push(tool.clone());
            } else {
                upfront.push(tool.clone());
            }
        }

        // If tool search is disabled (standard mode), send all tools upfront
        if !crate::tools::deferred_tools::is_tool_search_enabled_optimistic() {
            upfront.extend(deferred.drain(..));
            return (upfront, deferred);
        }

        // Check for already-discovered deferred tools from message history
        // Build API message format from our internal messages
        let api_messages: Vec<serde_json::Value> = self
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    api_types::MessageRole::User => "user",
                    api_types::MessageRole::Assistant => "assistant",
                    api_types::MessageRole::System => "system",
                    api_types::MessageRole::Tool => "tool",
                };
                serde_json::json!({
                    "role": role,
                    "content": msg.content
                })
            })
            .collect();

        let discovered = extract_discovered_tool_names(&api_messages);

        // Move discovered deferred tools to upfront (they've been loaded via tool_reference)
        deferred.retain(|t| {
            if discovered.contains(&t.name) {
                upfront.push(t.clone());
                false
            } else {
                true
            }
        });

        // Sort and deduplicate upfront tools for prompt cache stability
        let upfront = crate::tools::assemble_tool_pool(
            &upfront,
            &[], // deferred tools handled separately
        );

        (upfront, deferred)
    }

    /// Inject <available-deferred-tools> block into messages if tool search is enabled.
    /// This tells the model about deferred tool names so it can discover them via ToolSearch.
    pub(crate) fn maybe_inject_deferred_tools_block(
        &self,
        api_messages: &mut Vec<serde_json::Value>,
    ) {
        use crate::tools::deferred_tools::{
            extract_discovered_tool_names, get_deferred_tool_names, is_deferred_tool,
            is_tool_search_enabled_optimistic,
        };

        // Only inject if tool search is enabled
        if !is_tool_search_enabled_optimistic() {
            return;
        }

        // Get deferred tool names
        let all_deferred = get_deferred_tool_names(&self.config.tools);

        // Find already-discovered tools
        let discovered = extract_discovered_tool_names(api_messages);

        // Only show tools that haven't been discovered yet
        let undiscovered: Vec<&str> = all_deferred
            .iter()
            .filter(|name| !discovered.contains(*name))
            .map(|s| s.as_str())
            .collect();

        if undiscovered.is_empty() {
            return;
        }

        // Build the <available-deferred-tools> block
        let block_content = format!(
            "<available-deferred-tools>\n{}\n</available-deferred-tools>\n\n\
             Deferred tools appear by name above. \
             To use a deferred tool, call ToolSearchTool with query \"select:<tool_name>\" to fetch its schema. \
             Once fetched, the tool will be available for use.",
            undiscovered.join("\n")
        );

        // Inject as the first user message (after any existing system messages)
        let inject_msg = serde_json::json!({
            "role": "user",
            "content": block_content,
            "is_meta": true
        });

        // Find the position to inject (after any system messages, before first real user message)
        let mut insert_pos = 0;
        for (i, msg) in api_messages.iter().enumerate() {
            if msg.get("role").and_then(|v| v.as_str()) == Some("user") {
                insert_pos = i;
                break;
            }
            insert_pos = i + 1;
        }

        api_messages.insert(insert_pos, inject_msg);
    }

    /// Execute a tool by name
    pub async fn execute_tool(
        &mut self,
        name: &str,
        input: serde_json::Value,
        tool_call_id: String,
    ) -> Result<ToolResult, AgentError> {
        let context = ToolContext {
            cwd: self.config.cwd.clone(),
            abort_signal: Arc::clone(self.abort_controller.signal()),
        };

        // Clone the Arc out of the maps
        let (executor, render_metadata) = {
            let executors = self.tool_executors.lock().unwrap();
            let render_fns = self.tool_render_fns.lock().unwrap();
            (
                executors.get(name).cloned(),
                render_fns.get(name).map(|fns| ToolRenderMetadata {
                    user_facing_name: (Arc::clone(&fns.user_facing_name))(Some(&input)),
                    tool_use_summary: fns
                        .get_tool_use_summary
                        .as_ref()
                        .and_then(|f| f(Some(&input))),
                    activity_description: fns
                        .get_activity_description
                        .as_ref()
                        .and_then(|f| f(Some(&input))),
                }),
            )
        };

        if let Some(executor) = executor {
            // PRE-TOOL PERMISSION CHECK - matches TypeScript's wrappedCanUseTool
            // Returns 3-way PermissionResult: Allow, Deny, Ask, Passthrough
            if let Some(can_use_tool_fn) = &self.config.can_use_tool {
                if let Some(tool_def) = self.config.tools.iter().find(|t| &t.name == name) {
                    match can_use_tool_fn(tool_def.clone(), input.clone()) {
                        crate::permission::PermissionResult::Allow(_)
                        | crate::permission::PermissionResult::Passthrough { .. } => {
                            // Allowed, continue
                        }
                        crate::permission::PermissionResult::Deny(d) => {
                            self.permission_denials.push(PermissionDenial {
                                tool_name: name.to_string(),
                                tool_use_id: tool_call_id.clone(),
                                tool_input: input.clone(),
                            });
                            return Err(AgentError::Tool(format!(
                                "Tool '{}' permission denied: {}",
                                name, d.message
                            )));
                        }
                        crate::permission::PermissionResult::Ask(a) => {
                            // In SDK mode, Ask defaults to deny with a message
                            // (CLI would prompt the user interactively)
                            self.permission_denials.push(PermissionDenial {
                                tool_name: name.to_string(),
                                tool_use_id: tool_call_id.clone(),
                                tool_input: input.clone(),
                            });
                            return Err(AgentError::Tool(format!(
                                "Tool '{}' requires user confirmation (Ask mode not supported in SDK): {}",
                                name, a.message
                            )));
                        }
                    }
                }
            }

            // Emit ToolStart event with render metadata
            if let Some(ref cb) = self.config.on_event {
                if let Some(ref metadata) = render_metadata {
                    let user_facing = &metadata.user_facing_name;
                    cb(AgentEvent::ToolStart {
                        tool_name: name.to_string(),
                        tool_call_id: tool_call_id.clone(),
                        input: input.clone(),
                        display_name: Some(user_facing.clone()),
                        summary: metadata.tool_use_summary.clone(),
                        activity_description: metadata.activity_description.clone(),
                    });
                } else {
                    cb(AgentEvent::ToolStart {
                        tool_name: name.to_string(),
                        tool_call_id: tool_call_id.clone(),
                        input: input.clone(),
                        display_name: None,
                        summary: None,
                        activity_description: None,
                    });
                }
            }

            self.run_pre_tool_use_hooks(name, &input, &tool_call_id)
                .await?;

            // Execute the tool with timing
            let tool_start = std::time::Instant::now();
            let result = executor(input.clone(), &context).await;
            let tool_duration_ms = tool_start.elapsed().as_millis() as u64;
            crate::services::model_cost::record_turn_tool_duration(tool_duration_ms);

            // Emit ToolComplete or ToolError event with render hooks
            if let Some(ref cb) = self.config.on_event {
                match &result {
                    Ok(tool_result) => {
                        // Try to render the result message
                        let rendered_result = self.render_tool_result(name, &tool_result.content);
                        if let Some(ref metadata) = render_metadata {
                            let display = format!(
                                "{}({})",
                                metadata.user_facing_name,
                                metadata.tool_use_summary.as_deref().unwrap_or("?")
                            );
                            cb(AgentEvent::ToolComplete {
                                tool_name: name.to_string(),
                                tool_call_id: tool_call_id.clone(),
                                result: tool_result.clone(),
                                display_name: Some(display),
                                rendered_result: rendered_result.clone(),
                            });
                        } else {
                            cb(AgentEvent::ToolComplete {
                                tool_name: name.to_string(),
                                tool_call_id: tool_call_id.clone(),
                                result: tool_result.clone(),
                                display_name: None,
                                rendered_result: rendered_result,
                            });
                        }
                    }
                    Err(e) => {
                        cb(AgentEvent::ToolError {
                            tool_name: name.to_string(),
                            tool_call_id: tool_call_id.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }

            // Run PostToolUse or PostToolUseFailure hooks
            match &result {
                Ok(tool_result) => {
                    self.run_post_tool_use_hooks(name, tool_result, &tool_call_id)
                        .await;
                }
                Err(e) => {
                    self.run_post_tool_use_failure_hooks(name, e, &tool_call_id)
                        .await;
                }
            }

            result
        } else {
            Err(AgentError::Tool(format!("Tool '{}' not found", name)))
        }
    }

    /// Render a tool's result using its stored render_tool_result_message closure.
    /// Returns None if the tool has no render implementation or the content can't be parsed.
    fn render_tool_result(&self, tool_name: &str, content: &str) -> Option<String> {
        let content_value: serde_json::Value = serde_json::from_str(content).ok()?;
        let progress_messages: Vec<ProgressMessage> = vec![];
        let options = ToolResultRenderOptions {
            style: None,
            theme: "dark".to_string(),
            tools: self.config.tools.clone(),
            verbose: false,
            is_transcript_mode: false,
            is_brief_only: false,
            input: None,
        };
        let fns = self.tool_render_fns.lock().unwrap();
        let render_fn = fns.get(tool_name)?.render_tool_result_message.as_ref()?;
        render_fn(&content_value, &progress_messages, &options)
    }

    /// Set the hook registry
    pub fn set_hook_registry(&self, registry: HookRegistry) {
        let mut guard = self.hook_registry.lock().unwrap();
        *guard = Some(registry);
    }

    /// Run PreToolUse hooks
    async fn run_pre_tool_use_hooks(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        tool_use_id: &str,
    ) -> Result<(), AgentError> {
        // First check if we have hooks (outside of lock)
        let has_hooks = {
            let guard = self.hook_registry.lock().unwrap();
            guard
                .as_ref()
                .map(|r| r.has_hooks("PreToolUse"))
                .unwrap_or(false)
        };

        if !has_hooks {
            return Ok(());
        }

        // Build input outside of lock
        let input = HookInput {
            event: "PreToolUse".to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(tool_input.clone()),
            tool_output: None,
            tool_use_id: Some(tool_use_id.to_string()),
            session_id: None,
            cwd: Some(self.config.cwd.clone()),
            error: None,
            ..HookInput::default()
        };

        // Execute hooks (registry is Clone and Arc-wrapped, so we can clone the reference)
        let registry = {
            let guard = self.hook_registry.lock().unwrap();
            guard.as_ref().cloned()
        };

        if let Some(registry) = registry {
            let results = registry.execute("PreToolUse", input).await;

            // Check if any hook blocked the tool use
            for output in results {
                if let Some(block) = output.block {
                    if block {
                        return Err(AgentError::Tool(format!(
                            "Tool '{}' blocked by PreToolUse hook",
                            tool_name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Run PostToolUse hooks
    async fn run_post_tool_use_hooks(
        &self,
        tool_name: &str,
        tool_output: &ToolResult,
        tool_use_id: &str,
    ) {
        let has_hooks = {
            let guard = self.hook_registry.lock().unwrap();
            guard
                .as_ref()
                .map(|r| r.has_hooks("PostToolUse"))
                .unwrap_or(false)
        };

        if !has_hooks {
            return;
        }

        let input = HookInput {
            event: "PostToolUse".to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_input: None,
            tool_output: Some(serde_json::json!({
                "result_type": tool_output.result_type,
                "content": tool_output.content,
                "is_error": tool_output.is_error,
            })),
            tool_use_id: Some(tool_use_id.to_string()),
            session_id: None,
            cwd: Some(self.config.cwd.clone()),
            error: None,
            ..HookInput::default()
        };

        let registry = {
            let guard = self.hook_registry.lock().unwrap();
            guard.as_ref().cloned()
        };

        if let Some(registry) = registry {
            let _ = registry.execute("PostToolUse", input).await;
        }
    }

    /// Run PostToolUseFailure hooks
    async fn run_post_tool_use_failure_hooks(
        &self,
        tool_name: &str,
        error: &AgentError,
        tool_use_id: &str,
    ) {
        let has_hooks = {
            let guard = self.hook_registry.lock().unwrap();
            guard
                .as_ref()
                .map(|r| r.has_hooks("PostToolUseFailure"))
                .unwrap_or(false)
        };

        if !has_hooks {
            return;
        }

        let input = HookInput {
            event: "PostToolUseFailure".to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_input: None,
            tool_output: None,
            tool_use_id: Some(tool_use_id.to_string()),
            session_id: None,
            cwd: Some(self.config.cwd.clone()),
            error: Some(error.to_string()),
            ..HookInput::default()
        };

        let registry = {
            let guard = self.hook_registry.lock().unwrap();
            guard.as_ref().cloned()
        };

        if let Some(registry) = registry {
            let _ = registry.execute("PostToolUseFailure", input).await;
        }
    }

    pub fn get_turn_count(&self) -> u32 {
        self.turn_count
    }

    /// Get total token usage from all API calls
    pub fn get_usage(&self) -> TokenUsage {
        self.total_usage.clone()
    }

    pub fn get_messages(&self) -> Vec<crate::types::Message> {
        self.messages.clone()
    }

    /// Milliseconds elapsed since the start of the current query.
    /// Returns 0 if no query is active (start_time is None).
    pub fn query_duration_ms(&self) -> u64 {
        self.start_time
            .map(|t| std::time::Instant::now().duration_since(t).as_millis() as u64)
            .unwrap_or(0)
    }

    /// Reset conversation state — clear messages, usage, and turn count.
    /// Preserves config, tool executors, and abort controller.
    pub fn reset(&mut self) {
        self.messages.clear();
        self.reset_counters();
    }

    /// Reset only counters (turn count, usage, cost, recovery tracking).
    /// Does NOT clear messages — preserves conversation state across errors
    /// so the user can replay / continue (matches TypeScript behavior).
    pub fn reset_counters(&mut self) {
        self.turn_count = 0;
        self.total_usage = TokenUsage::default();
        self.total_cost = 0.0;
        self.permission_denials.clear();
        self.last_stop_reason = None;
        self.max_output_tokens_recovery_count = 0;
        self.has_attempted_reactive_compact = false;
        self.empty_response_retries = 0;
        self.max_output_tokens_override = None;
        self.stop_hook_active = false;
        self.transition = None;
        self.pending_tool_use_summary = None;
        self.structured_output_retries = 0;
    }

    /// Check if the last message represents a valid successful result.
    /// Matches TypeScript's isResultSuccessful() at queryHelpers.ts:56:
    /// - Assistant: has non-empty content and is not an API error message
    /// - User: has tool_result blocks (valid terminal state after tool execution)
    /// Does NOT check stop_reason — TS only validates message type/content.
    fn is_result_successful(&self, _last_stop_reason: Option<&str>) -> bool {
        let last = match self.messages.last() {
            Some(m) => m,
            None => return false,
        };
        match last.role {
            crate::types::MessageRole::Assistant => {
                !last.content.is_empty() && last.is_api_error_message != Some(true)
            }
            crate::types::MessageRole::User => {
                // User message (tool results) is a valid terminal state
                true
            }
            _ => false,
        }
    }

    /// Generate synthetic tool_result messages for orphaned tool_use blocks
    /// in the last assistant message.  Called before terminal error handling
    /// so that the conversation history remains well-formed (every tool_use
    /// has a matching tool_result).
    ///
    /// Matches TypeScript's yieldMissingToolResultBlocks /
    /// addOrphanedToolResults.
    fn add_orphaned_tool_results(&mut self, reason: &str) {
        // Collect the tool_call IDs from the last assistant message first
        // (to avoid borrow conflicts with self.messages.push below)
        let orphan_ids: Vec<(String, String)> = {
            let last = match self.messages.last() {
                Some(m) => m,
                None => return,
            };
            if last.role != crate::types::MessageRole::Assistant {
                return;
            }
            let tool_calls = match &last.tool_calls {
                Some(tc) => tc,
                None => return,
            };
            tool_calls.iter()
                .map(|tc| (tc.id.clone(), tc.name.clone()))
                .collect()
        };
        if orphan_ids.is_empty() {
            return;
        }

        // Find tool_use IDs that already have a tool_result in the messages
        let mut has_result = std::collections::HashSet::new();
        for msg in &self.messages {
            if msg.role == crate::types::MessageRole::Tool {
                if let Some(id) = &msg.tool_call_id {
                    has_result.insert(id.clone());
                }
            }
        }
        // Add synthetic tool_result for each orphaned tool_use
        for (tc_id, tc_name) in orphan_ids {
            if !has_result.contains(&tc_id) {
                self.messages.push(crate::types::Message {
                    role: crate::types::MessageRole::Tool,
                    content: format!("Tool '{}' was not executed: {}", tc_name, reason),
                    tool_call_id: Some(tc_id),
                    is_error: Some(true),
                    ..Default::default()
                });
            }
        }
    }

    /// Attempt to auto-compact the conversation when token count exceeds threshold
    /// Translated from: compactConversation in compact.ts
    /// Returns Ok(true) if compaction happened, Ok(false) if not needed, Err on failure
    /// Execute auto-compact.
    /// `snip_tokens_freed` is subtracted from the token count for the threshold
    /// check, matching TypeScript's autocompact threshold adjustment.
    /// Returns Ok(Some(summary)) if compaction happened with a formatted summary,
    /// Ok(None) if not needed, Err on failure.
    async fn do_auto_compact(&mut self, snip_tokens_freed: u32) -> Result<Option<String>, AgentError> {
        use crate::compact::{
            annotate_boundary_with_preserved_segment,
            build_post_compact_messages, create_compact_boundary_message,
            estimate_token_count, get_auto_compact_threshold,
            merge_hook_instructions, re_append_session_metadata,
            strip_images_from_messages, strip_reinjected_attachments,
        };
        use crate::services::compact::{
            format_compact_summary, get_compact_prompt as get_compact_prompt_service,
            get_compact_user_summary_message,
        };
        use crate::tools::deferred_tools::{
            get_deferred_tool_names, is_tool_search_enabled_optimistic,
        };

        let token_count = estimate_token_count(&self.messages, self.config.max_tokens);
        let threshold = get_auto_compact_threshold(&self.config.model);

        // Adjust for snip: subtract tokens snip already freed (matches TypeScript)
        let effective_tokens = (token_count as i64).saturating_sub(snip_tokens_freed as i64) as u32;

        // Check if we need to compact
        if effective_tokens <= threshold {
            return Ok(None);
        }

        log::info!(
            "[compact] Starting auto-compact: {} effective tokens ({} raw - {} snip freed), threshold: {}",
            effective_tokens,
            token_count,
            snip_tokens_freed,
            threshold
        );

        // Phase 1: Pre-compact hooks
        // Execute pre_compact hooks and merge any custom instructions
        let hook_custom_instructions = self.execute_pre_compact_hooks().await;
        // Merge hook instructions into compact prompt (matches TypeScript mergeHookInstructions)
        let merged_instructions =
            merge_hook_instructions(None, hook_custom_instructions.as_deref());

        // Emit CompactStart after hooks_start{PreCompact} (matches TypeScript order)
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Compact {
                event: CompactProgressEvent::CompactStart,
            });
        }

        // Phase 2: Try session memory compaction first (faster, no API call)
        if let Some(sm_result) = crate::services::compact::try_session_memory_compaction(
            &self.messages,
            None,
            Some(threshold as usize),
        )
        .await
        {
            if sm_result.compacted {
                log::info!("[compact] Session memory compaction succeeded");
                self.apply_compaction_result(
                    sm_result.messages_to_keep,
                    sm_result.post_compact_token_count as u32,
                );
                // Re-append session metadata (matches TypeScript compactConversation)
                re_append_session_metadata(
                    self.config.agent_id.as_deref().unwrap_or(""),
                    &self.config.model,
                    &self.config.cwd,
                    None,
                    None,
                );
                // Post-compaction bookkeeping (matches TypeScript compactConversation finally block)
                crate::bootstrap::state::mark_post_compaction();
                let _ = crate::services::api::prompt_cache_break_detection::notify_compaction(
                    "repl_main_thread",
                    self.config.agent_id.as_deref(),
                );
                // Post-compact cleanup (matches TypeScript autoCompactIfNeeded)
                crate::services::compact::run_post_compact_cleanup(Some("repl_main_thread"));
                // Post-compact hooks
                self.execute_post_compact_hooks("Session memory compaction applied").await;

                return Ok(Some("Session memory compaction applied".to_string()));
            }
        }

        // Phase 3: Strip images and reinjected attachments before compact API call
        let stripped_messages =
            strip_reinjected_attachments(&strip_images_from_messages(&self.messages));

        // Phase 4: Build compact prompt with merged hook instructions
        let compact_prompt = get_compact_prompt_service(merged_instructions.as_deref());

        // Phase 5: Generate summary using LLM with PTL retry logic
        let (summary, compaction_usage) = match self
            .generate_summary_with_ptl_retry(&stripped_messages, &compact_prompt)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                log::warn!("[compact] Summary generation failed: {}", e);
                return Err(e);
            }
        };
        log::debug!(
            "[compact] compaction_usage: input={} output={}",
            compaction_usage.input_tokens,
            compaction_usage.output_tokens
        );

        // Feed compaction API cost into session total
        let compact_cost = crate::services::model_cost::calculate_cost_for_tokens(
            &self.config.model,
            compaction_usage.input_tokens as u32,
            compaction_usage.output_tokens as u32,
            compaction_usage.cache_read_input_tokens.unwrap_or(0) as u32,
            compaction_usage.cache_creation_input_tokens.unwrap_or(0) as u32,
        );
        let _ = crate::services::model_cost::add_to_total_session_cost(
            compact_cost,
            compaction_usage.input_tokens as u32,
            compaction_usage.output_tokens as u32,
            compaction_usage.cache_read_input_tokens.unwrap_or(0) as u32,
            compaction_usage.cache_creation_input_tokens.unwrap_or(0) as u32,
            0,
            &self.config.model,
        );

        // Parse and format the summary
        let formatted_summary = format_compact_summary(&summary);

        // Phase 6: Build messages_to_keep (last 4 messages)
        let messages_to_keep: Vec<Message> = if self.messages.len() > 4 {
            self.messages[self.messages.len() - 4..].to_vec()
        } else {
            self.messages.clone()
        };

        // Create boundary marker with compact metadata
        let last_uuid = self
            .messages
            .last()
            .and_then(|m| m.uuid.as_deref());
        let discovered_tools = get_deferred_tool_names(&self.config.tools);
        let mut boundary_msg = create_compact_boundary_message(
            "auto",
            token_count,
            last_uuid,
            None,
            Some(self.messages.len()),
        );
        // Append summary content to boundary
        boundary_msg.content.push_str("\n\n");
        boundary_msg
            .content
            .push_str(&get_compact_user_summary_message(
                &formatted_summary,
                Some(true),
                None,
                None,
            ));
        // Attach deferred tools if applicable
        if !discovered_tools.is_empty() && is_tool_search_enabled_optimistic() {
            boundary_msg.content.push_str("\n\n<available-deferred-tools>\n");
            boundary_msg.content.push_str(&discovered_tools.join("\n"));
            boundary_msg.content.push_str("\n</available-deferred-tools>");
        }

        // Annotate boundary with preserved-segment metadata for session storage relinking
        // For prefix-preserving compact, anchor = boundary itself
        let anchor_uuid = boundary_msg.uuid.clone().unwrap_or_default();
        annotate_boundary_with_preserved_segment(
            &mut boundary_msg,
            &anchor_uuid,
            &messages_to_keep,
        );

        // Phase 6b: Post-compact attachment re-injection (matches TypeScript)
        let mut attachments: Vec<Message> = Vec::new();

        // Re-inject recently read files
        let file_attachments = crate::compact::create_post_compact_file_attachments(
            &crate::bootstrap::state::get_file_read_state(),
            &messages_to_keep,
            crate::compact::POST_COMPACT_MAX_FILES_TO_RESTORE as usize,
        );
        attachments.extend(file_attachments);

        // Re-inject skill attachments from tracked invocations
        let invoked_skills = crate::bootstrap::state::get_invoked_skills_for_agent(
            self.config.agent_id.as_deref(),
        );
        let skill_list: Vec<(String, String)> = invoked_skills
            .into_values()
            .map(|s| (s.skill_name.clone(), s.content))
            .collect();
        let skill_attachments =
            crate::compact::create_post_compact_skill_attachments(&skill_list);
        attachments.extend(skill_attachments);

        // Phase 6c: SessionStart hook re-injection (matches TypeScript)
        // Emit SessionStart hooks event
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Compact {
                event: CompactProgressEvent::HooksStart {
                    hook_type: CompactHookType::SessionStart,
                },
            });
        }

        // Process SessionStart hooks after successful compaction
        let hook_registry_for_session = {
            let guard = self.hook_registry.lock().unwrap();
            guard.clone()
        };
        let session_hook_results = crate::utils::conversation_recovery::process_session_start_hooks(
            "compact",
            None,
            &self.config.model,
            hook_registry_for_session.as_ref(),
        )
        .await;

        // Phase 7: markPostCompaction + notifyCompaction (matches TypeScript)
        crate::bootstrap::state::mark_post_compaction();
        let _ = crate::services::api::prompt_cache_break_detection::notify_compaction(
            "repl_main_thread",
            self.config.agent_id.as_deref(),
        );

        // Re-append session metadata (keeps metadata in 16KB tail window)
        re_append_session_metadata(
            self.config.agent_id.as_deref().unwrap_or(""),
            &self.config.model,
            &self.config.cwd,
            None,
            None,
        );

        // Build compaction result with all parts
        let compact_input_tokens = compaction_usage.input_tokens;
        let compact_output_tokens = compaction_usage.output_tokens;
        let compaction_result = crate::compact::CompactionResult {
            boundary_marker: boundary_msg,
            summary_messages: vec![Message {
                role: MessageRole::User,
                content: get_compact_user_summary_message(
                    &formatted_summary,
                    Some(true),
                    None,
                    None,
                ),
                is_meta: Some(true),
                ..Default::default()
            }],
            messages_to_keep: Some(messages_to_keep.clone()),
            attachments,
            hook_results: session_hook_results,
            pre_compact_token_count: token_count,
            post_compact_token_count: compact_input_tokens as u32,
            true_post_compact_token_count: None,
            compaction_usage: Some(compaction_usage),
        };

        // Build final message list with proper ordering
        let post_compact_messages = build_post_compact_messages(&compaction_result);

        // true_post_compact_token_count: rough estimation from compacted messages
        let true_post_compact_tokens = crate::compact::rough_token_count_estimation_for_content(
            &post_compact_messages
                .iter()
                .map(|m| m.content.clone())
                .collect::<String>(),
        ) as u64;
        let compact_usage_ref = compaction_result.compaction_usage.as_ref();
        log::debug!(
            "[compact] true_post_compact_token_count={} compaction_usage.input={} compaction_usage.output={}",
            true_post_compact_tokens,
            compact_usage_ref.map(|u| u.input_tokens).unwrap_or(0),
            compact_usage_ref.map(|u| u.output_tokens).unwrap_or(0),
        );

        // Phase 8: Post-compact hooks
        self.execute_post_compact_hooks(&formatted_summary).await;

        // Phase 9: Post-compaction cleanup (clears all caches)
        crate::services::compact::run_post_compact_cleanup(Some("repl_main_thread"));
        // Clear tracked state after compaction so post-compact attachments are fresh
        crate::bootstrap::state::clear_file_read_state();

        // Apply the new messages
        self.messages = post_compact_messages;
        let new_token_count = crate::compact::estimate_token_count(
            &self.messages,
            self.config.max_tokens,
        );

        log::info!(
            "[compact] Complete: {} tokens -> {} tokens",
            token_count,
            new_token_count
        );

        // Build human-readable summary for CompactEnd event
        let pct_reduced = if token_count > 0 {
            ((token_count as i64 - new_token_count as i64) as f64
                / token_count as f64)
                * 100.0
        } else {
            0.0
        };
        Ok(Some(format!(
            "Conversation compacted: {} → {} tokens ({:.0}% reduced)",
            token_count,
            new_token_count,
            pct_reduced
        )))
    }

    /// Generate summary with PTL (prompt-too-long) retry logic.
    /// If the compact API call fails with prompt-too-long, drops oldest
    /// message groups until the token gap is covered.
    /// Matches TypeScript: compact.ts for(;;) loop with truncateHeadForPTLRetry.
    async fn generate_summary_with_ptl_retry(
        &self,
        messages: &[Message],
        compact_prompt: &str,
    ) -> Result<(String, TokenUsage), AgentError> {
        const MAX_PTL_RETRIES: usize = 3;

        // Build messages for summary request
        let mut summary_messages = self.build_summary_messages(compact_prompt);

        for attempt in 0..MAX_PTL_RETRIES {
            // Estimate tokens and check if truncation needed
            let max_summary_tokens = 2048u32;
            let (truncated_messages, estimated_tokens) = compact::truncate_messages_for_summary(
                &summary_messages,
                &self.config.model,
                max_summary_tokens,
            );

            // Attempt summary generation
            match self
                .generate_summary_from_messages(&truncated_messages)
                .await
            {
                Ok((summary, usage)) => return Ok((summary, usage)),
                Err(e) => {
                    let error_str = format!("{}", e);
                    // Check if this is specifically a prompt-too-long error
                    let is_ptl = error_str.to_lowercase().contains("prompt is too long")
                        || error_str.to_lowercase().contains("prompt too long");

                    if is_ptl && attempt < MAX_PTL_RETRIES - 1 {
                        // Parse token gap from error for precise truncation
                        let token_gap = self.extract_ptl_token_gap(&error_str);
                        log::warn!(
                            "[compact] PTL retry {}/{}: gap={:?} tokens, dropping oldest groups",
                            attempt + 1,
                            MAX_PTL_RETRIES,
                            token_gap
                        );
                        summary_messages =
                            self.truncate_head_for_ptl_retry(&summary_messages, token_gap);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(AgentError::Api(
            "Summary generation failed after max retries".to_string(),
        ))
    }

    /// Extract the token gap (actual - limit) from a PTL error string.
    /// Returns None if the gap is unparseable (fallback to 20% in truncation).
    fn extract_ptl_token_gap(&self, error_str: &str) -> Option<u64> {
        use crate::services::api::errors::{
            parse_prompt_too_long_token_counts, PROMPT_TOO_LONG_ERROR_MESSAGE,
        };
        if !error_str.to_lowercase().contains(&PROMPT_TOO_LONG_ERROR_MESSAGE.to_lowercase()) {
            return None;
        }
        let (actual, limit) = parse_prompt_too_long_token_counts(error_str);
        let (Some(a), Some(l)) = (actual, limit) else {
            return None;
        };
        let gap = (a as i64).saturating_sub(l as i64);
        if gap > 0 { Some(gap as u64) } else { None }
    }

    /// Truncate the head of messages for PTL retry.
    /// Groups messages by API round and drops oldest groups until token gap covered.
    /// If unparseable token gap: drops 20% of groups.
    /// Keeps at least one group to ensure there's something to summarize.
    /// Prepends PTL_RETRY_MARKER if result starts with assistant (API requires user-first).
    /// Matches TypeScript: compact.ts truncateHeadForPTLRetry.
    fn truncate_head_for_ptl_retry(
        &self,
        messages: &[Message],
        token_gap: Option<u64>,
    ) -> Vec<Message> {
        use crate::services::compact::grouping::group_messages_by_api_round;

        // Strip previous PTL_RETRY_MARKER from a prior retry before regrouping
        let input = if messages.first().is_some_and(|m| {
            m.is_meta == Some(true) && m.content == crate::compact::PTL_RETRY_MARKER
        }) {
            &messages[1..]
        } else {
            messages
        };

        let groups = group_messages_by_api_round(input);
        if groups.len() < 2 {
            return messages.to_vec();
        }

        let groups_to_drop = if let Some(gap) = token_gap {
            // Gap-aware: accumulate group tokens until we cover the reported gap
            let mut acc: u64 = 0;
            let mut drop = 0;
            for g in &groups {
                acc += crate::services::token_estimation::rough_token_count_estimation_for_messages(g) as u64;
                drop += 1;
                if acc >= gap {
                    break;
                }
            }
            drop
        } else {
            // Fallback: drop 20% of groups (ceiling)
            let drop = ((groups.len() as f64 * 0.2).ceil() as usize).max(1);
            drop
        };

        let groups_to_drop = groups_to_drop.min(groups.len() - 1); // Keep at least one group
        if groups_to_drop < 1 {
            return messages.to_vec();
        }

        log::debug!(
            "[compact] Dropping {} of {} groups for PTL retry (gap={:?})",
            groups_to_drop,
            groups.len(),
            token_gap
        );

        let mut sliced: Vec<Message> = groups.into_iter().skip(groups_to_drop).flatten().collect();

        // Ensure first message is role=user (API requirement after dropping group 0)
        if sliced.first().is_some_and(|m| m.role == crate::types::MessageRole::Assistant) {
            sliced.insert(0, Message {
                role: crate::types::MessageRole::User,
                content: crate::compact::PTL_RETRY_MARKER.to_string(),
                is_meta: Some(true),
                ..Default::default()
            });
        }

        sliced
    }

    /// Build messages for summary generation request
    fn build_summary_messages(&self, compact_prompt: &str) -> Vec<Message> {
        let mut summary_messages = vec![Message {
            role: MessageRole::User,
            content: compact_prompt.to_string(),
            ..Default::default()
        }];

        // Add conversation messages, excluding existing system boundary messages
        for msg in &self.messages {
            if let MessageRole::System = msg.role {
                // Skip system boundary messages from previous compactions
                if msg.content.contains("compacted") || msg.content.contains("summarized") {
                    continue;
                }
            }
            summary_messages.push(msg.clone());
        }

        summary_messages
    }

    /// Generate summary from a set of messages
    async fn generate_summary_from_messages(
        &self,
        summary_messages: &[Message],
    ) -> Result<(String, TokenUsage), AgentError> {
        // Get API configuration
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| AgentError::Api("API key not provided".to_string()))?;

        let base_url = self
            .config
            .base_url
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("https://api.anthropic.com");

        let model = &self.config.model;

        // Convert messages to API format
        let api_summary_messages: Vec<serde_json::Value> = summary_messages
            .iter()
            .map(|msg| {
                let role_str = match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "user",
                    MessageRole::System => "system",
                };
                let mut msg_json = serde_json::json!({
                    "role": role_str,
                    "content": msg.content
                });
                if let Some(tool_call_id) = &msg.tool_call_id {
                    msg_json["tool_call_id"] = serde_json::json!(tool_call_id);
                }
                msg_json
            })
            .collect();

        // Build request with model-based compaction max_tokens (TS: compact.ts:1317-1320)
        let compact_max_tokens = crate::utils::context::COMPACT_MAX_OUTPUT_TOKENS
            .min(crate::utils::context::get_max_output_tokens_for_model(model)) as u32;
        let request_body = serde_json::json!({
            "model": model,
            "max_tokens": compact_max_tokens,
            "messages": api_summary_messages,
        });

        let client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", base_url);
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("User-Agent", get_user_agent())
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::Api(format!("Failed to send summary request: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError::Api(format!("Failed to read summary response: {}", e)))?;

        let response_json: serde_json::Value =
            serde_json::from_str(&response_text).map_err(|e| {
                AgentError::Api(format!(
                    "Failed to parse summary response: {} - {}",
                    e, response_text
                ))
            })?;

        if let Some(error) = response_json.get("error") {
            // Extract raw error message for PTL gap parsing
            let error_msg = error.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(&error.to_string())
                .to_string();
            return Err(AgentError::Api(error_msg));
        }

        // Extract usage from the compaction API call
        let usage = response_json.get("usage").map(|u| TokenUsage {
            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            cache_creation_input_tokens: u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()),
            cache_read_input_tokens: u.get("cache_read_input_tokens").and_then(|v| v.as_u64()),
            iterations: u.get("iterations").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|it| {
                    Some(IterationUsage {
                        input_tokens: it.get("input_tokens").and_then(|v| v.as_u64())?,
                        output_tokens: it.get("output_tokens").and_then(|v| v.as_u64())?,
                    })
                }).collect()
            }),
        }).unwrap_or_default();

        let summary = extract_text_from_response(&response_json);

        if summary.is_empty() {
            return Err(AgentError::Api("Summary response was empty".to_string()));
        }

        // Parse the summary to extract just the <summary> content
        let parsed_summary = parse_compact_summary(&summary);

        Ok((parsed_summary, usage))
    }

    /// Execute pre-compact hooks
    async fn execute_pre_compact_hooks(&self) -> Option<String> {
        let registry = {
            let guard = self.hook_registry.lock().unwrap();
            match guard.as_ref() {
                Some(r) => r.clone(),
                None => return None,
            }
        };

        if !registry.has_hooks("PreCompact") {
            return None;
        }

        // Emit hooks_start event
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Compact {
                event: CompactProgressEvent::HooksStart {
                    hook_type: CompactHookType::PreCompact,
                },
            });
        }

        let trigger = if self.auto_compact_tracking.compacted {
            "auto"
        } else {
            "manual"
        };

        let input = HookInput {
            event: "PreCompact".to_string(),
            tool_name: None,
            tool_input: Some(serde_json::json!({
                "trigger": trigger,
                "custom_instructions": null
            })),
            tool_output: None,
            tool_use_id: None,
            session_id: None,
            cwd: Some(self.config.cwd.clone()),
            error: None,
            ..HookInput::default()
        };

        let results = registry.execute("PreCompact", input).await;

        // Extract custom instructions from successful hooks with non-empty output
        let successful_outputs: Vec<String> = results
            .iter()
            .filter_map(|r| r.message.as_ref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if successful_outputs.is_empty() {
            None
        } else {
            Some(successful_outputs.join("\n\n"))
        }
    }

    /// Execute post-compact hooks
    async fn execute_post_compact_hooks(&self, compact_summary: &str) {
        let registry = {
            let guard = self.hook_registry.lock().unwrap();
            match guard.as_ref() {
                Some(r) => r.clone(),
                None => return,
            }
        };

        if !registry.has_hooks("PostCompact") {
            return;
        }

        // Emit hooks_start event
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Compact {
                event: CompactProgressEvent::HooksStart {
                    hook_type: CompactHookType::PostCompact,
                },
            });
        }

        let trigger = if self.auto_compact_tracking.compacted {
            "auto"
        } else {
            "manual"
        };

        let input = HookInput {
            event: "PostCompact".to_string(),
            tool_name: None,
            tool_input: Some(serde_json::json!({
                "trigger": trigger,
                "compact_summary": compact_summary
            })),
            tool_output: None,
            tool_use_id: None,
            session_id: None,
            cwd: Some(self.config.cwd.clone()),
            error: None,
            ..HookInput::default()
        };

        let _results = registry.execute("PostCompact", input).await;
    }

    /// Apply compaction result: replace messages with boundary + kept messages
    fn apply_compaction_result(
        &mut self,
        messages_to_keep: Vec<Message>,
        _post_compact_tokens: u32,
    ) {
        let mut boundary_msg = Message {
            role: MessageRole::System,
            content: "[Previous conversation summarized]".to_string(),
            is_meta: Some(true),
            ..Default::default()
        };

        // Annotate boundary with preserved-segment metadata for session storage relinking
        let anchor_uuid = boundary_msg.uuid.clone().unwrap_or_default();
        crate::compact::annotate_boundary_with_preserved_segment(
            &mut boundary_msg,
            &anchor_uuid,
            &messages_to_keep,
        );

        let mut new_messages = vec![boundary_msg];
        new_messages.extend(messages_to_keep);
        self.messages = new_messages;
    }

    pub async fn submit_message(
        &mut self,
        prompt: &str,
    ) -> Result<(String, crate::types::ExitReason), AgentError> {
        self.start_time = Some(std::time::Instant::now());
        // Transition session state to running
        if let Some(ref state) = self.config.session_state {
            state.start_running();
        }
        // Add user message to history
        self.messages.push(crate::types::Message {
            role: crate::types::MessageRole::User,
            content: prompt.to_string(),
            ..Default::default()
        });

        // Prefetch relevant memories and inject into context
        if let Some(memory_context) = build_memory_prefetch_context(prompt, &self.config, &self.loaded_nested_memory_paths).await {
            self.messages.push(crate::types::Message {
                role: crate::types::MessageRole::User,
                content: memory_context,
                ..Default::default()
            });
        }

        // Handle orphaned permission (only once per engine lifetime).
        // Matches TypeScript QueryEngine.ts:397-408 where handleOrphanedPermission
        // runs before the main query loop guarded by hasHandledOrphanedPermission.
        if !self.has_handled_orphaned_permission {
            if let Some(ref orphaned) = self.config.orphaned_permission {
                self.has_handled_orphaned_permission = true;

                // 1. Push the assistant message (containing the tool_use) to history
                // Check if it's already present to avoid duplicates on CCR resume
                let already_present = self.messages.iter().any(|m| {
                    m.role == crate::types::MessageRole::Assistant
                        && m.tool_calls.as_ref().is_some_and(|tc| {
                            tc.iter().any(|tc| tc.id == orphaned.tool_use_id)
                        })
                });
                if !already_present {
                    self.messages.push(orphaned.assistant_message.clone());
                }

                // 2. Generate a synthetic tool result message with the permission decision
                let result_content = match &orphaned.permission_result {
                    crate::permission::PermissionResult::Allow(_) => {
                        format!("Tool call {} is allowed", orphaned.tool_use_id)
                    }
                    crate::permission::PermissionResult::Deny(deny) => {
                        format!("Tool call {} is denied: {}", orphaned.tool_use_id, deny.message)
                    }
                    crate::permission::PermissionResult::Ask(ask) => {
                        format!(
                            "Tool call {} requires confirmation: {}",
                            orphaned.tool_use_id, ask.message
                        )
                    }
                    crate::permission::PermissionResult::Passthrough { message, .. } => {
                        format!("Tool call {} passed through: {}", orphaned.tool_use_id, message)
                    }
                };

                self.messages.push(crate::types::Message {
                    role: crate::types::MessageRole::Tool,
                    content: result_content,
                    tool_call_id: Some(orphaned.tool_use_id.clone()),
                    ..Default::default()
                });

                log::debug!(
                    "Handled orphaned permission for tool_use_id={}",
                    orphaned.tool_use_id
                );
            }
        }

        // Note: max_turns check is done AFTER turn completes (matching TypeScript)
        // See below after tool execution loop for the check

        // Emit Thinking event for the first turn before the first API call
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Thinking { turn: 1 });
        }
        self.turn_count = 1;

        // Tool call loop - continue until no more tool calls
        // Use config.max_turns as the limit (0xffffffff = effectively unlimited)
        //
        // Matching TypeScript query.ts flow:
        // Each iteration runs: snip → microcompact → context collapse → auto-compact → API call
        let mut max_tool_turns = self.config.max_turns;
        while max_tool_turns > 0 {
            max_tool_turns -= 1;

            // Reset compacted flag for this iteration (TypeScript: tracking.compacted = false
            // is implicit via state update at top of loop)
            self.auto_compact_tracking.compacted = false;

            // --- Compaction pipeline (matches TypeScript query.ts order) ---

            // 1. Snip compact (TypeScript: snipModule!.snipCompactIfNeeded)
            let snip_result = crate::services::compact::snip_compact_if_known(&self.messages);
            // Update messages reference if snip returned modified messages
            // (snip_compact_if_known currently returns &self.messages unchanged,
            // but we capture tokens_freed for the threshold adjustment)
            let snip_tokens_freed = snip_result.tokens_freed;

            // 2. Microcompact (TypeScript: deps.microcompact)
            crate::services::compact::microcompact::microcompact_messages(&mut self.messages);

            // 3. Context collapse (TypeScript: contextCollapse.applyCollapsesIfNeeded)
            // Runs BEFORE auto-compact so that if collapse gets us under the
            // auto-compact threshold, auto-compact is a no-op and we keep
            // granular context instead of a single summary.
            if crate::services::context_collapse::is_context_collapse_enabled() {
                let collapse_result = crate::services::context_collapse::apply_collapses_if_needed(
                    self.messages.clone(),
                );
                if collapse_result.changed {
                    self.messages = collapse_result.messages;
                }
            }

            // 4. Auto-compact check (TypeScript: deps.autocompact)
            // Only attempt if:
            // 1. Not disabled by circuit breaker (max 3 consecutive failures)
            // 2. Token count exceeds auto-compact threshold
            //
            // do_auto_compact internally checks token count vs threshold
            // (adjusted by snip_tokens_freed), so it returns Ok(false) when
            // compaction is not needed.
            if self.auto_compact_tracking.consecutive_failures < 3 {
                let token_estimate = compact::estimate_token_count(&self.messages, self.config.max_tokens);
                let threshold = get_auto_compact_threshold(&self.config.model);

                if token_estimate > threshold {
                    // Capture pre-compact token count for task budget
                    let pre_compact_tokens = token_estimate;
                    match self.do_auto_compact(snip_tokens_freed).await {
                        Ok(Some(compact_summary)) => {
                            // Compaction succeeded — reset tracking state (matching TypeScript)
                            self.auto_compact_tracking.compacted = true;
                            self.auto_compact_tracking.turn_id = uuid::Uuid::new_v4().to_string();
                            self.auto_compact_tracking.turn_counter = 0;
                            self.auto_compact_tracking.consecutive_failures = 0;

                            // task_budget: decrement remaining by pre-compact final context
                            if self.config.task_budget.is_some() {
                                let pre_ctx = pre_compact_tokens as u64;
                                let current = self.task_budget_remaining
                                    .or(self.config.task_budget.as_ref().map(|tb| tb.total));
                                self.task_budget_remaining = Some(current.unwrap_or(0).saturating_sub(pre_ctx));
                            }

                            // Emit CompactEnd with summary (exactly once, matches TypeScript finally)
                            if let Some(ref cb) = self.config.on_event {
                                cb(AgentEvent::Compact {
                                    event: CompactProgressEvent::CompactEnd {
                                        message: Some(compact_summary),
                                    },
                                });
                            }
                        }
                        Ok(None) => {
                            // No compaction needed — no event emitted
                        }
                        Err(e) => {
                            // Compaction failed — propagate failure count so the circuit breaker
                            // can stop retrying on the next iteration (matching TypeScript)
                            self.auto_compact_tracking.consecutive_failures += 1;
                            eprintln!("Auto-compact failed: {}", e);
                            if let Some(ref cb) = self.config.on_event {
                                cb(AgentEvent::Compact {
                                    event: CompactProgressEvent::CompactEnd {
                                        message: Some(format!("Compaction failed: {}", e)),
                                    },
                                });
                            }
                        }
                    }
                }
            }

            // Build messages for API
            let api_messages = self.build_api_messages()?;

            // Get API configuration
            let api_key: String = self
                .config
                .api_key
                .clone()
                .ok_or_else(|| AgentError::Api("API key not provided".to_string()))?;

            let base_url = self
                .config
                .base_url
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("https://api.anthropic.com");

            // Use current model, or fallback model if set
            let current_model = if let Some(ref fallback) = self.config.fallback_model {
                fallback.clone()
            } else {
                self.config.model.clone()
            };
            let model = &current_model;

            // Build request with tools if available
            // Always use streaming for all backends (matching TypeScript behavior)
            // Non-streaming fallback will be used if streaming fails
            // Resolve max_tokens: retry override > config override > model-based default
            let effective_max_tokens = self
                .max_output_tokens_override
                .unwrap_or_else(|| {
                    crate::utils::context::get_max_output_tokens_for_model(model) as u32
                });
            let mut request_body = serde_json::json!({
                "model": model,
                "max_tokens": effective_max_tokens,
                "messages": api_messages,
                "stream": true
            });

            // Add task_budget to output_config when configured (API beta: task-budgets-2026-03-13)
            if self.config.task_budget.is_some() {
                let tb = self.config.task_budget.as_ref().unwrap();
                let mut task_budget_obj = serde_json::json!({
                    "type": "tokens",
                    "total": tb.total,
                });
                if let Some(remaining) = self.task_budget_remaining {
                    task_budget_obj["remaining"] = serde_json::json!(remaining);
                }
                request_body["output_config"] = serde_json::json!({
                    "task_budget": task_budget_obj,
                });
            }

            // Add system prompt to request body (Anthropic uses separate field)
            // Include system_context if configured (matching TypeScript appendSystemContext)
            let system_prompt_to_use = if !self.config.system_context.is_empty() {
                let context_parts: Vec<String> = self
                    .config
                    .system_context
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value))
                    .collect();
                let context_str = context_parts.join("\n");

                if let Some(ref system_prompt) = self.config.system_prompt {
                    Some(format!("{}\n\n{}", system_prompt, context_str))
                } else {
                    Some(context_str)
                }
            } else {
                self.config.system_prompt.clone()
            };

            if let Some(ref sp) = system_prompt_to_use {
                request_body["system"] = serde_json::json!(sp);
            }

            // Add thinking config to request (matching TypeScript behavior)
            // Only for Anthropic API and when thinking is not disabled
            if base_url.contains("anthropic.com") {
                if let Some(ref thinking_config) = self.config.thinking {
                    match thinking_config {
                        crate::types::api_types::ThinkingConfig::Adaptive => {
                            request_body["thinking"] = serde_json::json!({
                                "type": "adaptive"
                            });
                        }
                        crate::types::api_types::ThinkingConfig::Enabled { budget_tokens } => {
                            // Clamp thinking budget to max_output_tokens - 1 (TS: claude.ts:1624)
                            let clamped_budget = std::cmp::min(
                                effective_max_tokens.saturating_sub(1) as u32,
                                *budget_tokens,
                            );
                            request_body["thinking"] = serde_json::json!({
                                "type": "enabled",
                                "budget_tokens": clamped_budget
                            });
                        }
                        crate::types::api_types::ThinkingConfig::Disabled => {
                            // Don't add thinking - it's disabled
                        }
                    }
                } else {
                    // Default: use adaptive thinking (matches TypeScript shouldEnableThinkingByDefault)
                    request_body["thinking"] = serde_json::json!({
                        "type": "adaptive"
                    });
                }
            }

            // Add tools to request if we have any
            // Handle deferred tool loading: separate upfront vs deferred tools
            if !self.config.tools.is_empty() {
                let use_anthropic_format = base_url.contains("anthropic.com");

                // Determine which tools to send upfront vs defer
                let (upfront_tools, deferred_tools) = self.separate_tools_for_request();

                // Send only upfront tools in the tools array
                // Deferred tools are discovered via ToolSearchTool
                let tools_to_send = if upfront_tools.is_empty() {
                    // If no upfront tools, still send ToolSearchTool if available
                    &upfront_tools
                } else {
                    &upfront_tools
                };

                let tools: Vec<serde_json::Value> = tools_to_send
                    .iter()
                    .map(|t| {
                        if use_anthropic_format {
                            serde_json::json!({
                                "type": "function",
                                "name": t.name,
                                "description": t.description,
                                "input_schema": t.input_schema
                            })
                        } else {
                            serde_json::json!({
                                "type": "function",
                                "function": {
                                    "name": t.name,
                                    "description": t.description,
                                    "parameters": t.input_schema
                                }
                            })
                        }
                    })
                    .collect();
                request_body["tools"] = serde_json::json!(tools);

                // Store deferred tools info for <available-deferred-tools> injection
                if !deferred_tools.is_empty()
                    && crate::tools::deferred_tools::is_tool_search_enabled_optimistic()
                {
                    // The <available-deferred-tools> block is injected as a synthetic user message
                    // This is handled in build_api_messages()
                    let _deferred_names: Vec<&str> =
                        deferred_tools.iter().map(|t| t.name.as_str()).collect();
                }
            }

            // Determine API endpoint and auth format based on backend
            // Anthropic uses /v1/messages, OpenAI-compatible uses /v1/chat/completions
            let url = if base_url.contains("anthropic.com") {
                format!("{}/v1/messages", base_url)
            } else {
                format!("{}/v1/chat/completions", base_url)
            };

            // Track if we need to fallback to alternate model
            // Matching TypeScript's attemptWithFallback logic
            let mut attempt_with_fallback = false;
            let mut streaming_result: StreamingResult;

            // Model fallback loop - try primary model first, then fallback if rate limited
            loop {
                // Use fallback model if primary failed
                let model_in_loop = if attempt_with_fallback {
                    self.config
                        .fallback_model
                        .as_ref()
                        .unwrap_or(&self.config.model)
                        .clone()
                } else {
                    self.config.model.clone()
                };

                // Update request body with current model
                request_body["model"] = serde_json::json!(model_in_loop);

                // Check if non-streaming fallback is disabled (matching TypeScript)
                if is_nonstreaming_fallback_disabled() {
                    return Err(AgentError::Api(
                        "Non-streaming fallback disabled".to_string(),
                    ));
                }

                // Make API request with 429/529 retry and exponential backoff.
                // Wraps the full streaming→non-streaming fallback flow.
                let retry_result = make_api_request_with_429_retry(
                    &self.http_client,
                    &url,
                    &api_key,
                    request_body.clone(),
                    self.config.on_event.clone(),
                    self.config.fallback_model.clone(),
                    &model_in_loop,
                    match self.config.thinking {
                        Some(crate::types::api_types::ThinkingConfig::Enabled { budget_tokens }) => Some(budget_tokens),
                        _ => None,
                    },
                )
                .await;

                match retry_result {
                    RetryResult::Success(result) => {
                        streaming_result = result;
                        break;
                    }
                    RetryResult::FallbackTriggered(fb_error) => {
                        // Only trigger once (attempt_with_fallback guard)
                        if attempt_with_fallback {
                            // Already attempted fallback, treat as terminal
                            // Add orphaned tool results before terminal error
                            self.add_orphaned_tool_results(&fb_error.to_string());

                            // Fire StopFailure hooks (fire-and-forget)
                            {
                                let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                                if let Some(registry) = registry_clone {
                                    let _ = crate::hooks::run_stop_failure_hooks(
                                        &registry,
                                        &fb_error.to_string(),
                                        &self.config.cwd,
                                    ).await;
                                }
                            }
                            return Err(AgentError::Api(fb_error.to_string()));
                        }

                        attempt_with_fallback = true;

                        // Yield missing tool result blocks for any orphaned tool_use
                        self.add_orphaned_tool_results("Model fallback triggered");

                        // Clear assistant message state for retry
                        // (remove the last assistant message that had the failed tool calls)
                        if let Some(last) = self.messages.last() {
                            if last.role == crate::types::MessageRole::Assistant {
                                self.messages.pop();
                            }
                        }

                        // Update config model to fallback
                        self.config.model = fb_error.fallback_model.clone();

                        // Emit warning about model switch
                        eprintln!(
                            "Switched to {} due to high demand for {}",
                            fb_error.fallback_model, fb_error.original_model
                        );

                        continue; // Retry with fallback model
                    }
                    RetryResult::RecreateClient(recreate_err) => {
                        // Rebuild HTTP client and retry
                        self.http_client = reqwest::Client::new();
                        emit_api_retry_event(
                            self.config.on_event.as_ref().map(|a| a.as_ref()),
                            1,
                            MAX_429_RETRIES,
                            500,
                            None,
                            &format!("Recreating client after: {}", recreate_err),
                        );
                        sleep_tokio(std::time::Duration::from_millis(500)).await;
                        continue; // Retry with fresh client
                    }
                    RetryResult::Terminal(e) => {
                        // Handle user abort
                        if is_user_abort_error(&e) {
                            return Err(AgentError::UserAborted);
                        }

                        // Check for 404 stream creation error
                        if is_404_stream_creation_error(&e) {
                            eprintln!(
                                "Streaming endpoint returned 404, falling back to non-streaming mode"
                            );
                        }

                        // Check if this is a prompt-too-long error
                        let error_str = e.to_string().to_lowercase();
                        let is_prompt_too_long = error_str.contains("413")
                            || error_str.contains("prompt_too_long")
                            || error_str.contains("prompt too long")
                            || error_str.contains("media too large");

                        // --- Context collapse drain stage (before reactive compact) ---
                        // Matches TypeScript: before reactive compact, try context collapse
                        // recoverFromOverflow if transition is not collapse_drain_retry.
                        if is_prompt_too_long
                            && crate::services::context_collapse::is_context_collapse_enabled()
                            && self.transition.as_deref() != Some("collapse_drain_retry")
                        {
                            let original_len = self.messages.len();
                            let drained = crate::services::context_collapse::recover_from_overflow(
                                self.messages.clone(),
                            );
                            // If the collapse function changed anything, use the result
                            if drained.len() < original_len {
                                self.messages = drained;
                                self.transition = Some("collapse_drain_retry".to_string());
                                continue; // Retry after collapse drain
                            }
                        }

                        if is_prompt_too_long {
                            eprintln!("Prompt too large (413), attempting reactive compact...");
                            // Emit CompactStart progress event
                            if let Some(ref cb) = self.config.on_event {
                                cb(AgentEvent::Compact {
                                    event: CompactProgressEvent::CompactStart,
                                });
                            }
                            let _pre_compact_instructions = self.execute_pre_compact_hooks().await;
                            match crate::services::compact::reactive_compact::run_reactive_compact(
                                &self.messages,
                                &self.config.model,
                            ) {
                                Ok(reactive_result) if reactive_result.compacted => {
                                    log::info!(
                                        "[reactive-compact] reduced {} messages after 413 error",
                                        reactive_result.messages.len()
                                    );
                                    // task_budget: decrement remaining by pre-compact final context
                                    if self.config.task_budget.is_some() {
                                        let pre_ctx = crate::compact::estimate_token_count(&self.messages, 0) as u64;
                                        let current = self.task_budget_remaining
                                            .or(self.config.task_budget.as_ref().map(|tb| tb.total));
                                        self.task_budget_remaining = Some(current.unwrap_or(0).saturating_sub(pre_ctx));
                                    }
                                    let reactive_msg_count = reactive_result.messages.len();
                                    self.messages = reactive_result.messages;
                                    // Post-compaction bookkeeping (matches TypeScript)
                                    crate::bootstrap::state::mark_post_compaction();
                                    let _ = crate::services::api::prompt_cache_break_detection::notify_compaction(
                                        "repl_main_thread",
                                        self.config.agent_id.as_deref(),
                                    );
                                    crate::services::compact::run_post_compact_cleanup(Some("repl_main_thread"));
                                    // Re-append session metadata (matches TypeScript)
                                    crate::compact::re_append_session_metadata(
                                        self.config.agent_id.as_deref().unwrap_or(""),
                                        &self.config.model,
                                        &self.config.cwd,
                                        None,
                                        None,
                                    );
                                    self.execute_post_compact_hooks("Reactive compact applied after 413 error").await;
                                    // Emit CompactEnd progress event
                                    if let Some(ref cb) = self.config.on_event {
                                        cb(AgentEvent::Compact {
                                            event: CompactProgressEvent::CompactEnd {
                                                message: Some(format!(
                                                    "[reactive-compact] reduced to {} messages after 413 error",
                                                    reactive_msg_count
                                                )),
                                            },
                                        });
                                    }
                                    self.transition = Some("reactive_compact_retry".to_string());
                                    continue; // Retry with compacted context
                                }
                                _ => {
                                    log::warn!(
                                        "[reactive-compact] no improvement possible, falling through"
                                    );
                                }
                            }
                            // Emit CompactEnd for failure path
                            if let Some(ref cb) = self.config.on_event {
                                cb(AgentEvent::Compact {
                                    event: CompactProgressEvent::CompactEnd { message: None },
                                });
                            }
                            // Reactive compact didn't help - this is terminal
                            // Add orphaned tool results before terminal error
                            self.add_orphaned_tool_results(&e.to_string());

                            // Fire StopFailure hooks (fire-and-forget, matches TypeScript)
                            {
                                let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                                if let Some(registry) = registry_clone {
                                    let _ = crate::hooks::run_stop_failure_hooks(&registry, &e.to_string(), &self.config.cwd).await;
                                }
                            }
                            return Err(e);
                        }

                        // Add orphaned tool results before terminal error
                        self.add_orphaned_tool_results(&e.to_string());

                        // Fire StopFailure hooks (fire-and-forget, matches TypeScript)
                        {
                            let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                            if let Some(registry) = registry_clone {
                                let _ = crate::hooks::run_stop_failure_hooks(&registry, &e.to_string(), &self.config.cwd).await;
                            }
                        }
                        return Err(e);
                    }
                }
            }

            // Emit StreamRequestEnd — TUI can use this to hide spinner after API response
            if let Some(ref cb) = self.config.on_event {
                cb(AgentEvent::StreamRequestEnd);
            }

            // Check for refusal before max_output_tokens check (matches TypeScript)
            if let Some(refusal_msg) = get_error_message_if_refusal(
                streaming_result.stop_reason.as_deref(),
                &self.config.model,
                false, // is_non_interactive
            ) {
                // Add the refusal as an API error message
                self.messages.push(crate::types::Message {
                    role: crate::types::MessageRole::Assistant,
                    content: refusal_msg.content.clone().unwrap_or_default(),
                    is_api_error_message: Some(true),
                    error_details: refusal_msg.error_details.clone(),
                    ..Default::default()
                });
                // Fire StopFailure hooks
                {
                    let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                    if let Some(registry) = registry_clone {
                        let _ = crate::hooks::run_stop_failure_hooks(
                            &registry,
                            &refusal_msg.content.as_ref().map(|s| s.as_str()).unwrap_or("refusal"),
                            &self.config.cwd,
                        ).await;
                    }
                }
                return Err(AgentError::Api(
                    refusal_msg.content.unwrap_or_else(|| "Refusal".to_string()),
                ));
            }

            // Execute post-sampling hooks after model response is complete
            // (matches TypeScript executePostSamplingHooks in query.ts:999-1008)
            if !streaming_result.content.is_empty() || !streaming_result.tool_calls.is_empty() {
                let hook_messages = self.messages.clone();
                let hook_system_prompt = self
                    .config
                    .system_prompt
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                let hook_user_context = self.config.user_context.clone();
                let hook_system_context = self.config.system_context.clone();
                let hook_tool_use_context = Arc::new(
                    crate::utils::hooks::can_use_tool::ToolUseContext {
                        session_id: self
                            .config
                            .session_state
                            .as_ref()
                            .map(|_| "query_engine".to_string())
                            .unwrap_or_else(|| "query_engine".to_string()),
                        cwd: Some(self.config.cwd.clone()),
                        is_non_interactive_session: false,
                        options: None,
                    },
                );
                let hook_query_source = self.config.agent_id.as_ref().map(|_| "agent".to_string());
                let has_hook_count = {
                    crate::utils::hooks::get_post_sampling_hook_count() > 0
                };
                if has_hook_count {
                    let messages_clone = hook_messages;
                    let system_prompt_clone = hook_system_prompt;
                    let user_context_clone = hook_user_context;
                    let system_context_clone = hook_system_context;
                    let tool_use_context_clone = hook_tool_use_context;
                    let query_source_clone = hook_query_source;
                    tokio::spawn(async move {
                        crate::utils::hooks::execute_post_sampling_hooks(
                            messages_clone,
                            system_prompt_clone,
                            user_context_clone,
                            system_context_clone,
                            tool_use_context_clone,
                            query_source_clone,
                        )
                        .await;
                    });
                }
            }

            // Check for tool calls in the streaming result
            if streaming_result.tool_calls.is_empty() {
                // Check for max_output_tokens error and handle recovery
                // Two-phase recovery matching TypeScript query.ts:1188-1256
                if streaming_result.api_error.as_deref() == Some("max_output_tokens") {
                    const MAX_OUTPUT_TOKENS_RECOVERY_LIMIT: u32 = 3;
                    const ESCALATED_MAX_TOKENS: u64 = 64_000;

                    // Phase 1: Escalation (TS: query.ts:1188-1221)
                    // If no override set and no env var, escalate to 64k and retry same request
                    // Feature 'tengu_otk_slot_v1' always enabled per CLAUDE.md
                    if self.max_output_tokens_override.is_none()
                        && std::env::var(crate::constants::env::ai_code::MAX_OUTPUT_TOKENS).is_err()
                    {
                        self.max_output_tokens_override = Some(ESCALATED_MAX_TOKENS as u32);
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Thinking {
                                turn: self.turn_count + 1,
                            });
                        }
                        continue;
                    }

                    // Phase 2: Multi-turn recovery (TS: query.ts:1223-1252)
                    // Inject meta recovery message, reset override to default tokens
                    if self.max_output_tokens_recovery_count < MAX_OUTPUT_TOKENS_RECOVERY_LIMIT {
                        let recovery_message = crate::types::Message {
                            role: crate::types::MessageRole::User,
                            content: "Output token limit hit. Resume directly — no apology, no recap of what you were doing. Pick up mid-thought if that is where the cut happened. Break remaining work into smaller pieces.".to_string(),
                            is_meta: Some(true),
                            ..Default::default()
                        };
                        self.messages.push(recovery_message);
                        // Reset override so we go back to model-based default
                        self.max_output_tokens_override = None;
                        self.max_output_tokens_recovery_count += 1;

                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Thinking {
                                turn: self.turn_count + 1,
                            });
                        }
                        continue;
                    }

                    // Recovery exhausted - return the error as final response
                    if let Some(ref cb) = self.config.on_event {
                        cb(AgentEvent::Done {
                            result: crate::types::QueryResult {
                                text: "Output token limit reached and recovery exhausted"
                                    .to_string(),
                                usage: self.total_usage.clone(),
                                num_turns: self.turn_count,
                                duration_ms: self.query_duration_ms(),
                                exit_reason: crate::types::ExitReason::MaxTokens,
                            },
                        });
                    }
                    return Ok((
                        "Output token limit reached and recovery exhausted".to_string(),
                        crate::types::ExitReason::MaxTokens,
                    ));
                }

                // No tool calls - check for unfinished tasks before finalizing
                if self.config.max_turns == 0 || self.turn_count < self.config.max_turns {
                    if let Some(nudge) = crate::utils::inspector::check() {
                        log::debug!(
                            "[query_engine] unfinished tasks found, nudging LLM to continue (turn {})",
                            self.turn_count
                        );
                        self.messages.push(crate::types::Message {
                            role: crate::types::MessageRole::System,
                            content: nudge,
                            ..Default::default()
                        });
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Thinking {
                                turn: self.turn_count + 1,
                            });
                        }
                        self.turn_count += 1;
                        continue;
                    }
                }

                // No tool calls - this is the final response
                let response_text = streaming_result.content.clone();

                // Don't strip thinking from result.text - preserve it for history
                // The thinking will still be shown during streaming via streaming_text

                // If both content and tool_calls are empty, the API response was empty.
                // This can happen from rate limiting, network issues, or model errors
                // that slip past HTTP status checks (e.g., 200 OK with error body,
                // or stream dropped mid-response). Retry a couple of times.
                if response_text.is_empty()
                    && streaming_result.tool_calls.is_empty()
                    && self.config.max_turns > 0
                    && self.turn_count < self.config.max_turns
                {
                    self.empty_response_retries += 1;
                    if self.empty_response_retries <= 2 {
                        log::warn!(
                            "[query_engine] empty model response, retrying ({}) stop_reason={:?}",
                            self.empty_response_retries,
                            streaming_result.stop_reason,
                        );
                        // Brief backoff between retries
                        tokio::time::sleep(std::time::Duration::from_millis(
                            500 * self.empty_response_retries as u64,
                        ))
                        .await;
                        // Continue to rebuild and retry the API call
                        continue;
                    }
                    self.empty_response_retries = 0;
                } else {
                    self.empty_response_retries = 0;
                }

                // If both content and tool_calls are empty, it's a genuine error
                if response_text.is_empty() && streaming_result.tool_calls.is_empty() {
                    log::error!(
                        "[query_engine] model returned empty response after retries: stop_reason={:?}",
                        streaming_result.stop_reason,
                    );
                    return Err(AgentError::Api(
                        "Model response contained no text and no tool calls".to_string(),
                    ));
                }

                let final_text = response_text.clone();

                // Update total usage (matching TypeScript usage tracking)
                self.total_usage.input_tokens += streaming_result.usage.input_tokens;
                self.total_usage.output_tokens += streaming_result.usage.output_tokens;
                self.total_usage.cache_creation_input_tokens = Some(
                    self.total_usage.cache_creation_input_tokens.unwrap_or(0)
                        + streaming_result.usage.cache_creation_input_tokens.unwrap_or(0),
                );
                self.total_usage.cache_read_input_tokens = Some(
                    self.total_usage.cache_read_input_tokens.unwrap_or(0)
                        + streaming_result.usage.cache_read_input_tokens.unwrap_or(0),
                );
                self.turn_tokens += streaming_result.usage.output_tokens as u64;

                // Update total cost (matching TypeScript cost tracking)
                self.total_cost += streaming_result.cost;

                // Check if USD budget has been exceeded
                if let Some(max_budget) = self.config.max_budget_usd {
                    if self.total_cost >= max_budget {
                        let final_text = self.messages.iter()
                            .rev()
                            .find(|m| matches!(m.role, crate::types::MessageRole::Assistant))
                            .map(|m| m.content.clone())
                            .unwrap_or_default();
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Done {
                                result: crate::types::QueryResult {
                                    text: final_text.clone(),
                                    usage: self.total_usage.clone(),
                                    num_turns: self.turn_count,
                                    duration_ms: self.query_duration_ms(),
                                    exit_reason: crate::types::ExitReason::MaxBudgetExceeded {
                                        max_budget_usd: max_budget,
                                    },
                                },
                            });
                        }
                        return Ok((
                            final_text,
                            crate::types::ExitReason::MaxBudgetExceeded {
                                max_budget_usd: max_budget,
                            },
                        ));

                    }
                }

                // Update global cost state for session-level reporting
                let model = self.config.model.clone();
                let _ = crate::services::model_cost::add_to_total_session_cost(
                    streaming_result.cost,
                    streaming_result.usage.input_tokens as u32,
                    streaming_result.usage.output_tokens as u32,
                    streaming_result.usage.cache_read_input_tokens.unwrap_or(0) as u32,
                    streaming_result.usage.cache_creation_input_tokens.unwrap_or(0) as u32,
                    0,
                    &model,
                );

                // Add assistant response to message history
                self.messages.push(crate::types::Message {
                    role: crate::types::MessageRole::Assistant,
                    content: response_text.clone(),
                    ..Default::default()
                });

                // Reset recovery count on successful completion
                self.max_output_tokens_recovery_count = 0;
                self.max_output_tokens_override = None;

                // Check max_turns limit BEFORE incrementing (TypeScript checks nextTurnCount before increment)
                let next_turn_count = self.turn_count + 1;
                if self.config.max_turns > 0 && next_turn_count > self.config.max_turns {
                    // Emit max_turns_reached event (matches TypeScript behavior)
                    // Emit Done event (matches TypeScript yielding { type: 'result' })
                    if let Some(ref cb) = self.config.on_event {
                        cb(AgentEvent::MaxTurnsReached {
                            max_turns: self.config.max_turns,
                            turn_count: next_turn_count,
                        });
                        cb(AgentEvent::Done {
                            result: crate::types::QueryResult {
                                text: final_text.clone(),
                                usage: self.total_usage.clone(),
                                num_turns: self.turn_count,
                                duration_ms: self.query_duration_ms(),
                                exit_reason: crate::types::ExitReason::MaxTurns {
                                    max_turns: self.config.max_turns,
                                    turn_count: next_turn_count,
                                },
                            },
                        });
                    }
                    // Return what we have, don't exceed max turns
                    return Ok((
                        final_text,
                        crate::types::ExitReason::MaxTurns {
                            max_turns: self.config.max_turns,
                            turn_count: next_turn_count,
                        },
                    ));
                }

                // Increment turn_count AFTER tool execution (matches TypeScript behavior)
                self.turn_count = next_turn_count;

                // Fire Stop hooks before finalizing (matches TypeScript handleStopHooks)
                // Short-circuit: if the last assistant message is an API error message,
                // skip stop hooks to avoid the death spiral:
                // error → hook blocking → retry → error → …
                let last_is_api_error = self.messages.iter().rev().find_map(|m| {
                    if m.role == crate::types::MessageRole::Assistant {
                        Some(m.is_api_error_message == Some(true))
                    } else {
                        None
                    }
                }).unwrap_or(false);

                if !self.stop_hook_active && !last_is_api_error {
                    self.stop_hook_active = true;
                    let stop_result = {
                        let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                        if let Some(registry) = registry_clone {
                            crate::hooks::run_stop_hooks(&registry, &self.config.cwd, &final_text).await
                        } else {
                            crate::hooks::StopHookResult::default()
                        }
                    };

                    // Memory extraction (fire-and-forget, matches TypeScript EXTRACT_MEMORIES feature)
                    // Only for main agent (no agent_id), not subagents
                    if self.config.agent_id.is_none() {
                        let messages: Vec<crate::types::message::Message> = self.messages
                            .iter()
                            .filter_map(|m| match serde_json::to_value(m) {
                                Ok(v) => serde_json::from_value(v).ok(),
                                Err(_) => None,
                            })
                            .collect();
                        let extract_ctx = crate::services::extract_memories::ExtractMemoryContext {
                            messages,
                            system_prompt: self.config
                                .system_prompt
                                .as_deref()
                                .unwrap_or("")
                                .to_string(),
                            user_context: self.config.user_context.clone(),
                            system_context: self.config.system_context.clone(),
                            tool_use_context: None,
                            agent_id: self.config.agent_id.clone(),
                        };
                        let ctx_clone = extract_ctx.clone();
                        tokio::spawn(async move {
                            crate::services::extract_memories::execute_extract_memories(
                                ctx_clone,
                                None,
                            )
                            .await;
                        });
                    }

                    if !stop_result.blocking_errors.is_empty() {
                        // Inject blocking errors as system messages and re-query
                        for err_msg in stop_result.blocking_errors {
                            self.messages.push(crate::types::Message {
                                role: crate::types::MessageRole::System,
                                content: err_msg,
                                ..Default::default()
                            });
                        }
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Thinking {
                                turn: self.turn_count + 1,
                            });
                        }
                        continue;
                    }
                    if stop_result.prevent_continuation {
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Done {
                                result: crate::types::QueryResult {
                                    text: final_text.clone(),
                                    usage: self.total_usage.clone(),
                                    num_turns: self.turn_count,
                                    duration_ms: self.query_duration_ms(),
                                    exit_reason: crate::types::ExitReason::Completed,
                                },
                            });
                        }
                        return Ok((final_text, crate::types::ExitReason::Completed));
                    }
                }

                // Emit Thinking event for next turn
                if let Some(ref cb) = self.config.on_event {
                    cb(AgentEvent::Thinking {
                        turn: self.turn_count + 1,
                    });
                }

                // Check token budget (TOKEN_BUDGET feature)
                // When a token budget is set, we continue the loop with a nudge message
                // until we reach 90% of the budget or hit diminishing returns.
                // Snapshot output tokens at turn start for per-turn budget tracking
                crate::bootstrap::state::snapshot_output_tokens_for_turn(self.config.token_budget);
                let token_budget = self.config.token_budget;
                let agent_id = self.config.agent_id.clone();
                match crate::token_budget::check_token_budget(
                    &mut self.budget_tracker,
                    agent_id.as_deref(),
                    token_budget,
                    self.turn_tokens,
                ) {
                    crate::token_budget::TokenBudgetDecision::Continue { nudge_message } => {
                        // Inject nudge as synthetic user message and re-query
                        self.messages.push(crate::types::Message {
                            role: crate::types::MessageRole::User,
                            content: nudge_message,
                            ..Default::default()
                        });
                        self.transition = Some("token_budget_continuation".to_string());
                        continue;
                    }
                    crate::token_budget::TokenBudgetDecision::Stop { .. } => {
                        // Normal exit path
                    }
                }

                // Validate result before emitting (matches TypeScript isResultSuccessful check at QueryEngine.ts:1082)
                let last_stop_reason = streaming_result.stop_reason.as_deref();
                if !self.is_result_successful(last_stop_reason) {
                    let error_detail = format!(
                        "Invalid result state: last_message_type={:?}, stop_reason={:?}",
                        self.messages.last().map(|m| &m.role),
                        last_stop_reason
                    );
                    if let Some(ref cb) = self.config.on_event {
                        cb(AgentEvent::Done {
                            result: crate::types::QueryResult {
                                text: final_text.clone(),
                                usage: self.total_usage.clone(),
                                num_turns: self.turn_count,
                                duration_ms: self.query_duration_ms(),
                                exit_reason: crate::types::ExitReason::ModelError { error: error_detail.clone() },
                            },
                        });
                    }
                    return Ok((final_text, crate::types::ExitReason::ModelError { error: error_detail.clone() }));
                }

                // Emit Done event (matches TypeScript yielding { type: 'result' })
                if let Some(ref cb) = self.config.on_event {
                    cb(AgentEvent::Done {
                        result: crate::types::QueryResult {
                            text: final_text.clone(),
                            usage: self.total_usage.clone(),
                            num_turns: self.turn_count,
                            duration_ms: self.query_duration_ms(),
                            exit_reason: crate::types::ExitReason::Completed,
                        },
                    });
                }
                // Return the final text (already processed above)
                return Ok((final_text, crate::types::ExitReason::Completed));
            }

            // Process tool calls from streaming result
            let tool_calls = streaming_result.tool_calls;

            // Convert JSON tool calls to ToolCall structs
            let mut tool_call_structs: Vec<crate::types::ToolCall> = Vec::new();
            for tc in &tool_calls {
                let name = tc
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let id = tc
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = tc
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| empty_json_value());
                tool_call_structs.push(crate::types::ToolCall {
                    id,
                    r#type: "function".to_string(),
                    name,
                    arguments,
                });
            }

            // Use orchestration for concurrent/serial tool execution
            // This matches TypeScript's runTools() with partitioning
            let tool_context = crate::types::ToolContext {
                cwd: self.config.cwd.clone(),
                abort_signal: Arc::clone(self.abort_controller.signal()),
            };

            // Create executor closure using the tool executors stored in QueryEngine
            // Wrap in Arc so it can be cloned for concurrent execution
            let tool_executors = Arc::new(self.tool_executors.lock().unwrap().clone());
            let tool_render_fns = Arc::new(self.tool_render_fns.lock().unwrap().clone());
            let tool_backfill_fns = Arc::new(self.tool_backfill_fns.lock().unwrap().clone());
            let tools = self.config.tools.clone();
            let can_use_tool = self.config.can_use_tool.clone();
            let cwd = self.config.cwd.clone();
            let on_event = self.config.on_event.clone();
            let abort_signal = self.abort_controller.signal().clone();
            let hook_registry = self.hook_registry.clone();

            let executor = move |name: String, args: serde_json::Value, tool_call_id: String| {
                let tool_executors = tool_executors.clone();
                let tool_render_fns = tool_render_fns.clone();
                let tool_backfill_fns = tool_backfill_fns.clone();
                let tools = tools.clone();
                let can_use_tool = can_use_tool.clone();
                let cwd = cwd.clone();
                let on_event = on_event.clone();
                let abort_signal = abort_signal.clone();
                let hook_registry = hook_registry.clone();
                async move {
                    // The actual tool execution is now handled by QueryEngine::execute_tool
                    // but since we are in a closure passed to orchestration::run_tools,
                    // we have to implement the logic here or change orchestration.
                    // To keep it consistent with the new execute_tool, we'll mimic its logic.

                    // Backfill observable input (TS: toolExecution.ts:783-792)
                    // Clone args, call backfill on clone, use backfilled for hooks/events
                    // Original args passed to executor_fn (preserves prompt cache)
                    let mut backfilled_args = args.clone();
                    if let Some(backfill_fn) = tool_backfill_fns.get(&name) {
                        backfill_fn(&mut backfilled_args);
                    }

                    // Emit ToolStart event with render metadata (use backfilled input for observers)
                    if let Some(ref cb) = on_event {
                        let meta_input = Some(&backfilled_args);
                        let metadata = tool_render_fns.get(&name).map(|fns| ToolRenderMetadata {
                            user_facing_name: (Arc::clone(&fns.user_facing_name))(meta_input),
                            tool_use_summary: fns
                                .get_tool_use_summary
                                .as_ref()
                                .and_then(|f| f(meta_input)),
                            activity_description: fns
                                .get_activity_description
                                .as_ref()
                                .and_then(|f| f(meta_input)),
                        });
                        if let Some(ref meta) = metadata {
                            cb(AgentEvent::ToolStart {
                                tool_name: name.clone(),
                                tool_call_id: tool_call_id.clone(),
                                input: backfilled_args.clone(),
                                display_name: Some(meta.user_facing_name.clone()),
                                summary: meta.tool_use_summary.clone(),
                                activity_description: meta.activity_description.clone(),
                            });
                        } else {
                            cb(AgentEvent::ToolStart {
                                tool_name: name.clone(),
                                tool_call_id: tool_call_id.clone(),
                                input: backfilled_args.clone(),
                                display_name: None,
                                summary: None,
                                activity_description: None,
                            });
                        }
                    }

                    // We don't have access to `self` here, so we can't call self.execute_tool.
                    // However, the hooks and permissions are part of the config/registry.
                    // For now, let's maintain the logic but ensure we use tool_call_id.

                    let cwd_clone = cwd.clone();

                    let context = crate::types::ToolContext {
                        cwd,
                        abort_signal: abort_signal.clone(),
                    };

                    let executor_fn = tool_executors.get(&name).cloned();

                    if let Some(executor_fn) = executor_fn {
                        // Compute render metadata
                        let meta_input = Some(&args);
                        let metadata = tool_render_fns.get(&name).map(|fns| ToolRenderMetadata {
                            user_facing_name: (Arc::clone(&fns.user_facing_name))(meta_input),
                            tool_use_summary: fns
                                .get_tool_use_summary
                                .as_ref()
                                .and_then(|f| f(meta_input)),
                            activity_description: fns
                                .get_activity_description
                                .as_ref()
                                .and_then(|f| f(meta_input)),
                        });

                        // Pre-tool permission check (3-way: Allow/Deny/Ask) - use backfilled input
                        if let Some(can_use_fn) = can_use_tool {
                            if let Some(tool_def) = tools.iter().find(|t| &t.name == &name) {
                                match can_use_fn(tool_def.clone(), backfilled_args.clone()) {
                                    crate::permission::PermissionResult::Allow(_)
                                    | crate::permission::PermissionResult::Passthrough { .. } => {}
                                    crate::permission::PermissionResult::Deny(d) => {
                                        return Err(crate::error::AgentError::Tool(format!(
                                            "Tool '{}' permission denied: {}",
                                            name, d.message
                                        )));
                                    }
                                    crate::permission::PermissionResult::Ask(a) => {
                                        return Err(crate::error::AgentError::Tool(format!(
                                            "Tool '{}' requires user confirmation (Ask not supported in SDK): {}",
                                            name, a.message
                                        )));
                                    }
                                }
                            }
                        }

                        // PreToolUse hooks (fire before execution, can block) - use backfilled input
                        {
                            let registry_clone = hook_registry.lock().unwrap().as_ref().cloned();
                            if let Some(registry) = registry_clone {
                                if let Err(e) =
                                    crate::hooks::run_pre_tool_use_hooks(&registry, &name, &backfilled_args, &tool_call_id, &cwd_clone)
                                        .await
                                {
                                    return Err(e);
                                }
                            }
                        }

                        // Execute with original args (preserves prompt cache, TS: callInput)
                        let result = executor_fn(args, &context).await;

                        // PostToolUse / PostToolUseFailure hooks
                        {
                            let registry_clone = hook_registry.lock().unwrap().as_ref().cloned();
                            if let Some(registry) = registry_clone {
                                match &result {
                                    Ok(tool_result) => {
                                        let _ = crate::hooks::run_post_tool_use_hooks(&registry, &name, tool_result, &tool_call_id, &cwd_clone).await;
                                    }
                                    Err(e) => {
                                        let _ = crate::hooks::run_post_tool_use_failure_hooks(&registry, &name, &e.to_string(), &tool_call_id, &cwd_clone).await;
                                    }
                                }
                            }
                        }

                        // Emit ToolComplete or ToolError event with render hooks
                        if let Some(ref cb) = on_event {
                            match &result {
                                Ok(tool_result) => {
                                    let rendered_result = tool_render_fns
                                        .get(&name)
                                        .and_then(|fns| fns.render(&tool_result.content, &tools));
                                    if let Some(ref meta) = metadata {
                                        let display = format!(
                                            "{}({})",
                                            meta.user_facing_name,
                                            meta.tool_use_summary.as_deref().unwrap_or("?")
                                        );
                                        cb(AgentEvent::ToolComplete {
                                            tool_name: name.clone(),
                                            tool_call_id: tool_call_id.clone(),
                                            result: tool_result.clone(),
                                            display_name: Some(display),
                                            rendered_result: rendered_result.clone(),
                                        });
                                    } else {
                                        cb(AgentEvent::ToolComplete {
                                            tool_name: name.clone(),
                                            tool_call_id: tool_call_id.clone(),
                                            result: tool_result.clone(),
                                            display_name: None,
                                            rendered_result: rendered_result,
                                        });
                                    }
                                }
                                Err(e) => {
                                    cb(AgentEvent::ToolError {
                                        tool_name: name.clone(),
                                        tool_call_id: tool_call_id.clone(),
                                        error: e.to_string(),
                                    });
                                }
                            }
                        }

                        result
                    } else {
                        let err =
                            crate::error::AgentError::Tool(format!("Tool '{}' not found", name));
                        if let Some(ref cb) = on_event {
                            cb(AgentEvent::ToolError {
                                tool_name: name.clone(),
                                tool_call_id: tool_call_id.clone(),
                                error: err.to_string(),
                            });
                        }
                        Err(err)
                    }
                }
            };

            // Add assistant message with tool_calls to history BEFORE execution
            // This matches TypeScript behavior - the assistant message contains tool call info
            let assistant_msg = crate::types::Message {
                role: crate::types::MessageRole::Assistant,
                content: format!(
                    "Calling tool(s): {:?}",
                    tool_calls
                        .iter()
                        .map(|tc| tc.get("name").and_then(|n| n.as_str()).unwrap_or(""))
                        .collect::<Vec<_>>()
                ),
                tool_calls: Some(tool_call_structs.clone()),
                ..Default::default()
            };
            self.messages.push(assistant_msg);

            let updates = orchestration::run_tools(
                tool_call_structs,
                self.config.tools.clone(),
                tool_context,
                executor,
                Some(self.config.cwd.clone()),
                None,
            )
            .await;

            // Process tool results - matches TypeScript's normalizeMessagesForAPI
            for update in updates {
                if let Some(message) = update.message {
                    // Add tool result message to history
                    // Truncate large tool results to prevent 413 Payload Too Large errors
                    let truncated_content = truncate_tool_result_content(&message.content, "");
                    let mut msg = message;
                    msg.content = truncated_content;
                    self.messages.push(msg);
                }
            }

            // Enforce aggregate tool result budget after tool results are added
            if let Some(ref mut state) = self.content_replacement_state {
                crate::services::compact::apply_tool_result_budget(&mut self.messages, Some(state));
            }

            // After tool execution, check max_turns BEFORE incrementing
            let next_turn_count = self.turn_count + 1;
            if self.config.max_turns > 0 && next_turn_count > self.config.max_turns {
                // Emit max_turns_reached event
                if let Some(ref cb) = self.config.on_event {
                    cb(AgentEvent::MaxTurnsReached {
                        max_turns: self.config.max_turns,
                        turn_count: next_turn_count,
                    });
                }
                // Return what we have, don't exceed max turns
                let final_text = self
                    .messages
                    .iter()
                    .filter(|m| m.role == crate::types::MessageRole::Assistant)
                    .last()
                    .map(|m| m.content.clone())
                    .unwrap_or_else(|| "Max turns reached".to_string());
                // Don't strip thinking - preserve for history
                let final_text = final_text;
                if let Some(ref cb) = self.config.on_event {
                    cb(AgentEvent::Done {
                        result: crate::types::QueryResult {
                            text: final_text.clone(),
                            usage: self.total_usage.clone(),
                            num_turns: self.turn_count,
                            duration_ms: self.query_duration_ms(),
                            exit_reason: crate::types::ExitReason::default(),
                        },
                    });
                }
                return Ok((final_text, crate::types::ExitReason::default()));
            }

            // After tool execution, increment turn count
            // TypeScript increments once per full turn (user msg + assistant + tools)
            self.turn_count = next_turn_count;

            // Post-compaction turn counter tracking (matches TypeScript's tracking.turnCounter++)
            // Only increment if we compacted in the previous turn
            if self.auto_compact_tracking.compacted {
                self.auto_compact_tracking.turn_counter += 1;
            }

            // Emit Thinking event for next turn
            if let Some(ref cb) = self.config.on_event {
                cb(AgentEvent::Thinking {
                    turn: self.turn_count + 1,
                });
            }

            // Continue the loop to get next response
            continue;
        }

        // Max tool turns reached
        let final_text = self
            .messages
            .iter()
            .filter(|m| m.role == crate::types::MessageRole::Assistant)
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_else(|| "Max tool execution turns reached".to_string());

        // Don't strip thinking - preserve for history
        let final_text = final_text;

        // Emit Done event
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Done {
                result: crate::types::QueryResult {
                    text: final_text.clone(),
                    usage: self.total_usage.clone(),
                    num_turns: self.turn_count,
                    duration_ms: self.query_duration_ms(),
                    exit_reason: crate::types::ExitReason::Completed,
                },
            });
        }

        Ok((final_text, crate::types::ExitReason::Completed))
    }

    fn build_api_messages(&self) -> Result<Vec<serde_json::Value>, AgentError> {
        // Determine if this is Anthropic API or OpenAI-compatible
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com");
        let is_anthropic = base_url.contains("anthropic.com");

        // Prepend user context if configured (matching TypeScript prependUserContext)
        let mut all_messages = self.messages.clone();
        if !self.config.user_context.is_empty() {
            let context_parts: Vec<String> = self
                .config
                .user_context
                .iter()
                .map(|(key, value)| format!("# {}\n{}", key, value))
                .collect();
            let context_content = format!(
                "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n{}\n\nIMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it's highly relevant to the work you're doing.\n</system-reminder>\n",
                context_parts.join("\n")
            );
            let context_msg = crate::types::Message {
                role: crate::types::MessageRole::User,
                content: context_content,
                is_meta: Some(true),
                ..Default::default()
            };
            all_messages.insert(0, context_msg);
        }

        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        // Note: System prompt is handled separately in the request body, not in messages array

        for msg in &all_messages {
            match msg.role {
                crate::types::MessageRole::User => {
                    // User message - simple text content
                    api_messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                crate::types::MessageRole::Assistant => {
                    // Assistant message - could have tool_use blocks
                    if let Some(tool_calls) = &msg.tool_calls {
                        if is_anthropic {
                            // Anthropic format: content array with text and tool_use blocks
                            let mut content_blocks: Vec<serde_json::Value> = Vec::new();

                            // Add text content if present
                            if !msg.content.is_empty()
                                && msg.content
                                    != format!(
                                        "Calling tool: {} with args: ",
                                        tool_calls.first().map(|t| t.name.as_str()).unwrap_or("")
                                    )
                            {
                                content_blocks.push(serde_json::json!({
                                    "type": "text",
                                    "text": msg.content
                                }));
                            }

                            // Add tool_use blocks
                            for tc in tool_calls {
                                content_blocks.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": tc.id,
                                    "name": tc.name,
                                    "input": tc.arguments
                                }));
                            }

                            api_messages.push(serde_json::json!({
                                "role": "assistant",
                                "content": content_blocks
                            }));
                        } else {
                            // OpenAI format: separate content and tool_calls fields
                            // Build tool_calls array
                            let mut openai_tool_calls: Vec<serde_json::Value> = Vec::new();
                            for tc in tool_calls {
                                openai_tool_calls.push(serde_json::json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default()
                                    }
                                }));
                            }

                            api_messages.push(serde_json::json!({
                                "role": "assistant",
                                "content": msg.content,
                                "tool_calls": openai_tool_calls
                            }));
                        }
                    } else {
                        // Simple assistant message with text only
                        api_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": msg.content
                        }));
                    }
                }
                crate::types::MessageRole::Tool => {
                    // Tool result message
                    let tool_use_id = msg.tool_call_id.clone().unwrap_or_default();

                    // Build content for tool result
                    let content = if msg.is_error == Some(true) {
                        format!("<tool_use_error>{}</tool_use_error>", msg.content)
                    } else {
                        msg.content.clone()
                    };

                    if is_anthropic {
                        // Anthropic API expects tool_result blocks in a 'user' role message
                        api_messages.push(serde_json::json!({
                            "role": "user",
                            "content": [
                                {
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": content
                                }
                            ]
                        }));
                    } else {
                        // OpenAI-compatible API expects plain text content for tool results
                        api_messages.push(serde_json::json!({
                            "role": "tool",
                            "content": content,
                            "tool_call_id": tool_use_id
                        }));
                    }
                }
                crate::types::MessageRole::System => {
                    // System messages - include as user message per Anthropic
                    api_messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
            }
        }
        // Inject <available-deferred-tools> block if tool search is enabled
        self.maybe_inject_deferred_tools_block(&mut api_messages);

        Ok(api_messages)
    }
}

/// Calculate which messages to keep during compaction
/// Keeps first few messages (system prompt context) and recent messages
/// This is a simplified version - TypeScript uses LLM to create a summary
fn calculate_compaction_messages(
    messages: &[crate::types::Message],
    target_tokens: u32,
) -> Vec<crate::types::Message> {
    if messages.len() <= 4 {
        // Not enough messages to need compaction
        return messages.to_vec();
    }

    // Estimate tokens per message (rough average)
    let avg_tokens_per_msg = 500;
    let target_message_count = (target_tokens / avg_tokens_per_msg).max(10) as usize;

    // Always keep at least first 2 messages (system + initial context)
    // Keep more recent messages to preserve conversation context
    let keep_first = 2;
    let keep_last = target_message_count.saturating_sub(keep_first);

    if messages.len() <= keep_first + keep_last {
        return messages.to_vec();
    }

    let first_part = &messages[..keep_first];
    let last_part = &messages[messages.len() - keep_last..];

    let mut result = Vec::with_capacity(keep_first + keep_last);
    result.extend(first_part.iter().cloned());
    result.extend(last_part.iter().cloned());
    result
}

/// Extract text from API response (supports both Anthropic and OpenAI formats)
fn extract_text_from_response(response: &serde_json::Value) -> String {
    // Try OpenAI format first: response.choices[].message.content
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(content) = first_choice.get("message").and_then(|m| m.get("content")) {
                if let Some(text) = content.as_str() {
                    return text.to_string();
                }
            }
        }
    }

    // Try Anthropic format: response.content[].text
    if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
        for block in content {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                return text.to_string();
            }
        }
    }

    String::new()
}

/// Parse the compact summary to extract <summary> content
/// Strips the <analysis> block as it's just a drafting scratchpad
fn parse_compact_summary(raw_summary: &str) -> String {
    // Extract <summary> content
    if let Some(start) = raw_summary.find("<summary>") {
        if let Some(end) = raw_summary.find("</summary>") {
            let mut summary = raw_summary[start + 9..end].trim().to_string();

            // Also look for any content after </summary> that might be part of summary
            if let Some(after) = raw_summary.find("</summary>") {
                let remaining = raw_summary[after + 11..].trim();
                if !remaining.is_empty() && !remaining.starts_with('<') {
                    summary.push_str("\n\n");
                    summary.push_str(remaining);
                }
            }

            // If no <summary> tag found, use the whole response as summary
            return if summary.is_empty() {
                raw_summary.trim().to_string()
            } else {
                summary
            };
        }
    }

    // If no <summary> tags, try to find and remove <analysis> section
    let mut cleaned = raw_summary.to_string();
    if let Some(analysis_start) = cleaned.find("<analysis>") {
        if let Some(analysis_end) = cleaned.find("</analysis>") {
            cleaned = format!(
                "{}{}",
                &cleaned[..analysis_start],
                cleaned[analysis_end + 11..].trim()
            );
        }
    }

    cleaned.trim().to_string()
}

fn extract_tool_calls(response: &serde_json::Value) -> Vec<serde_json::Value> {
    // First try OpenAI format: response.choices[].message.tool_calls
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
                    if !tool_calls.is_empty() {
                        return tool_calls
                            .iter()
                            .map(|tc| {
                                let func = tc.get("function");
                                let name = func
                                    .and_then(|f| f.get("name"))
                                    .cloned()
                                    .unwrap_or_else(|| empty_json_value());
                                // Handle arguments - could be string or object
                                let args = func.and_then(|f| f.get("arguments"));
                                let arguments = if let Some(args_val) = args {
                                    if let Some(arg_str) = args_val.as_str() {
                                        // Parse JSON string to object
                                        serde_json::from_str(arg_str).unwrap_or(args_val.clone())
                                    } else {
                                        args_val.clone()
                                    }
                                } else {
                                    serde_json::Value::Null
                                };
                                // Get tool_call_id from the tc object directly
                                let id = tc.get("id").cloned();
                                let mut result = serde_json::json!({
                                    "name": name,
                                    "arguments": arguments,
                                });
                                if let Some(id_val) = id {
                                    result["id"] = id_val;
                                }
                                result
                            })
                            .collect();
                    }
                }
            }
        }
    }

    vec![]
}
/// Format: \n<minimax:tool_call>\n<invoke name="tool-name">\n<parameter name="key">value

fn extract_response_text(response: &serde_json::Value) -> String {
    // OpenAI chat completions format
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    return content.to_string();
                }
            }
        }
    }

    // Fallback: check for Anthropic format
    if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
        for block in content {
            if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                match block_type {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            return t.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    String::new()
}

fn extract_usage(response: &serde_json::Value) -> TokenUsage {
    // OpenAI format: response.usage
    if let Some(usage) = response.get("usage") {
        return TokenUsage {
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                + usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            iterations: None,
        };
    }

    // Fallback: Anthropic format
    let usage = response.get("usage");
    TokenUsage {
        input_tokens: usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_creation_input_tokens: usage
            .and_then(|u| u.get("cache_creation_input_tokens"))
            .and_then(|v| v.as_u64()),
        cache_read_input_tokens: usage
            .and_then(|u| u.get("cache_read_input_tokens"))
            .and_then(|v| v.as_u64()),
        iterations: None,
    }
}

/// Maximum number of 429 retry attempts at the query level
const MAX_429_RETRIES: u32 = 5;
/// Base delay between 429 retries in milliseconds
const _429_RETRY_BASE_MS: u64 = 2000;
/// Maximum delay between 429 retries in milliseconds
const _429_RETRY_MAX_MS: u64 = 30_000;
/// Maximum structured output retries
const MAX_STRUCTURED_OUTPUT_RETRIES: u32 = 5;

/// Result type for the retry loop.  Distinguishes between a successful API
/// call, a model fallback that should be handled by the caller, client
/// recreation (rebuild HTTP client and retry), and terminal errors.
///
/// Matches TypeScript's withRetry generator which throws
/// FallbackTriggeredError or returns normally.
enum RetryResult {
    Success(StreamingResult),
    FallbackTriggered(FallbackTriggeredError),
    RecreateClient(AgentError),
    Terminal(AgentError),
}

fn error_to_message_for_retry(error: &AgentError) -> String {
    match error {
        AgentError::Api(msg) => msg.clone(),
        AgentError::Http(e) => format!("{}", e),
        other => other.to_string(),
    }
}

/// Calculate delay with exponential backoff and jitter for retries.
fn calculate_retry_delay(attempt: u32) -> u64 {
    let base = _429_RETRY_BASE_MS * 2u64.saturating_pow(attempt.saturating_sub(1));
    let capped = base.min(_429_RETRY_MAX_MS);
    // Add up to 25% jitter
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let jitter = (capped as f64 * 0.25 * (nanos as f64 / u32::MAX as f64)) as u64;
    capped + jitter
}

/// Attempt streaming then non-streaming request.
/// Wraps the full streaming-to-fallback flow used by submit_message.
async fn async_make_api_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    request_body: serde_json::Value,
    on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
) -> Result<StreamingResult, AgentError> {
    // Try streaming first
    match make_anthropic_streaming_request(
        client,
        url,
        api_key,
        request_body.clone(),
        on_event.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .await
    {
        Ok(result) => return Ok(result),
        Err(_) => {} // Fall through to non-streaming
    }

    // Streaming failed - fall back to non-streaming
    make_nonstreaming_request(client, url, api_key, request_body, on_event).await
}

/// Make an API request with 429/529 retry and exponential backoff.
///
/// Tracks consecutive 529 errors separately.  After MAX_529_RETRIES (3)
/// consecutive 529s with a fallback model available, returns
/// RetryResult::FallbackTriggered so the caller can switch models.
///
/// On stale connection (ECONNRESET/EPIPE) or auth errors (401), returns
/// RetryResult::RecreateClient so the caller rebuilds the HTTP client.
///
/// On max-tokens-context-overflow, adjusts max_output_tokens and retries.
///
/// Returns RetryResult::Terminal for errors that cannot be retried.
///
/// Matches TypeScript's withRetry() generator in withRetry.ts.
async fn make_api_request_with_429_retry(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    request_body: serde_json::Value,
    on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    fallback_model: Option<String>,
    current_model: &str,
    thinking_budget_tokens: Option<u32>,
) -> RetryResult {
    let mut consecutive_529s: u32 = 0;
    let mut last_error_str: Option<String> = None;

    // We clone request_body so we can mutate max_tokens on overflow retries
    let mut mutable_request = request_body.clone();

    for attempt in 0..=MAX_429_RETRIES {
        match async_make_api_request(
            client,
            url,
            api_key,
            mutable_request.clone(),
            on_event.clone(),
        )
        .await
        {
            Ok(result) => return RetryResult::Success(result),
            Err(e) => {
                last_error_str = Some(e.to_string());

                // --- 529 tracking: consecutive 529 errors ---
                if is_529_error(&e) {
                    consecutive_529s += 1;
                    if consecutive_529s >= MAX_529_RETRIES {
                        if let Some(ref fb) = fallback_model {
                            return RetryResult::FallbackTriggered(FallbackTriggeredError {
                                original_model: current_model.to_string(),
                                fallback_model: fb.clone(),
                            });
                        }
                        // No fallback model -- treat as terminal after max 529s
                        if attempt >= MAX_429_RETRIES {
                            return RetryResult::Terminal(e);
                        }
                    }
                } else {
                    // Non-529 error resets the consecutive counter
                    consecutive_529s = 0;
                }

                // --- Stale connection or auth error: recreate client ---
                if is_stale_connection_error(&e) || is_auth_error(&e) {
                    return RetryResult::RecreateClient(e);
                }

                // --- Max tokens context overflow: adjust and retry ---
                if let Some((input_tokens, _max_tokens, context_limit)) =
                    parse_max_tokens_context_overflow(&e)
                {
                    let safety_buffer: u64 = 1000;
                    let available = context_limit.saturating_sub(input_tokens).saturating_sub(safety_buffer);
                    if available < FLOOR_OUTPUT_TOKENS {
                        return RetryResult::Terminal(e);
                    }
                    // Ensure enough tokens for thinking + at least 1 output token (TS: withRetry.ts:418-422)
                    let min_required = (thinking_budget_tokens.unwrap_or(0) as u64).saturating_add(1);
                    let adjusted = std::cmp::max(FLOOR_OUTPUT_TOKENS, std::cmp::max(available, min_required));
                    if let Some(max_t) = mutable_request.get_mut("max_tokens") {
                        *max_t = serde_json::json!(adjusted as u32);
                    }
                    // Retry immediately with adjusted max_tokens
                    continue;
                }

                // --- Pure 429 (not 529): retry with backoff ---
                if is_429_only_error(&e) && attempt < MAX_429_RETRIES {
                    let delay = calculate_retry_delay(attempt + 1);
                    emit_api_retry_event(
                        on_event.as_ref().map(|a| a.as_ref()),
                        attempt + 1,
                        MAX_429_RETRIES,
                        delay,
                        None,
                        &e.to_string(),
                    );
                    sleep_tokio(std::time::Duration::from_millis(delay)).await;
                    continue;
                }

                // --- 529 (below MAX_529_RETRIES): retry with backoff ---
                if is_529_error(&e) && attempt < MAX_429_RETRIES {
                    let delay = calculate_retry_delay(attempt + 1);
                    emit_api_retry_event(
                        on_event.as_ref().map(|a| a.as_ref()),
                        attempt + 1,
                        MAX_429_RETRIES,
                        delay,
                        None,
                        &e.to_string(),
                    );
                    sleep_tokio(std::time::Duration::from_millis(delay)).await;
                    continue;
                }

                // --- Terminal error ---
                return RetryResult::Terminal(e);
            }
        }
    }

    RetryResult::Terminal(AgentError::Api(last_error_str.unwrap_or_else(|| {
        "Retry exhausted".to_string()
    })))
}

/// Make non-streaming API request (fallback when streaming fails)
/// Matches TypeScript's executeNonStreamingRequest behavior
async fn make_nonstreaming_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    mut request_body: serde_json::Value,
    on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
) -> Result<StreamingResult, AgentError> {
    // Disable streaming for non-streaming request
    request_body["stream"] = serde_json::json!(false);

    // Get model name for cost tracking
    let model = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Determine if this is Anthropic API or a third-party API (OpenAI-compatible)
    let is_anthropic = url.contains("anthropic.com");

    // Build the request and execute with retry (wraps .send() with exponential backoff)
    let request_builder = if is_anthropic {
        // Anthropic format
        client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("User-Agent", get_user_agent())
            .json(&request_body)
    } else {
        // OpenAI-compatible format (vLLM, etc.) - use Bearer auth
        client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("User-Agent", get_user_agent())
            .json(&request_body)
    };

    // Send request directly — no retry here since callers handle retry
    let response = request_builder.send().await.map_err(AgentError::from)?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AgentError::Api(format!(
            "Non-streaming API error {}: {}",
            status,
            sanitize_html_error(&error_text)
        )));
    }

    // Emit MessageStart event
    if let Some(ref cb) = on_event {
        cb(AgentEvent::MessageStart {
            message_id: uuid::Uuid::new_v4().to_string(),
        });
    }

    // Get response body
    let response_text = response
        .text()
        .await
        .map_err(|e| AgentError::Api(format!("Failed to read non-streaming response: {}", e)))?;

    // Parse JSON response
    let response_json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        AgentError::Api(format!(
            "Failed to parse non-streaming response: {} - {}",
            e, response_text
        ))
    })?;

    // Check for API error
    if let Some(error) = response_json.get("error") {
        // Check for max_output_tokens error type (matching TypeScript's isWithheldMaxOutputTokens)
        if let Some(error_type) = error.get("type").and_then(|t| t.as_str()) {
            if error_type == "max_tokens" || error_type == "max_output_tokens" {
                // Return result with api_error instead of failing - allows recovery flow
                let mut result = StreamingResult::default();
                result.api_error = Some("max_output_tokens".to_string());
                return Ok(result);
            }
        }
        // Check for prompt-too-long / 413 - trigger reactive compact
        let error_str = error.to_string().to_lowercase();
        if error_str.contains("413")
            || error_str.contains("prompt_too_long")
            || error_str.contains("prompt too long")
        {
            return Err(AgentError::Api("prompt_too_long: context size exceeded. The query engine will attempt reactive compact.".to_string()));
        }
        return Err(AgentError::Api(format!("API error: {}", error)));
    }

    let mut result = StreamingResult::default();

    // Handle Anthropic format: response.content[] with blocks
    if let Some(content) = response_json.get("content").and_then(|c| c.as_array()) {
        for block in content {
            let block_type = block.get("type").and_then(|t| t.as_str());
            match block_type {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        result.content.push_str(text);
                    }
                }
                Some("thinking") | Some("redacted_thinking") => {
                    // Handle thinking blocks - extract thinking content
                    // In TypeScript, thinking is stored as structured block in content
                    // We need to extract it and store properly for display
                    if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                        // Store thinking with markers so TUI can extract it
                        result
                            .content
                            .push_str(&format!("【thinking:{}】", thinking));
                    }
                }
                Some("tool_use") => {
                    let tool_id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let tool_name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let tool_input = block
                        .get("input")
                        .cloned()
                        .unwrap_or_else(|| empty_json_value());

                    result.tool_calls.push(serde_json::json!({
                        "id": tool_id,
                        "name": tool_name,
                        "arguments": tool_input,
                    }));
                }
                _ => {}
            }
        }
        // Extract usage
        if let Some(usage) = response_json.get("usage") {
            result.usage = parse_anthropic_usage(usage);
        }
        // Calculate cost (matching TypeScript cost tracking)
        result.cost = calculate_streaming_cost(&result.usage, &model);
    }
    // Handle OpenAI format: response.choices[].message
    else if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                // Extract content
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    result.content = content.to_string();
                }
                // Extract tool calls
                if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let func = tc.get("function");
                        let name = func
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let args = func.and_then(|f| f.get("arguments"));
                        let args_val = if let Some(args_str) = args.and_then(|a| a.as_str()) {
                            serde_json::from_str(args_str).unwrap_or_else(|_| empty_json_value())
                        } else {
                            args.cloned().unwrap_or_else(|| empty_json_value())
                        };
                        result.tool_calls.push(serde_json::json!({
                            "id": id,
                            "name": name,
                            "arguments": args_val,
                        }));
                    }
                }
            }
        }
        // Extract usage (OpenAI format)
        if let Some(usage) = response_json.get("usage") {
            result.usage = TokenUsage {
                input_tokens: usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                output_tokens: usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                iterations: None,
            };
        }
        // Calculate cost (matching TypeScript cost tracking)
        result.cost = calculate_streaming_cost(&result.usage, &model);
    }

    // Emit completion events
    if let Some(ref cb) = on_event {
        cb(AgentEvent::ContentBlockStart {
            index: 0,
            block_type: "text".to_string(),
        });
        if !result.content.is_empty() {
            cb(AgentEvent::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::Text {
                    text: result.content.clone(),
                },
            });
        }
        cb(AgentEvent::ContentBlockStop { index: 0 });
        cb(AgentEvent::MessageStop);
    }

    Ok(result)
}

/// Make streaming API call and process SSE events
/// Matches TypeScript query.ts behavior for streaming
/// Includes: idle watchdog, stall detection, TTFT, cost tracking, abort handling,
/// stream completion validation, message_delta handling, resource cleanup.
async fn make_anthropic_streaming_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    request_body: serde_json::Value,
    on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    abort_handle: Arc<AtomicBool>,
) -> Result<StreamingResult, AgentError> {
    use futures_util::stream::StreamExt;

    // Determine if this is Anthropic API or a third-party API (OpenAI-compatible)
    let is_anthropic = url.contains("anthropic.com");

    // Get model name from request body for cost tracking
    let model = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // ─── Stream Watchdog Setup (matching TypeScript lines 1743-1793) ───
    let watchdog = StreamWatchdog::from_env();
    let watchdog_aborted = Arc::new(AtomicBool::new(false));
    let watchdog_aborted_clone = watchdog_aborted.clone();

    // ─── Stall Detection Setup (matching TypeScript lines 1801-1821) ───
    let mut stall_stats = StallStats::default();
    let mut last_event_time: Option<std::time::Instant> = None;
    let mut is_first_chunk = true;
    let start_time = std::time::Instant::now();

    // Record when the stream started (for TTFT calculation)
    let mut ttft_recorded = false;

    // Build the request and execute with retry (wraps .send() with exponential backoff)
    // 404 stream creation errors are NOT retryable, so they bypass the retry layer
    let request_builder = if is_anthropic {
        // Anthropic format
        client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", get_user_agent())
            .json(&request_body)
    } else {
        // OpenAI-compatible format (vLLM, etc.) - use Bearer auth
        client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", get_user_agent())
            .json(&request_body)
    };

    // Send request directly — no retry here since callers handle retry
    let response = request_builder.send().await.map_err(AgentError::from)?;

    // Check if user aborted before we even started reading
    if abort_handle.load(Ordering::SeqCst) {
        return Err(AgentError::UserAborted);
    }

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        let sanitized = sanitize_html_error(&error_text);
        // Check for 404 stream creation error (matching TypeScript)
        if status.as_u16() == 404 {
            return Err(AgentError::Stream404CreationError(format!(
                "Streaming endpoint returned 404: {}",
                sanitized
            )));
        }
        return Err(AgentError::Api(format!(
            "Streaming API error {}: {}",
            status, sanitized
        )));
    }

    // Store response for cleanup
    let response_for_cleanup = Arc::new(Mutex::new(Some(response)));
    let response_for_cleanup_clone = response_for_cleanup.clone();

    // ─── Reset stream idle timer (called at start and after each event) ───
    let reset_idle_timer = || {
        if watchdog.enabled {
            let watchdog_aborted_warning = watchdog_aborted_clone.clone();
            let watchdog_aborted_timeout = watchdog_aborted_clone.clone();
            let timeout_ms = watchdog.idle_timeout_ms;
            let warning_ms = watchdog.warning_threshold_ms;
            let response_for_cleanup_inner = response_for_cleanup.clone();

            // Warning timer
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(warning_ms)).await;
                if !watchdog_aborted_warning.load(Ordering::SeqCst) {
                    eprintln!(
                        "Streaming idle warning: no chunks received for {}s",
                        warning_ms / 1000
                    );
                }
            });

            // Abort timer
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
                if !watchdog_aborted_timeout.load(Ordering::SeqCst) {
                    watchdog_aborted_timeout.store(true, Ordering::SeqCst);
                    eprintln!(
                        "Streaming idle timeout: no chunks received for {}s, aborting stream",
                        timeout_ms / 1000
                    );
                    // Cancel the response body to release resources
                    if let Ok(mut guard) = response_for_cleanup_inner.lock() {
                        if let Some(resp) = guard.take() {
                            let _ = resp.error_for_status_ref();
                        }
                    }
                }
            });
        }
    };
    reset_idle_timer();

    // Get the streaming body - take ownership from the Arc
    let response = response_for_cleanup.lock().unwrap().take().unwrap();
    let body = response.bytes_stream();
    let mut stream: futures_util::stream::BoxStream<'_, Result<bytes::Bytes, reqwest::Error>> =
        Box::pin(body);

    let mut result = StreamingResult::default();
    let mut current_tool_use: Option<(String, String, String)> = None; // (id, name, args_str)
    // OpenAI tool_calls accumulator: index -> (id, name, accumulated args)
    let mut openai_tool_calls: HashMap<u32, (String, String, String)> = HashMap::new();
    let mut openai_tool_finalized: HashSet<u32> = HashSet::new();
    let mut content_index: u32 = 0;
    let mut tool_use_index: u32 = 0;
    let mut thinking_index: u32 = 0;
    let mut in_tool_use = false;
    let mut text_block_started = false;
    let mut in_thinking = false;
    let mut thinking_content = String::new();

    // ─── Process stream chunks ───
    'stream_loop: while let Some(chunk_result) = stream.next().await {
        // Check if user aborted
        if abort_handle.load(Ordering::SeqCst) {
            // Release stream resources
            release_stream_resources(&Some(abort_handle.clone()), &None);
            return Err(AgentError::UserAborted);
        }

        // Check if watchdog aborted the stream
        if watchdog_aborted.load(Ordering::SeqCst) {
            release_stream_resources(&Some(abort_handle.clone()), &None);
            return Err(AgentError::Api(format!(
                "Stream idle timeout - no chunks received for {}ms",
                watchdog.idle_timeout_ms
            )));
        }

        let chunk =
            chunk_result.map_err(|e| AgentError::Api(format!("Stream read error: {}", e)))?;

        // Reset idle timer on each chunk
        reset_idle_timer();

        // Stall detection (matching TypeScript: only after first event)
        let now = std::time::Instant::now();
        if let Some(last) = last_event_time {
            let gap = now.duration_since(last).as_millis() as u64;
            if gap > STALL_THRESHOLD_MS {
                stall_stats.stall_count += 1;
                stall_stats.total_stall_time_ms += gap;
                stall_stats.stall_durations.push(gap);
                eprintln!(
                    "Streaming stall detected: {:.1}s gap between events (stall #{})",
                    gap as f64 / 1000.0,
                    stall_stats.stall_count
                );
            }
        }
        last_event_time = Some(now);

        // TTFT recording (matching TypeScript)
        if is_first_chunk {
            let ttft = now.duration_since(start_time).as_millis() as u64;
            result.ttft_ms = Some(ttft);
            ttft_recorded = true;
            is_first_chunk = false;
        }

        // Parse the chunk as text
        if let Ok(text) = String::from_utf8(chunk.to_vec()) {
            // Check if this is SSE format (starts with "data: ")
            if !text.starts_with("data: ") {
                // Not SSE format - might be complete JSON response or vLLM streaming
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Check for vLLM streaming format: has "content" at top level (not in choices)
                    if json.get("content").is_some() && json.get("choices").is_none() {
                        // Check if this is a complete non-streaming response (has content array)
                        if let Some(content_array) = json.get("content").and_then(|c| c.as_array())
                        {
                            for block in content_array {
                                let block_type = block.get("type").and_then(|t| t.as_str());
                                match block_type {
                                    Some("text") => {
                                        if let Some(text) =
                                            block.get("text").and_then(|t| t.as_str())
                                        {
                                            result.content.push_str(text);
                                        }
                                    }
                                    Some("tool_use") => {
                                        let tool_id =
                                            block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                        let tool_name = block
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("");
                                        let tool_input = block
                                            .get("input")
                                            .cloned()
                                            .unwrap_or_else(|| empty_json_value());
                                        result.tool_calls.push(serde_json::json!({
                                            "id": tool_id,
                                            "name": tool_name,
                                            "arguments": tool_input,
                                        }));
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(usage) = json.get("usage") {
                                result.usage = parse_anthropic_usage(usage);
                            }
                            result.message_started = true;
                            result.content_blocks_started += 1;
                            result.content_blocks_completed += 1;
                            // Calculate cost
                            result.cost = calculate_streaming_cost(&result.usage, &model);
                            if let Some(ref cb) = on_event {
                                cb(AgentEvent::MessageStart {
                                    message_id: json
                                        .get("id")
                                        .and_then(|i| i.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                });
                                cb(AgentEvent::ContentBlockStart {
                                    index: 0,
                                    block_type: "text".to_string(),
                                });
                                if !result.content.is_empty() {
                                    cb(AgentEvent::ContentBlockDelta {
                                        index: 0,
                                        delta: ContentDelta::Text {
                                            text: result.content.clone(),
                                        },
                                    });
                                }
                                cb(AgentEvent::ContentBlockStop { index: 0 });
                                cb(AgentEvent::MessageStop);
                            }
                            return Ok(result);
                        }
                        // vLLM streaming chunk - accumulate content
                        if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                            result.content.push_str(content);
                        }
                        // Check for stop reason (not null) to know when to finish
                        if let Some(stop_reason) = json.get("stop_reason") {
                            if !stop_reason.is_null() {
                                result.stop_reason = stop_reason.as_str().map(|s| s.to_string());
                                if let Some(ref cb) = on_event {
                                    cb(AgentEvent::ContentBlockStart {
                                        index: 0,
                                        block_type: "text".to_string(),
                                    });
                                    if !result.content.is_empty() {
                                        cb(AgentEvent::ContentBlockDelta {
                                            index: 0,
                                            delta: ContentDelta::Text {
                                                text: result.content.clone(),
                                            },
                                        });
                                    }
                                    cb(AgentEvent::ContentBlockStop { index: 0 });
                                    cb(AgentEvent::MessageStop);
                                }
                                result.message_started = true;
                                result.content_blocks_started += 1;
                                result.content_blocks_completed += 1;
                                result.cost = calculate_streaming_cost(&result.usage, &model);
                                return Ok(result);
                            }
                        }
                        continue;
                    }

                    // Standard OpenAI streaming format: choices[0].delta.content
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(first) = choices.first() {
                            if let Some(delta) = first.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str())
                                {
                                    result.content.push_str(content);
                                }
                                // Extract tool calls from delta (streaming tool calls)
                                if let Some(tool_calls) =
                                    delta.get("tool_calls").and_then(|t| t.as_array())
                                {
                                    for tc in tool_calls {
                                        let idx =
                                            tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                                as u32;
                                        let id =
                                            tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                        let func = tc.get("function");
                                        let name = func
                                            .and_then(|f| f.get("name"))
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("");
                                        let args_str = func
                                            .and_then(|f| f.get("arguments"))
                                            .and_then(|a| a.as_str())
                                            .unwrap_or("");

                                        // Accumulate args into openai_tool_calls map
                                        if !openai_tool_finalized.contains(&idx) {
                                            let entry =
                                                openai_tool_calls.entry(idx).or_insert_with(|| {
                                                    (
                                                        id.to_string(),
                                                        name.to_string(),
                                                        String::new(),
                                                    )
                                                });
                                            if entry.0.is_empty() && !id.is_empty() {
                                                entry.0 = id.to_string();
                                            }
                                            if entry.1.is_empty() && !name.is_empty() {
                                                entry.1 = name.to_string();
                                            }
                                            entry.2.push_str(args_str);
                                        }
                                    }
                                }
                            }
                            // Check for finish_reason to know when to stop
                            if let Some(finish_reason) =
                                first.get("finish_reason").and_then(|f| f.as_str())
                            {
                                if !finish_reason.is_empty()
                                    && finish_reason != "null"
                                    && (!result.content.is_empty()
                                        || !result.tool_calls.is_empty()
                                        || !openai_tool_calls.is_empty())
                                {
                                    result.stop_reason = Some(finish_reason.to_string());

                                    // Finalize accumulated OpenAI tool calls
                                    for (idx, (id, name, args)) in &openai_tool_calls {
                                        if !openai_tool_finalized.contains(idx) {
                                            let args_val: serde_json::Value =
                                                serde_json::from_str(args)
                                                    .unwrap_or_else(|_| empty_json_value());
                                            result.tool_calls.push(serde_json::json!({
                                                "id": id,
                                                "name": name,
                                                "arguments": args_val,
                                            }));
                                        }
                                    }
                                    openai_tool_finalized.extend(openai_tool_calls.keys().copied());

                                    if let Some(ref cb) = on_event {
                                        cb(AgentEvent::ContentBlockStop { index: 0 });
                                        cb(AgentEvent::MessageStop);
                                    }
                                    result.message_started = true;
                                    result.content_blocks_started += 1;
                                    result.content_blocks_completed += 1;
                                    result.cost = calculate_streaming_cost(&result.usage, &model);
                                    return Ok(result);
                                }
                            }
                        }
                        continue;
                    }

                    // Complete non-streaming response (standard OpenAI format)
                    if json.get("choices").is_some() {
                        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                            if let Some(first) = choices.first() {
                                if let Some(msg) = first.get("message") {
                                    if let Some(content) =
                                        msg.get("content").and_then(|c| c.as_str())
                                    {
                                        result.content = content.to_string();
                                    }
                                    if let Some(tool_calls) =
                                        msg.get("tool_calls").and_then(|t| t.as_array())
                                    {
                                        for tc in tool_calls {
                                            let id =
                                                tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                            let func = tc.get("function");
                                            let name = func
                                                .and_then(|f| f.get("name"))
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("");
                                            let args = func.and_then(|f| f.get("arguments"));
                                            let args_val = if let Some(args_str) =
                                                args.and_then(|a| a.as_str())
                                            {
                                                serde_json::from_str(args_str)
                                                    .unwrap_or_else(|_| empty_json_value())
                                            } else {
                                                args.cloned().unwrap_or_else(|| empty_json_value())
                                            };
                                            result.tool_calls.push(serde_json::json!({
                                                "id": id,
                                                "name": name,
                                                "arguments": args_val,
                                            }));
                                        }
                                    }
                                    // Extract stop_reason from finish_reason
                                    if let Some(finish_reason) =
                                        first.get("finish_reason").and_then(|f| f.as_str())
                                    {
                                        result.stop_reason = Some(finish_reason.to_string());
                                    }
                                }
                            }
                        }
                        if let Some(usage) = json.get("usage") {
                            result.usage = TokenUsage {
                                input_tokens: usage
                                    .get("prompt_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0),
                                output_tokens: usage
                                    .get("completion_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0),
                                cache_creation_input_tokens: None,
                                cache_read_input_tokens: None,
                                iterations: None,
                            };
                        }
                        result.message_started = true;
                        result.content_blocks_started += 1;
                        result.content_blocks_completed += 1;
                        result.cost = calculate_streaming_cost(&result.usage, &model);
                        if let Some(ref cb) = on_event {
                            cb(AgentEvent::ContentBlockStart {
                                index: 0,
                                block_type: "text".to_string(),
                            });
                            if !result.content.is_empty() {
                                cb(AgentEvent::ContentBlockDelta {
                                    index: 0,
                                    delta: ContentDelta::Text {
                                        text: result.content.clone(),
                                    },
                                });
                            }
                            cb(AgentEvent::ContentBlockStop { index: 0 });
                            cb(AgentEvent::MessageStop);
                        }
                        return Ok(result);
                    }
                }
                continue;
            }

            // ─── Parse SSE format: "data: {...}\n\n" ───
            for line in text.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];

                    // Skip [DONE] sentinel
                    if data == "[DONE]" {
                        continue;
                    }

                    // Parse JSON
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        // Handle Anthropic streaming format: check for event type
                        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                            match event_type {
                                "message_start" => {
                                    // Message started - get usage if present
                                    // Matches TypeScript: partialMessage = part.message, usage = updateUsage()
                                    result.message_started = true;
                                    if let Some(usage) = json.get("usage") {
                                        result.usage = parse_anthropic_usage(usage);
                                    }
                                    // Extract research data (internal only, for ant userType)
                                    if json.get("research").is_some() {
                                        result.research = json.get("research").cloned();
                                    }
                                    // Emit MessageStart event (matches TypeScript stream_event flow)
                                    if let Some(ref cb) = on_event {
                                        cb(AgentEvent::MessageStart {
                                            message_id: json
                                                .get("message")
                                                .and_then(|m| m.get("id"))
                                                .and_then(|i| i.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                        });
                                    }
                                }
                                "content_block_start" => {
                                    let index =
                                        json.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as u32;
                                    let block_type = json
                                        .get("content_block")
                                        .and_then(|b| b.get("type"))
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("text")
                                        .to_string();

                                    result.content_blocks_started += 1;

                                    if block_type == "tool_use" {
                                        tool_use_index = index;
                                        in_tool_use = true;
                                        let tool_name = json
                                            .get("content_block")
                                            .and_then(|b| b.get("name"))
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let tool_id = json
                                            .get("content_block")
                                            .and_then(|b| b.get("id"))
                                            .and_then(|i| i.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        current_tool_use =
                                            Some((tool_id, tool_name, String::new()));
                                    } else if block_type == "thinking"
                                        || block_type == "redacted_thinking"
                                    {
                                        in_thinking = true;
                                        thinking_index = index;
                                        thinking_content.clear();
                                    } else {
                                        content_index = index;
                                        text_block_started = true;
                                    }

                                    if let Some(ref cb) = on_event {
                                        cb(AgentEvent::ContentBlockStart { index, block_type });
                                    }
                                }
                                "content_block_delta" => {
                                    let index =
                                        json.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as u32;
                                    if let Some(delta) = json.get("delta") {
                                        let delta_type = delta.get("type").and_then(|t| t.as_str());

                                        match delta_type {
                                            Some("text_delta") => {
                                                if let Some(text) =
                                                    delta.get("text").and_then(|t| t.as_str())
                                                {
                                                    result.content.push_str(text);
                                                    if let Some(ref cb) = on_event {
                                                        cb(AgentEvent::ContentBlockDelta {
                                                            index,
                                                            delta: ContentDelta::Text {
                                                                text: text.to_string(),
                                                            },
                                                        });
                                                    }
                                                }
                                            }
                                            Some("thinking_delta") => {
                                                if let Some(thinking) =
                                                    delta.get("thinking").and_then(|t| t.as_str())
                                                {
                                                    thinking_content.push_str(thinking);
                                                    if let Some(ref cb) = on_event {
                                                        cb(AgentEvent::ContentBlockDelta {
                                                            index,
                                                            delta: ContentDelta::Thinking {
                                                                text: thinking.to_string(),
                                                            },
                                                        });
                                                    }
                                                }
                                            }
                                            Some("input_json_delta") => {
                                                let partial_json = delta
                                                    .get("partial_json")
                                                    .and_then(|p| p.as_str())
                                                    .unwrap_or("");

                                                if let Some(ref mut current) = current_tool_use {
                                                    current.2.push_str(partial_json);
                                                }

                                                if let Some(ref cb) = on_event {
                                                    let tool_name = current_tool_use
                                                        .as_ref()
                                                        .map(|(_, n, _)| n.clone())
                                                        .unwrap_or_default();
                                                    let tool_id = current_tool_use
                                                        .as_ref()
                                                        .map(|(i, _, _)| i.clone())
                                                        .unwrap_or_default();
                                                    cb(AgentEvent::ContentBlockDelta {
                                                        index,
                                                        delta: ContentDelta::ToolUse {
                                                            id: tool_id,
                                                            name: tool_name,
                                                            input: serde_json::json!({ "partial": partial_json }),
                                                            is_complete: false,
                                                        },
                                                    });
                                                }
                                            }
                                            Some("signature_delta") => {
                                                // Signature delta - tracking for thinking block signing
                                                // No content to accumulate, but event is emitted
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                "content_block_stop" => {
                                    let index =
                                        json.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as u32;

                                    result.content_blocks_completed += 1;

                                    // Check if this was a tool_use block
                                    if in_tool_use && index == tool_use_index {
                                        if let Some((id, name, args_str)) = current_tool_use.take()
                                        {
                                            let args: serde_json::Value =
                                                serde_json::from_str(&args_str)
                                                    .unwrap_or_else(|_| empty_json_value());

                                            result.tool_calls.push(serde_json::json!({
                                                "id": id,
                                                "name": name,
                                                "arguments": args,
                                            }));
                                            result.any_tool_use_completed = true;
                                        }
                                        in_tool_use = false;
                                    }

                                    // Check if this was a thinking block
                                    if in_thinking && index == thinking_index {
                                        if !thinking_content.is_empty() {
                                            result.content.push_str(&format!(
                                                "【thinking:{}】",
                                                thinking_content
                                            ));
                                        }
                                        in_thinking = false;
                                        thinking_content.clear();
                                    }

                                    if let Some(ref cb) = on_event {
                                        cb(AgentEvent::ContentBlockStop { index });
                                    }
                                }
                                "message_delta" => {
                                    // Message delta - matches TypeScript's message_delta handling:
                                    // - Updates usage
                                    // - Extracts stop_reason
                                    // - Calculates cost
                                    if let Some(usage) = json.get("usage") {
                                        result.usage = parse_anthropic_usage(usage);
                                    }
                                    // Extract stop_reason from delta
                                    if let Some(delta) = json.get("delta") {
                                        if let Some(stop_reason) =
                                            delta.get("stop_reason").and_then(|s| s.as_str())
                                        {
                                            result.stop_reason = Some(stop_reason.to_string());
                                        }
                                    }
                                    // Calculate cost from current usage
                                    result.cost = calculate_streaming_cost(&result.usage, &model);
                                    if let Some(ref cb) = on_event {
                                        cb(AgentEvent::TokenUsage {
                                            usage: result.usage.clone(),
                                            cost: result.cost,
                                        });
                                    }
                                }
                                "message_stop" => {
                                    // Message complete — break from the stream loop so the
                                    // post-loop code can emit AgentEvent::MessageStop.
                                    // (The server may not close the HTTP connection
                                    // immediately, causing the loop to hang indefinitely.)
                                    break 'stream_loop;
                                }
                                _ => {}
                            }
                        }

                        // Handle OpenAI streaming format in SSE: choices[0].delta.content
                        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                            if let Some(first) = choices.first() {
                                if let Some(delta) = first.get("delta") {
                                    if let Some(content) =
                                        delta.get("content").and_then(|c| c.as_str())
                                    {
                                        if !content.is_empty() {
                                            result.content.push_str(content);
                                            // Emit MessageStart before first content block delta
                                            if !result.message_started {
                                                result.message_started = true;
                                                if let Some(ref cb) = on_event {
                                                    cb(AgentEvent::MessageStart {
                                                        message_id: uuid::Uuid::new_v4()
                                                            .to_string(),
                                                    });
                                                    cb(AgentEvent::ContentBlockStart {
                                                        index: 0,
                                                        block_type: "text".to_string(),
                                                    });
                                                }
                                            }
                                            if let Some(ref cb) = on_event {
                                                cb(AgentEvent::ContentBlockDelta {
                                                    index: 0,
                                                    delta: ContentDelta::Text {
                                                        text: content.to_string(),
                                                    },
                                                });
                                            }
                                        }
                                    }
                                    // Extract tool calls from delta (streaming tool calls)
                                    if let Some(tool_calls) =
                                        delta.get("tool_calls").and_then(|t| t.as_array())
                                    {
                                        // Emit MessageStart before first tool call
                                        if !result.message_started {
                                            result.message_started = true;
                                            if let Some(ref cb) = on_event {
                                                cb(AgentEvent::MessageStart {
                                                    message_id: uuid::Uuid::new_v4().to_string(),
                                                });
                                            }
                                        }
                                        for tc in tool_calls {
                                            let idx = tc
                                                .get("index")
                                                .and_then(|i| i.as_u64())
                                                .unwrap_or(0)
                                                as u32;
                                            let id =
                                                tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                                            let func = tc.get("function");
                                            let name = func
                                                .and_then(|f| f.get("name"))
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("");
                                            let args_str = func
                                                .and_then(|f| f.get("arguments"))
                                                .and_then(|a| a.as_str())
                                                .unwrap_or("");

                                            // Accumulate args into openai_tool_calls map
                                            if !openai_tool_finalized.contains(&idx) {
                                                let entry = openai_tool_calls
                                                    .entry(idx)
                                                    .or_insert_with(|| {
                                                        (
                                                            id.to_string(),
                                                            name.to_string(),
                                                            String::new(),
                                                        )
                                                    });
                                                // Update id/name on first chunk for this index
                                                if entry.0.is_empty() && !id.is_empty() {
                                                    entry.0 = id.to_string();
                                                }
                                                if entry.1.is_empty() && !name.is_empty() {
                                                    entry.1 = name.to_string();
                                                }
                                                entry.2.push_str(args_str);
                                            }
                                        }
                                    }
                                }
                                // Check for finish_reason
                                if let Some(finish_reason) =
                                    first.get("finish_reason").and_then(|f| f.as_str())
                                {
                                    if !finish_reason.is_empty() && finish_reason != "null" {
                                        result.stop_reason = Some(finish_reason.to_string());
                                        if let Some(ref cb) = on_event {
                                            cb(AgentEvent::ContentBlockStop { index: 0 });
                                            cb(AgentEvent::MessageStop);
                                        }
                                        result.message_started = true;
                                        result.content_blocks_started += 1;
                                        result.content_blocks_completed += 1;
                                        result.cost =
                                            calculate_streaming_cost(&result.usage, &model);

                                        // Finalize accumulated OpenAI tool calls
                                        for (idx, (id, name, args)) in &openai_tool_calls {
                                            if !openai_tool_finalized.contains(idx) {
                                                let args_val: serde_json::Value =
                                                    serde_json::from_str(args)
                                                        .unwrap_or_else(|_| empty_json_value());
                                                result.tool_calls.push(serde_json::json!({
                                                    "id": id,
                                                    "name": name,
                                                    "arguments": args_val,
                                                }));
                                            }
                                        }
                                        openai_tool_finalized
                                            .extend(openai_tool_calls.keys().copied());

                                        return Ok(result);
                                    }
                                }
                            }
                            continue;
                        }

                        // Also check for non-streaming response format (Anthropic)
                        if json.get("content").is_some() || json.get("id").is_some() {
                            if let Some(content_array) =
                                json.get("content").and_then(|c| c.as_array())
                            {
                                for block in content_array {
                                    let block_type = block.get("type").and_then(|t| t.as_str());
                                    match block_type {
                                        Some("text") => {
                                            if let Some(text) =
                                                block.get("text").and_then(|t| t.as_str())
                                            {
                                                result.content.push_str(text);
                                            }
                                        }
                                        Some("tool_use") => {
                                            let tool_id = block
                                                .get("id")
                                                .and_then(|i| i.as_str())
                                                .unwrap_or("");
                                            let tool_name = block
                                                .get("name")
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("");
                                            let tool_input = block
                                                .get("input")
                                                .cloned()
                                                .unwrap_or_else(|| empty_json_value());

                                            result.tool_calls.push(serde_json::json!({
                                                "id": tool_id,
                                                "name": tool_name,
                                                "arguments": tool_input,
                                            }));
                                            result.any_tool_use_completed = true;
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            if let Some(usage) = json.get("usage") {
                                result.usage = parse_anthropic_usage(usage);
                            }
                            result.message_started = true;
                            result.content_blocks_started += 1;
                            result.content_blocks_completed += 1;
                            result.cost = calculate_streaming_cost(&result.usage, &model);

                            if let Some(ref cb) = on_event {
                                cb(AgentEvent::ContentBlockStart {
                                    index: 0,
                                    block_type: "text".to_string(),
                                });
                                if !result.content.is_empty() {
                                    cb(AgentEvent::ContentBlockDelta {
                                        index: 0,
                                        delta: ContentDelta::Text {
                                            text: result.content.clone(),
                                        },
                                    });
                                }
                                cb(AgentEvent::ContentBlockStop { index: 0 });
                                cb(AgentEvent::MessageStop);
                            }
                            return Ok(result);
                        }
                    }
                }
            }
        }
    }

    // ─── Stream ended - final processing ───

    // Calculate final cost
    result.cost = calculate_streaming_cost(&result.usage, &model);

    // Mark watchdog as no longer running (prevent timer from firing after stream ends)
    watchdog_aborted.store(true, Ordering::SeqCst);

    // Emit MessageStop event
    if let Some(ref cb) = on_event {
        cb(AgentEvent::MessageStop);
    }

    // Validate stream completion (matching TypeScript: throw if no events received)
    validate_stream_completion(&result)?;

    Ok(result)
}

/// Build memory prefetch context by finding relevant memories for the query.
async fn build_memory_prefetch_context(
    prompt: &str,
    config: &QueryEngineConfig,
    loaded_paths: &std::collections::HashSet<String>,
) -> Option<String> {
    use crate::memdir::{find_relevant_memories, get_memory_base_dir, is_auto_memory_enabled};

    if !is_auto_memory_enabled() {
        return None;
    }

    let memory_dir = get_memory_base_dir();

    let relevant = find_relevant_memories(prompt, &memory_dir).await;

    if relevant.is_empty() {
        return None;
    }

    let new_paths: Vec<String> = relevant
        .into_iter()
        .filter(|p| !loaded_paths.contains(p.as_str()))
        .collect();

    if new_paths.is_empty() {
        return None;
    }

    let paths_display = new_paths.join("\n");
    Some(format!(
        "<relevant-memories>\nThe following memory files may be relevant to your query:\n{}\n</relevant-memories>",
        paths_display
    ))
}
