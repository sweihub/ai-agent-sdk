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
use crate::services::streaming::{
    STALL_THRESHOLD_MS, StallStats, StreamWatchdog, StreamingResult, StreamingToolExecutor,
    calculate_streaming_cost, cleanup_stream, get_nonstreaming_fallback_timeout_ms,
    is_404_stream_creation_error, is_api_timeout_error, is_nonstreaming_fallback_disabled,
    is_user_abort_error, release_stream_resources, validate_stream_completion,
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
    pub token_budget: Option<u64>,
    /// Optional agent ID for subagent identification. Token budget is skipped for subagents.
    pub agent_id: Option<String>,
    /// Memory paths already loaded by parent agents
    pub loaded_nested_memory_paths: std::collections::HashSet<String>,
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
            loaded_nested_memory_paths: std::collections::HashSet::new(),
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
            total_usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
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

            // Execute the tool
            let result = executor(input.clone(), &context).await;

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

    /// Set the event callback for agent events (tool start/complete/error, thinking, done)
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        self.config.on_event = Some(std::sync::Arc::new(callback));
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

    /// Reset conversation state — clear messages, usage, and turn count.
    /// Preserves config, tool executors, and abort controller.
    pub fn reset(&mut self) {
        self.messages.clear();
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
    }

    /// Attempt to auto-compact the conversation when token count exceeds threshold
    /// Translated from: compactConversation in compact.ts
    /// Returns Ok(true) if compaction happened, Ok(false) if not needed, Err on failure
    async fn do_auto_compact(&mut self) -> Result<bool, AgentError> {
        use crate::compact::{
            estimate_token_count, get_auto_compact_threshold, get_compact_prompt,
            strip_images_from_messages, strip_reinjected_attachments,
        };
        use crate::services::compact::{
            PartialCompactDirection, format_compact_summary,
            get_compact_prompt as get_compact_prompt_service, get_compact_user_summary_message,
        };
        use crate::tools::deferred_tools::{
            get_deferred_tool_names, is_tool_search_enabled_optimistic,
        };

        let token_count = estimate_token_count(&self.messages, self.config.max_tokens);
        let threshold = get_auto_compact_threshold(&self.config.model);

        // Check if we need to compact
        if token_count <= threshold {
            return Ok(false);
        }

        log::info!(
            "[compact] Starting auto-compact: {} tokens, threshold: {}",
            token_count,
            threshold
        );

        // Phase 1: Pre-compact hooks
        // Execute pre_compact hooks and merge any custom instructions
        let _hook_results = self.execute_pre_compact_hooks().await;

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
                return Ok(true);
            }
        }

        // Phase 3: Strip images and reinjected attachments before compact API call
        let stripped_messages =
            strip_reinjected_attachments(&strip_images_from_messages(&self.messages));

        // Phase 4: Build compact prompt
        let compact_prompt = get_compact_prompt();

        // Phase 5: Generate summary using LLM with PTL retry logic
        let summary = match self
            .generate_summary_with_ptl_retry(&stripped_messages, &compact_prompt)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[compact] Summary generation failed: {}", e);
                return Err(e);
            }
        };

        // Parse and format the summary
        let formatted_summary = format_compact_summary(&summary);

        // Phase 6: Build post-compact messages
        let messages_to_keep: Vec<Message> = if self.messages.len() > 4 {
            self.messages[self.messages.len() - 4..].to_vec()
        } else {
            self.messages.clone()
        };

        // Create boundary marker with summary
        let discovered_tools = get_deferred_tool_names(&self.config.tools);
        let mut boundary_content = format!(
            "[Previous conversation summarized]\n\n{}",
            get_compact_user_summary_message(&formatted_summary, Some(true), None, None)
        );
        if !discovered_tools.is_empty() && is_tool_search_enabled_optimistic() {
            boundary_content.push_str("\n\n<available-deferred-tools>\n");
            boundary_content.push_str(&discovered_tools.join("\n"));
            boundary_content.push_str("\n</available-deferred-tools>");
        }

        let boundary_msg = Message {
            role: MessageRole::System,
            content: boundary_content,
            is_meta: Some(true),
            ..Default::default()
        };

        // Create new message list: boundary + recent messages
        let mut new_messages = vec![boundary_msg];
        new_messages.extend(messages_to_keep.clone());

        let new_token_count = estimate_token_count(&new_messages, self.config.max_tokens);

        // Phase 7: Post-compact phase
        // Clear file read state and loaded memory paths
        // Re-add plan attachment, plan mode attachment, skill attachment if applicable
        // Execute session_start hooks
        // Execute post_compact hooks
        self.execute_post_compact_hooks(&formatted_summary).await;

        // Phase 8: Post-compaction cleanup
        crate::services::compact::run_post_compact_cleanup(None);

        // Apply the new messages
        self.messages = new_messages;

        log::info!(
            "[compact] Complete: {} tokens -> {} tokens",
            token_count,
            new_token_count
        );

        Ok(true)
    }

    /// Generate summary with PTL (prompt-too-long) retry logic.
    /// If the compact API call fails with prompt-too-long, drops oldest
    /// message groups until the gap is covered.
    async fn generate_summary_with_ptl_retry(
        &self,
        messages: &[Message],
        compact_prompt: &str,
    ) -> Result<String, AgentError> {
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

            // Verify it's safe before proceeding
            if estimated_tokens > 150000 {
                if attempt < MAX_PTL_RETRIES - 1 {
                    // PTL retry: drop oldest message groups
                    log::warn!(
                        "[compact] PTL retry {}/{}: {} tokens, dropping oldest groups",
                        attempt + 1,
                        MAX_PTL_RETRIES,
                        estimated_tokens
                    );
                    summary_messages =
                        self.truncate_head_for_ptl_retry(&summary_messages, estimated_tokens);
                    continue;
                }
                return Err(AgentError::Api(format!(
                    "Cannot generate summary: estimated {} tokens exceeds safe limit after {} retries",
                    estimated_tokens, MAX_PTL_RETRIES
                )));
            }

            // Attempt summary generation
            match self
                .generate_summary_from_messages(&truncated_messages)
                .await
            {
                Ok(summary) => return Ok(summary),
                Err(e) => {
                    if attempt < MAX_PTL_RETRIES - 1 {
                        log::warn!(
                            "[compact] Summary attempt {}/{} failed: {}, retrying",
                            attempt + 1,
                            MAX_PTL_RETRIES,
                            e
                        );
                        summary_messages =
                            self.truncate_head_for_ptl_retry(&summary_messages, estimated_tokens);
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

    /// Truncate the head of messages for PTL retry.
    /// Groups messages by API round and drops oldest groups until gap covered.
    /// If unparseable token gap: drops 20% of groups.
    /// Keeps at least one group to ensure there's something to summarize.
    fn truncate_head_for_ptl_retry(
        &self,
        messages: &[Message],
        estimated_tokens: u32,
    ) -> Vec<Message> {
        use crate::services::compact::grouping::group_messages_by_api_round;

        let groups = group_messages_by_api_round(messages);
        if groups.is_empty() {
            return messages.to_vec();
        }

        // Calculate how many groups to drop (20% fallback)
        let groups_to_drop = (groups.len() as f64 * 0.2).ceil() as usize;
        let groups_to_drop = groups_to_drop.min(groups.len() - 1); // Keep at least one group

        log::debug!(
            "[compact] Dropping {} of {} groups for PTL retry",
            groups_to_drop,
            groups.len()
        );

        // Flatten remaining groups
        groups.into_iter().skip(groups_to_drop).flatten().collect()
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
    ) -> Result<String, AgentError> {
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

        // Build request
        let request_body = serde_json::json!({
            "model": model,
            "max_tokens": 2048,
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
            return Err(AgentError::Api(format!("Summary API error: {}", error)));
        }

        let summary = extract_text_from_response(&response_json);

        if summary.is_empty() {
            return Err(AgentError::Api("Summary response was empty".to_string()));
        }

        // Parse the summary to extract just the <summary> content
        let parsed_summary = parse_compact_summary(&summary);

        Ok(parsed_summary)
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
        };

        let _results = registry.execute("PostCompact", input).await;
    }

    /// Apply compaction result: replace messages with boundary + kept messages
    fn apply_compaction_result(
        &mut self,
        messages_to_keep: Vec<Message>,
        _post_compact_tokens: u32,
    ) {
        let boundary_msg = Message {
            role: MessageRole::System,
            content: "[Previous conversation summarized]".to_string(),
            is_meta: Some(true),
            ..Default::default()
        };

        let mut new_messages = vec![boundary_msg];
        new_messages.extend(messages_to_keep);
        self.messages = new_messages;
    }

    pub async fn submit_message(
        &mut self,
        prompt: &str,
    ) -> Result<(String, crate::types::ExitReason), AgentError> {
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

        // Note: max_turns check is done AFTER turn completes (matching TypeScript)
        // See below after tool execution loop for the check

        // Apply snip compact and microcompact with PreCompact/PostCompact hooks
        let _pre_compact_instructions = self.execute_pre_compact_hooks().await;
        // Apply snip compact before microcompact (establishes compaction order)
        crate::services::compact::snip_compact_if_known(&self.messages);
        // Apply microcompact before auto-compact: evict old tool result content
        crate::services::compact::microcompact::microcompact_messages(&mut self.messages);
        self.execute_post_compact_hooks("Snip + microcompact applied").await;

        // Check auto-compact BEFORE entering tool loop - don't wait until after API call
        // This ensures we compact before hitting the token limit
        let threshold = get_auto_compact_threshold(&self.config.model);
        let token_count = compact::estimate_token_count(&self.messages, self.config.max_tokens);

        if self.auto_compact_tracking.consecutive_failures < 3 && token_count > threshold {
            // Try to compact before making any API call
            match self.do_auto_compact().await {
                Ok(true) => {
                    // Compaction succeeded, reset tracking state (matching TypeScript)
                    self.auto_compact_tracking.compacted = true;
                    self.auto_compact_tracking.turn_id = uuid::Uuid::new_v4().to_string();
                    self.auto_compact_tracking.turn_counter = 0;
                    self.auto_compact_tracking.consecutive_failures = 0;
                }
                Ok(false) => {
                    // No compaction needed or possible
                }
                Err(e) => {
                    // Compaction failed, continue anyway
                    self.auto_compact_tracking.consecutive_failures += 1;
                    eprintln!("Auto-compact failed: {}", e);
                }
            }
        }

        // Emit Thinking event for the first turn before the first API call
        if let Some(ref cb) = self.config.on_event {
            cb(AgentEvent::Thinking { turn: 1 });
        }
        self.turn_count = 1;

        // Tool call loop - continue until no more tool calls
        // Use config.max_turns as the limit (0xffffffff = effectively unlimited)
        let mut max_tool_turns = self.config.max_turns;
        while max_tool_turns > 0 {
            max_tool_turns -= 1;

            // Apply snip compact and microcompact with PreCompact/PostCompact hooks
            let _pre_compact_instructions = self.execute_pre_compact_hooks().await;
            // Apply snip compact before microcompact (establishes compaction order)
            crate::services::compact::snip_compact_if_known(&self.messages);
            // Apply microcompact before auto-compact in the tool loop
            crate::services::compact::microcompact::microcompact_messages(&mut self.messages);
            self.execute_post_compact_hooks("Snip + microcompact applied").await;

            // Check if we should auto-compact based on token count (after tool execution)
            let token_count = compact::estimate_token_count(&self.messages, self.config.max_tokens);
            let threshold = get_auto_compact_threshold(&self.config.model);
            let _effective_window = get_effective_context_window_size(&self.config.model);

            // Only attempt auto-compact if:
            // 1. Not disabled by circuit breaker (max 3 consecutive failures)
            // 2. Token count exceeds auto-compact threshold
            if self.auto_compact_tracking.consecutive_failures < 3 && token_count > threshold {
                // Attempt auto-compact
                match self.do_auto_compact().await {
                    Ok(true) => {
                        // Compaction succeeded, reset tracking state (matching TypeScript)
                        // Reset turnCounter/turnId to reflect the MOST RECENT compact
                        self.auto_compact_tracking.compacted = true;
                        self.auto_compact_tracking.turn_id = uuid::Uuid::new_v4().to_string();
                        self.auto_compact_tracking.turn_counter = 0;
                        self.auto_compact_tracking.consecutive_failures = 0;
                        // Rebuild api_messages after compaction
                        continue;
                    }
                    Ok(false) => {
                        // No compaction needed or possible
                    }
                    Err(e) => {
                        // Compaction failed, increment failure counter
                        self.auto_compact_tracking.consecutive_failures += 1;
                        eprintln!("Auto-compact failed: {}", e);
                    }
                }
            }

            // Reset compacted flag for next iteration
            self.auto_compact_tracking.compacted = false;

            // Build messages for API
            let api_messages = self.build_api_messages()?;

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
            let effective_max_tokens = self.max_output_tokens_override.unwrap_or(self.config.max_tokens);
            let mut request_body = serde_json::json!({
                "model": model,
                "max_tokens": effective_max_tokens,
                "messages": api_messages,
                "stream": true
            });

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
                            request_body["thinking"] = serde_json::json!({
                                "type": "enabled",
                                "budget_tokens": budget_tokens
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
            let mut use_fallback_model = false;
            let mut streaming_result: StreamingResult;

            // Model fallback loop - try primary model first, then fallback if rate limited
            loop {
                // Use fallback model if primary failed with rate limit
                let current_model = if use_fallback_model {
                    self.config
                        .fallback_model
                        .as_ref()
                        .unwrap_or(&self.config.model)
                        .clone()
                } else {
                    self.config.model.clone()
                };

                // Update request body with current model
                request_body["model"] = serde_json::json!(current_model);

                // Check if non-streaming fallback is disabled (matching TypeScript)
                if is_nonstreaming_fallback_disabled() {
                    return Err(AgentError::Api(
                        "Non-streaming fallback disabled".to_string(),
                    ));
                }

                // Make API request with 429 retry and exponential backoff.
                // Wraps the full streaming→non-streaming fallback flow.
                streaming_result = match make_api_request_with_429_retry(
                    &self.http_client,
                    &url,
                    api_key,
                    request_body.clone(),
                    self.config.on_event.clone(),
                )
                .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        // Check if streaming failed with 429 (all retries exhausted)
                        if is_429_error(&e) {
                            eprintln!("Rate limit hit (429), exhausted 429 retries");
                            if let Some(ref cb) = self.config.on_event {
                                cb(AgentEvent::RateLimitStatus {
                                    is_rate_limited: true,
                                    retry_after_secs: None,
                                });
                            }
                        }

                        // Handle user abort (matching TypeScript APIUserAbortError handling)
                        if is_user_abort_error(&e) {
                            return Err(AgentError::UserAborted);
                        }

                        // Check for 404 stream creation error
                        if is_404_stream_creation_error(&e) {
                            eprintln!(
                                "Streaming endpoint returned 404, falling back to non-streaming mode"
                            );
                        }

                        // Check if this is a prompt-too-long error that should trigger reactive compact
                        let error_str = e.to_string().to_lowercase();
                        let is_prompt_too_long = error_str.contains("413")
                            || error_str.contains("prompt_too_long")
                            || error_str.contains("prompt too long")
                            || error_str.contains("media too large");

                        if is_prompt_too_long {
                            eprintln!("Prompt too large (413), attempting reactive compact...");
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
                                    self.messages = reactive_result.messages;
                                    self.execute_post_compact_hooks("Reactive compact applied after 413 error").await;
                                    use_fallback_model = true;
                                    continue; // Retry with compacted context
                                }
                                _ => {
                                    log::warn!(
                                        "[reactive-compact] no improvement possible, falling through to non-streaming"
                                    );
                                }
                            }
                        }

                        // For all other errors (including exhausted 429 retries without fallback), return
                        // Fire StopFailure hooks (fire-and-forget, matches TypeScript)
                        {
                            let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                            if let Some(registry) = registry_clone {
                                let _ = crate::hooks::run_stop_failure_hooks(&registry, &e.to_string(), &self.config.cwd).await;
                            }
                        }
                        return Err(e);
                    }
                };

                // Successfully got result, break out of loop
                break;
            }

            // Emit StreamRequestEnd — TUI can use this to hide spinner after API response
            if let Some(ref cb) = self.config.on_event {
                cb(AgentEvent::StreamRequestEnd);
            }

            // Check for tool calls in the streaming result
            if streaming_result.tool_calls.is_empty() {
                // Check for max_output_tokens error and handle recovery
                // Matching TypeScript's isWithheldMaxOutputTokens recovery logic
                if streaming_result.api_error.as_deref() == Some("max_output_tokens") {
                    // Escalating retry: if we hit the limit, try with higher max_tokens
                    // This fires once per turn, then falls through to multi-turn recovery if 64k also hits the cap
                    const MAX_OUTPUT_TOKENS_RECOVERY_LIMIT: u32 = 3;
                    const ESCALATED_MAX_TOKENS: u32 = 64_000;

                    if self.max_output_tokens_recovery_count < MAX_OUTPUT_TOKENS_RECOVERY_LIMIT {
                        // Escalate max_tokens for the retry (matches TypeScript escalation)
                        self.max_output_tokens_override = Some(ESCALATED_MAX_TOKENS);

                        // Inject recovery message to resume generation
                        let recovery_message = crate::types::Message {
                            role: crate::types::MessageRole::User,
                            content: "Output token limit hit. Resume directly — no apology, no recap of what you were doing. Pick up mid-thought if that is where the cut happened. Break remaining work into smaller pieces.".to_string(),
                            ..Default::default()
                        };

                        // Add messages for recovery attempt
                        let all_messages = std::mem::take(&mut self.messages);
                        self.messages = all_messages;
                        self.messages.push(recovery_message);

                        // Increment recovery count
                        self.max_output_tokens_recovery_count += 1;

                        // Emit Thinking event for recovery attempt
                        if let Some(ref cb) = self.config.on_event {
                            cb(AgentEvent::Thinking {
                                turn: self.turn_count + 1,
                            });
                        }

                        // Continue to next iteration (retry the request)
                        continue;
                    }

                    // Recovery exhausted - return the error as final response
                    // The content will be empty but we signal completion
                    if let Some(ref cb) = self.config.on_event {
                        cb(AgentEvent::Done {
                            result: crate::types::QueryResult {
                                text: "Output token limit reached and recovery exhausted"
                                    .to_string(),
                                usage: self.total_usage.clone(),
                                num_turns: self.turn_count,
                                duration_ms: 0,
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
                self.turn_tokens += streaming_result.usage.output_tokens;

                // Update total cost (matching TypeScript cost tracking)
                self.total_cost += streaming_result.cost;

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
                                duration_ms: 0,
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
                if !self.stop_hook_active {
                    self.stop_hook_active = true;
                    let stop_result = {
                        let registry_clone = self.hook_registry.lock().unwrap().as_ref().cloned();
                        if let Some(registry) = registry_clone {
                            crate::hooks::run_stop_hooks(&registry, &self.config.cwd, &final_text).await
                        } else {
                            crate::hooks::StopHookResult::default()
                        }
                    };
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
                                    duration_ms: 0,
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

                // Emit Done event (matches TypeScript yielding { type: 'result' })
                if let Some(ref cb) = self.config.on_event {
                    cb(AgentEvent::Done {
                        result: crate::types::QueryResult {
                            text: final_text.clone(),
                            usage: self.total_usage.clone(),
                            num_turns: self.turn_count,
                            duration_ms: 0,
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
                            duration_ms: 0,
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
                    duration_ms: 0, // Could track start time for accurate duration
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
    }
}

/// Maximum number of 429 retry attempts at the query level
const MAX_429_RETRIES: u32 = 5;
/// Base delay between 429 retries in milliseconds
const _429_RETRY_BASE_MS: u64 = 2000;
/// Maximum delay between 429 retries in milliseconds
const _429_RETRY_MAX_MS: u64 = 30_000;

/// Check if an error is a rate limit (429) error
fn is_429_error(error: &AgentError) -> bool {
    let msg = error_to_message_for_retry(error);
    let lower = msg.to_lowercase();
    lower.contains("429")
        || lower.contains("rate_limit")
        || lower.contains("rate limit")
        || lower.contains("overloaded")
        || lower.contains("529")
}

fn error_to_message_for_retry(error: &AgentError) -> String {
    match error {
        AgentError::Api(msg) => msg.clone(),
        AgentError::Http(e) => format!("{}", e),
        other => other.to_string(),
    }
}

/// Calculate delay with exponential backoff and jitter for 429 retries
fn _calculate_429_delay(attempt: u32) -> u64 {
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

/// Make an API request with 429 retry and exponential backoff.
/// Retries the full streaming→non-streaming flow when rate-limited.
async fn make_api_request_with_429_retry(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    request_body: serde_json::Value,
    on_event: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
) -> Result<StreamingResult, AgentError> {
    let mut last_error: Option<AgentError> = None;

    for attempt in 0..=MAX_429_RETRIES {
        match async_make_api_request(client, url, api_key, request_body.clone(), on_event.clone())
            .await
        {
            Ok(result) => return Ok(result),
            Err(e) if is_429_error(&e) && attempt < MAX_429_RETRIES => {
                let delay = _calculate_429_delay(attempt + 1);
                eprintln!(
                    "Rate limited (429), retrying in {}ms (attempt {}/{})",
                    delay,
                    attempt + 1,
                    MAX_429_RETRIES
                );
                sleep_tokio(std::time::Duration::from_millis(delay)).await;
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_error.unwrap_or_else(|| AgentError::Api("429 retry exhausted".to_string())))
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
            status, error_text
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
        // Check for 404 stream creation error (matching TypeScript)
        if status.as_u16() == 404 {
            return Err(AgentError::Stream404CreationError(format!(
                "Streaming endpoint returned 404: {}",
                error_text
            )));
        }
        return Err(AgentError::Api(format!(
            "Streaming API error {}: {}",
            status, error_text
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
    while let Some(chunk_result) = stream.next().await {
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
                                }
                                "message_stop" => {
                                    // Message complete - no-op marker (matching TypeScript)
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
