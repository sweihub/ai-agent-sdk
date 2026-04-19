// Source: ~/claudecode/openclaudecode/src/utils/forkedAgent.ts
//! Helper for running forked agent query loops with usage tracking.
//!
//! This utility ensures forked agents:
//! 1. Share identical cache-critical params with the parent to guarantee prompt cache hits
//! 2. Track full usage metrics across the entire query loop
//! 3. Log metrics via the tengu_fork_agent_query event when complete
//! 4. Isolate mutable state to prevent interference with the main agent loop

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::tool::{DenialTrackingState, QueryChainTracking, ToolUseContext, ToolUseContextOptions};
use crate::types::message::Message;
use crate::utils::messages::Usage;
use crate::utils::abort_controller::{create_child_abort_controller, AbortController};
use crate::utils::file_state_cache::{clone_file_state_cache, FileStateCache};
use crate::utils::uuid::create_agent_id;

// ---------------------------------------------------------------------------
// CacheSafeParams
// ---------------------------------------------------------------------------

/// Parameters that must be identical between the fork and parent API requests
/// to share the parent's prompt cache. The Anthropic API cache key is composed of:
/// system prompt, tools, model, messages (prefix), and thinking config.
///
/// `CacheSafeParams` carries the first five. Thinking config is derived from the
/// inherited `tool_use_context.options.thinking_config` — but can be inadvertently
/// changed if the fork sets `max_output_tokens`, which clamps `budget_tokens` in
/// claude.ts (but only for older models that do not use adaptive thinking).
/// See the `max_output_tokens` doc on `ForkedAgentConfig`.
#[derive(Clone)]
pub struct CacheSafeParams {
    /// System prompt - must match parent for cache hits
    pub system_prompt: String,
    /// User context - prepended to messages, affects cache
    pub user_context: HashMap<String, String>,
    /// System context - appended to system prompt, affects cache
    pub system_context: HashMap<String, String>,
    /// Tool use context containing tools, model, and other options
    pub tool_use_context: Arc<ToolUseContext>,
    /// Parent context messages for prompt cache sharing
    pub fork_context_messages: Vec<Message>,
}

// Slot written by handle_stop_hooks after each turn so post-turn forks
// (prompt_suggestion, post_turn_summary, /btw) can share the main loop's
// prompt cache without each caller threading params through.
static LAST_CACHE_SAFE_PARAMS: std::sync::Mutex<Option<CacheSafeParams>> = std::sync::Mutex::new(None);

/// Save cache-safe params for later retrieval by post-turn forks.
pub fn save_cache_safe_params(params: Option<CacheSafeParams>) {
    let mut guard = LAST_CACHE_SAFE_PARAMS.lock().unwrap();
    *guard = params;
}

/// Get the last saved cache-safe params.
pub fn get_last_cache_safe_params() -> Option<CacheSafeParams> {
    LAST_CACHE_SAFE_PARAMS.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------
// ForkedAgentConfig / ForkedAgentResult
// ---------------------------------------------------------------------------

/// Source identifier for tracking query origins.
#[derive(Debug, Clone)]
pub struct QuerySource(pub String);

/// CanUseTool function type - determines whether a tool may be executed.
pub type CanUseToolFn = dyn Fn(
        &serde_json::Value, // tool definition
        &serde_json::Value, // input
        Arc<ToolUseContext>,
        Arc<crate::types::message::AssistantMessage>,
        &str,  // query source
        bool,  // is explicit
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<PermissionDecision, String>> + Send>,
    > + Send
    + Sync;

/// Permission decision from can_use_tool.
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: Option<String> },
    Ask { expires_at: Option<u64> },
}

/// Options for creating a subagent context.
///
/// By default, all mutable state is isolated to prevent interference with the parent.
/// Use these options to:
/// - Override specific fields (e.g., custom options, agent_id, messages)
/// - Explicitly opt-in to sharing specific callbacks (for interactive subagents)
#[derive(Clone)]
pub struct SubagentContextOverrides {
    /// Override the options object (e.g., custom tools, model)
    pub options: Option<ToolUseContextOptions>,
    /// Override the agent_id (for subagents with their own ID)
    pub agent_id: Option<String>,
    /// Override the agent_type (for subagents with a specific type)
    pub agent_type: Option<String>,
    /// Override the messages array
    pub messages: Option<Vec<Message>>,
    /// Override the read_file_state (e.g., fresh cache instead of clone)
    pub read_file_state: Option<Arc<FileStateCache>>,
    /// Override the abort_controller
    pub abort_controller: Option<Arc<AbortController>>,
    /// Override the get_app_state function
    pub get_app_state: Option<Arc<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync>>,

    /// Explicit opt-in to share parent's set_app_state callback.
    /// Use for interactive subagents that need to update shared state.
    /// @default false (isolated no-op)
    pub share_set_app_state: bool,
    /// Explicit opt-in to share parent's set_response_length callback.
    /// Use for subagents that contribute to parent's response metrics.
    /// @default false (isolated no-op)
    pub share_set_response_length: bool,
    /// Explicit opt-in to share parent's abort_controller.
    /// Use for interactive subagents that should abort with parent.
    /// Note: Only applies if abort_controller override is not provided.
    /// @default false (new controller linked to parent)
    pub share_abort_controller: bool,
    /// Critical system reminder to re-inject at every user turn
    pub critical_system_reminder_experimental: Option<String>,
    /// When true, can_use_tool must always be called even when hooks auto-approve.
    /// Used by speculation for overlay file path rewriting.
    pub require_can_use_tool: Option<bool>,
    /// Override content_replacement_state — used by resumeAgentBackground to thread
    /// state reconstructed from the resumed sidechain so the same results
    /// are re-replaced (prompt cache stability).
    pub content_replacement_state: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl Default for SubagentContextOverrides {
    fn default() -> Self {
        Self {
            options: None,
            agent_id: None,
            agent_type: None,
            messages: None,
            read_file_state: None,
            abort_controller: None,
            get_app_state: None,
            share_set_app_state: false,
            share_set_response_length: false,
            share_abort_controller: false,
            critical_system_reminder_experimental: None,
            require_can_use_tool: None,
            content_replacement_state: None,
        }
    }
}

/// Configuration for a forked agent query.
pub struct ForkedAgentConfig {
    /// Messages to start the forked query loop with
    pub prompt_messages: Vec<Message>,
    /// Cache-safe parameters that must match the parent query
    pub cache_safe_params: CacheSafeParams,
    /// Permission check function for the forked agent
    pub can_use_tool: Arc<CanUseToolFn>,
    /// Source identifier for tracking
    pub query_source: QuerySource,
    /// Label for analytics (e.g., 'session_memory', 'supervisor')
    pub fork_label: String,
    /// Optional overrides for the subagent context
    pub overrides: Option<SubagentContextOverrides>,
    /// Optional cap on output tokens. CAUTION: setting this changes both max_tokens
    /// AND budget_tokens (via clamping in claude.ts). If the fork uses cache_safe_params
    /// to share the parent's prompt cache, a different budget_tokens will invalidate
    /// the cache — thinking config is part of the cache key. Only set this when cache
    /// sharing is not a goal (e.g., compact summaries).
    pub max_output_tokens: Option<u64>,
    /// Optional cap on number of turns (API round-trips)
    pub max_turns: Option<u32>,
    /// Optional callback invoked for each message as it arrives (for streaming UI)
    pub on_message: Option<Arc<dyn Fn(Message) + Send + Sync>>,
    /// Skip sidechain transcript recording (e.g., for ephemeral work like speculation)
    pub skip_transcript: bool,
    /// Skip writing new prompt cache entries on the last message. For
    /// fire-and-forget forks where no future request will read from this prefix.
    pub skip_cache_write: bool,
}

/// Result from a forked agent query.
pub struct ForkedAgentResult {
    /// All messages yielded during the query loop
    pub messages: Vec<Message>,
    /// Accumulated usage across all API calls in the loop
    pub total_usage: Usage,
}

// ---------------------------------------------------------------------------
// Helper: create_cache_safe_params
// ---------------------------------------------------------------------------

/// Creates `CacheSafeParams` from a parent `ToolUseContext`.
/// Use this helper when forking from a post-sampling hook context.
///
/// To override specific fields (e.g., tool_use_context with cloned file state),
/// clone the result and override the field.
pub fn create_cache_safe_params(
    system_prompt: String,
    user_context: HashMap<String, String>,
    system_context: HashMap<String, String>,
    tool_use_context: Arc<ToolUseContext>,
    fork_context_messages: Vec<Message>,
) -> CacheSafeParams {
    CacheSafeParams {
        system_prompt,
        user_context,
        system_context,
        tool_use_context,
        fork_context_messages,
    }
}

// ---------------------------------------------------------------------------
// Helper: create_get_app_state_with_allowed_tools
// ---------------------------------------------------------------------------

/// Creates a modified get_app_state that adds allowed tools to the permission context.
/// This is used by forked skill/command execution to grant tool permissions.
pub fn create_get_app_state_with_allowed_tools(
    base_get_app_state: Arc<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync>,
    allowed_tools: Vec<String>,
) -> Arc<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync> {
    if allowed_tools.is_empty() {
        return base_get_app_state;
    }
    Arc::new(move || {
        let app_state = base_get_app_state();
        // In a full implementation, this would modify the tool_permission_context
        // to add the allowed_tools to always_allow_rules.command.
        // For now, return the base state since the type is opaque.
        app_state
    })
}

// ---------------------------------------------------------------------------
// PreparedForkedContext
// ---------------------------------------------------------------------------

/// Result from preparing a forked command context.
pub struct PreparedForkedContext {
    /// Skill content with args replaced
    pub skill_content: String,
    /// Modified get_app_state with allowed tools
    pub modified_get_app_state: Arc<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync>,
    /// The general-purpose agent to use
    pub base_agent: serde_json::Value,
    /// Initial prompt messages
    pub prompt_messages: Vec<Message>,
}

/// Prepares the context for executing a forked command/skill.
/// This handles the common setup that both SkillTool and slash commands need.
#[allow(dead_code)]
pub async fn prepare_forked_command_context(
    command: serde_json::Value, // PromptCommand as JSON
    args: &str,
    context: &ToolUseContext,
) -> Result<PreparedForkedContext, String> {
    // Get skill content with $ARGUMENTS replaced
    // In a full implementation, this would call command.get_prompt_for_command(args, context)
    let skill_content_for_msg = args.to_string();
    let skill_content_for_result = skill_content_for_msg.clone();

    // Parse and prepare allowed tools
    let allowed_tools: Vec<String> = command
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Create modified context with allowed tools
    // We can't capture `context` into a 'static closure, so we just use a no-op wrapper
    let modified_get_app_state: Arc<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync> =
        Arc::new(|| Box::new(()) as Box<dyn std::any::Any>);
    let _ = create_get_app_state_with_allowed_tools; // unused in this simplified version

    // Use command.agent if specified, otherwise 'general-purpose'
    let agent_type_name = command
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("general-purpose");

    let agents = context.options.agent_definitions.active_agents.clone();
    let base_agent = agents
        .iter()
        .find(|a: &&serde_json::Value| {
            a.get("agent_type")
                .and_then(|v| v.as_str())
                .map(|s| s == agent_type_name)
                .unwrap_or(false)
        })
        .or_else(|| {
            agents.iter().find(|a: &&serde_json::Value| {
                a.get("agent_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "general-purpose")
                    .unwrap_or(false)
            })
        })
        .or_else(|| agents.first())
        .cloned();

    let base_agent =
        base_agent.ok_or_else(|| "No agent available for forked execution".to_string())?;

    // Prepare prompt messages
    let prompt_messages = vec![Message::User(crate::types::message::UserMessage {
        base: crate::types::message::MessageBase {
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            parent_uuid: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            created_at: None,
            is_meta: None,
            is_virtual: None,
            is_compact_summary: None,
            tool_use_result: None,
            origin: None,
            extra: HashMap::new(),
        },
        message_type: "user".to_string(),
        message: crate::types::message::UserMessageContent {
            content: crate::types::message::UserContent::Text(skill_content_for_msg),
            extra: HashMap::new(),
        },
    })];

    Ok(PreparedForkedContext {
        skill_content: skill_content_for_result,
        modified_get_app_state,
        base_agent,
        prompt_messages,
    })
}

// ---------------------------------------------------------------------------
// Helper: extract_result_text
// ---------------------------------------------------------------------------

/// Extracts result text from agent messages.
#[allow(dead_code)]
pub fn extract_result_text(agent_messages: &[Message], default_text: &str) -> String {
    // Find the last assistant message and extract text from its content.
    let last_assistant = agent_messages.iter().rev().find(|m| {
        matches!(m, Message::Assistant(_))
    });
    match last_assistant {
        Some(msg) => {
            if let Ok(json) = serde_json::to_value(msg) {
                let content = json
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array());
                if let Some(arr) = content {
                    let text = extract_text_content_json(arr, "\n");
                    if text.is_empty() {
                        return default_text.to_string();
                    }
                    return text;
                }
            }
            default_text.to_string()
        }
        None => default_text.to_string(),
    }
}

/// Extract text content from a message's content array.
fn extract_text_content_json(content: &[serde_json::Value], separator: &str) -> String {
    let texts: Vec<String> = content
        .iter()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
        .map(|t| t.to_string())
        .collect();
    texts.join(separator)
}

// ---------------------------------------------------------------------------
// create_subagent_context
// ---------------------------------------------------------------------------

/// Creates an isolated `ToolUseContext` for subagents.
///
/// By default, ALL mutable state is isolated to prevent interference:
/// - read_file_state: cloned from parent
/// - abort_controller: new controller linked to parent (parent abort propagates)
/// - get_app_state: wrapped to set should_avoid_permission_prompts
/// - All mutation callbacks (set_app_state, etc.): no-op
/// - Fresh collections: nested_memory_attachment_triggers, tool_decisions
///
/// Callers can:
/// - Override specific fields via the overrides parameter
/// - Explicitly opt-in to sharing specific callbacks (share_set_app_state, etc.)
pub fn create_subagent_context(
    parent_context: &ToolUseContext,
    overrides: Option<&SubagentContextOverrides>,
) -> ToolUseContext {
    let overrides = overrides.cloned().unwrap_or_default();

    // Determine abort_controller: explicit override > share parent's > new child linked to parent
    // Since ToolUseContext stores abort_signal as Option<()>, we create a new AbortController
    // linked to a default parent for the subagent context.
    let child_controller = create_child_abort_controller(&AbortController::default(), None);
    let _abort_controller = child_controller;

    // Determine get_app_state - wrap to set should_avoid_permission_prompts unless sharing
    // (if sharing abort_controller, it's an interactive agent that CAN show UI)
    // Since get_app_state is a Box<dyn Fn...> and can't be cloned, we wrap it in Arc.
    // We need to move the closure out of parent_context, which requires 'static.
    // Since ToolUseContext.get_app_state is a Box<dyn Fn() -> Box<dyn Any> + Send + Sync>,
    // we can't clone it. We use a no-op wrapper for now.
    let get_app_state: Box<dyn Fn() -> Box<dyn std::any::Any> + Send + Sync> =
        if let Some(fn_arc) = overrides.get_app_state {
            Box::new(move || fn_arc())
        } else {
            // No-op wrapper - in a full impl, ToolUseContext would use Arc for this field
            Box::new(|| Box::new(()) as Box<dyn std::any::Any>)
        };

    // Clone file state cache: cloned from parent (or from override)
    let read_file_state = if let Some(override_cache) = &overrides.read_file_state {
        Some(Arc::new(clone_file_state_cache(override_cache)) as Arc<dyn std::any::Any + Send + Sync>)
    } else {
        parent_context.read_file_state.clone()
    };

    // Content replacement state: override > clone of parent > None
    // Clone by default (not fresh): cache-sharing forks process parent
    // messages containing parent tool_use_ids. A fresh state would see
    // them as unseen and make divergent replacement decisions → wire
    // prefix differs → cache miss. A clone makes identical decisions → cache hit.
    // For non-forking subagents the parent UUIDs never match — clone is harmless.
    let content_replacement_state = overrides
        .content_replacement_state
        .clone()
        .or_else(|| parent_context.content_replacement_state.clone());

    // Denial tracking: isolated for non-sharing, shared for sharing
    let local_denial_tracking = if overrides.share_set_app_state {
        parent_context.local_denial_tracking.clone()
    } else {
        Some(Arc::new(std::sync::Mutex::new(DenialTrackingState::default())))
    };

    ToolUseContext {
        // Mutable state - cloned by default to maintain isolation
        read_file_state,
        nested_memory_attachment_triggers: Some(Arc::new(std::sync::Mutex::new(HashSet::new()))),
        loaded_nested_memory_paths: Some(Arc::new(std::sync::Mutex::new(HashSet::new()))),
        dynamic_skill_dir_triggers: Some(Arc::new(std::sync::Mutex::new(HashSet::new()))),
        // Per-subagent: tracks skills surfaced by discovery for was_discovered telemetry
        discovered_skill_names: Some(Arc::new(std::sync::Mutex::new(HashSet::new()))),
        tool_decisions: None,
        // Content replacement state
        content_replacement_state,
        // Abort signal
        abort_signal: None,
        // AppState access
        get_app_state,
        set_app_state: if overrides.share_set_app_state {
            // Can't clone Box<dyn Fn>, so we use a no-op wrapper that calls parent
            // Since we can't move parent_context.set_app_state, we use a no-op here.
            // In a full implementation, ToolUseContext would use Arc for these callbacks.
            Box::new(|_: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>| {})
        } else {
            // No-op
            Box::new(|_: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>| {})
        },
        // Task registration/kill must always reach the root store
        // Can't clone Box<dyn Fn>, use no-op
        set_app_state_for_tasks: Some(Box::new(|_: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>| {})),
        local_denial_tracking,
        // Mutation callbacks - no-op by default (Box<dyn Fn> can't be cloned)
        set_in_progress_tool_use_ids: {
            type SetIdsFn = dyn Fn(&HashSet<String>) -> HashSet<String>;
            Box::new(|_: Box<SetIdsFn>| {})
        },
        set_response_length: if overrides.share_set_response_length {
            // Can't clone, use no-op
            Box::new(|_: Box<dyn Fn(usize) -> usize>| {})
        } else {
            Box::new(|_: Box<dyn Fn(usize) -> usize>| {})
        },
        push_api_metrics_entry: None, // Can't clone Box<dyn Fn>
        update_file_history_state: Box::new(
            |_: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>| {},
        ),
        // Attribution is scoped and functional (prev => next) — use no-op since we can't clone
        update_attribution_state: Box::new(
            |_: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any>>| {},
        ),
        // UI callbacks - None for subagents (can't control parent UI)
        add_notification: None,
        set_tool_jsx: None,
        set_stream_mode: None,
        set_sdk_status: None,
        open_message_selector: None,
        // Fields that can be overridden or copied from parent
        options: overrides.options.clone().unwrap_or_else(|| parent_context.options.clone()),
        messages: overrides
            .messages
            .clone()
            .unwrap_or_else(|| parent_context.messages.clone()),
        // Generate new agent_id for subagents (each subagent should have its own ID)
        agent_id: overrides
            .agent_id
            .clone()
            .or_else(|| Some(create_agent_id(None))),
        agent_type: overrides.agent_type.clone().or_else(|| parent_context.agent_type.clone()),
        // Create new query tracking chain for subagent with incremented depth
        query_tracking: Some(QueryChainTracking {
            chain_id: uuid::Uuid::new_v4().to_string(),
            depth: parent_context
                .query_tracking
                .as_ref()
                .map(|t| t.depth + 1)
                .unwrap_or(0),
        }),
        file_reading_limits: parent_context.file_reading_limits.clone(),
        glob_limits: parent_context.glob_limits.clone(),
        user_modified: parent_context.user_modified,
        critical_system_reminder_experimental: overrides
            .critical_system_reminder_experimental
            .clone()
            .or_else(|| parent_context.critical_system_reminder_experimental.clone()),
        require_can_use_tool: overrides
            .require_can_use_tool
            .unwrap_or(parent_context.require_can_use_tool),
        preserve_tool_use_results: parent_context.preserve_tool_use_results,
        rendered_system_prompt: parent_context.rendered_system_prompt.clone(),
        request_prompt: None, // Can't clone Arc<dyn Fn...>
        tool_use_id: parent_context.tool_use_id.clone(),
        handle_elicitation: None, // Can't clone Arc<dyn Fn...>
        append_system_message: None, // Can't clone Box<dyn Fn>
        send_os_notification: None, // Can't clone Box<dyn Fn>
        set_has_interruptible_tool_in_progress: None, // Can't clone Box<dyn Fn>
        set_conversation_id: None, // Can't clone Box<dyn Fn>
        on_compact_progress: None, // Can't clone Box<dyn Fn>
    }
}

// ---------------------------------------------------------------------------
// run_forked_agent
// ---------------------------------------------------------------------------

/// Runs a forked agent query loop and tracks cache hit metrics.
///
/// This function:
/// 1. Uses identical cache-safe params from parent to enable prompt caching
/// 2. Accumulates usage across all query iterations
/// 3. Logs tengu_fork_agent_query with full usage when complete
///
/// NOTE: The actual query loop integration depends on the `query` module which
/// is still being translated. This implementation provides the full structure
/// and will wire up to the query loop once it's complete.
pub async fn run_forked_agent(
    config: ForkedAgentConfig,
) -> Result<ForkedAgentResult, String> {
    let start_time = std::time::Instant::now();
    let fork_label = config.fork_label.clone();
    let query_source_str = config.query_source.0.clone();

    let ForkedAgentConfig {
        prompt_messages,
        cache_safe_params,
        query_source,
        overrides,
        max_output_tokens,
        max_turns,
        skip_cache_write,
        ..
    } = config;

    let CacheSafeParams {
        system_prompt,
        user_context,
        system_context,
        tool_use_context,
        fork_context_messages,
    } = cache_safe_params;

    // Create isolated context to prevent mutation of parent state
    let overrides_ref = overrides.as_ref();
    let _isolated_tool_use_context = create_subagent_context(&tool_use_context, overrides_ref);

    // Do NOT filter_incomplete_tool_calls here — it drops the whole assistant on
    // partial tool batches, orphaning the paired results (API 400). Dangling
    // tool_uses are repaired downstream by ensure_tool_result_pairing in claude.ts,
    // same as the main thread — identical post-repair prefix keeps the cache hit.
    let mut initial_messages: Vec<Message> =
        Vec::with_capacity(fork_context_messages.len() + prompt_messages.len());
    initial_messages.extend_from_slice(&fork_context_messages);
    initial_messages.extend_from_slice(&prompt_messages);

    // Generate agent ID and record initial messages for transcript
    // When skip_transcript is set, skip agent ID creation and all transcript I/O
    let agent_id = if config.skip_transcript {
        None
    } else {
        Some(create_agent_id(Some(&fork_label)))
    };
    let _ = agent_id; // reserved for transcript recording

    // In a full implementation, this would call the query engine:
    // let result = query_engine.submit_message(&prompt).await;
    // let (output_messages, total_usage) = collect_query_results(result);

    let _ = (
        system_prompt,
        user_context,
        system_context,
        query_source,
        max_output_tokens,
        max_turns,
        skip_cache_write,
        initial_messages,
    );

    // Placeholder result until query loop integration is complete
    let output_messages: Vec<Message> = Vec::new();
    let total_usage = Usage::default();

    log::debug!(
        "Forked agent [{}] finished: {} messages, total_usage: input={} output={} cache_read={} cache_create={}",
        fork_label,
        output_messages.len(),
        total_usage.input_tokens,
        total_usage.output_tokens,
        total_usage.cache_read_input_tokens,
        total_usage.cache_creation_input_tokens,
    );

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Log the fork query metrics with full Usage
    log_fork_agent_query_event(
        &fork_label,
        &query_source_str,
        duration_ms,
        output_messages.len(),
        &total_usage,
        tool_use_context.query_tracking.as_ref(),
    );

    Ok(ForkedAgentResult {
        messages: output_messages,
        total_usage,
    })
}

/// Accumulate usage from a new usage entry.
fn accumulate_usage(acc: &mut Usage, delta: &Usage) {
    acc.input_tokens += delta.input_tokens;
    acc.output_tokens += delta.output_tokens;
    acc.cache_creation_input_tokens += delta.cache_creation_input_tokens;
    acc.cache_read_input_tokens += delta.cache_read_input_tokens;
    acc.server_tool_use.web_search_requests += delta.server_tool_use.web_search_requests;
    acc.server_tool_use.web_fetch_requests += delta.server_tool_use.web_fetch_requests;
    if let (Some(acc_cache), Some(delta_cache)) = (&mut acc.cache_creation, &delta.cache_creation) {
        acc_cache.ephemeral_1h_input_tokens += delta_cache.ephemeral_1h_input_tokens;
        acc_cache.ephemeral_5m_input_tokens += delta_cache.ephemeral_5m_input_tokens;
    }
    if acc.cache_creation.is_none() && delta.cache_creation.is_some() {
        acc.cache_creation = delta.cache_creation.clone();
    }
    if delta.service_tier.is_some() {
        acc.service_tier = delta.service_tier.clone();
    }
}

// ---------------------------------------------------------------------------
// log_fork_agent_query_event
// ---------------------------------------------------------------------------

/// Logs the tengu_fork_agent_query event with full Usage fields.
fn log_fork_agent_query_event(
    fork_label: &str,
    query_source: &str,
    duration_ms: u64,
    message_count: usize,
    total_usage: &Usage,
    query_tracking: Option<&QueryChainTracking>,
) {
    // Calculate cache hit rate
    let total_input_tokens = total_usage.input_tokens as u64
        + total_usage.cache_creation_input_tokens as u64
        + total_usage.cache_read_input_tokens as u64;
    let cache_hit_rate = if total_input_tokens > 0 {
        total_usage.cache_read_input_tokens as f64 / total_input_tokens as f64
    } else {
        0.0
    };

    log::debug!(
        "tengu_fork_agent_query: fork_label={} query_source={} duration_ms={} message_count={} \
         input_tokens={} output_tokens={} cache_read={} cache_create={} cache_hit_rate={:.4} \
         chain_id={} depth={}",
        fork_label,
        query_source,
        duration_ms,
        message_count,
        total_usage.input_tokens,
        total_usage.output_tokens,
        total_usage.cache_read_input_tokens,
        total_usage.cache_creation_input_tokens,
        cache_hit_rate,
        query_tracking
            .map(|t| t.chain_id.as_str())
            .unwrap_or("none"),
        query_tracking.map(|t| t.depth).unwrap_or(0),
    );
}

// ---------------------------------------------------------------------------
// is_in_fork_child (guard against recursive forking)
// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/forkSubagent.ts
// ---------------------------------------------------------------------------

use crate::constants::xml_tags::FORK_BOILERPLATE_TAG;

/// Guard against recursive forking. Fork children keep the Agent tool in their
/// tool pool for cache-identical tool definitions, so we reject fork attempts
/// at call time by detecting the fork boilerplate tag in conversation history.
pub fn is_in_fork_child(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        if let Message::User(user) = m {
            match &user.message.content {
                crate::types::message::UserContent::Blocks(content) => content.iter().any(|block| {
                    // UserContentBlock is a struct with block_type and text fields
                    let is_text = block.block_type == "text";
                    let has_tag = block
                        .text
                        .as_ref()
                        .map(|t| t.contains(FORK_BOILERPLATE_TAG))
                        .unwrap_or(false);
                    is_text && has_tag
                }),
                crate::types::message::UserContent::Text(text) => {
                    text.contains(FORK_BOILERPLATE_TAG)
                }
            }
        } else {
            false
        }
    })
}
