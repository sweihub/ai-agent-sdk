// Source: ~/claudecode/openclaudecode/src/utils/hooks/execAgentHook.ts
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::types::Message;
use crate::utils::hooks::helpers::{add_arguments_to_prompt, hook_response_json_schema};
use crate::utils::hooks::hook_helpers::SYNTHETIC_OUTPUT_TOOL_NAME;
use crate::utils::hooks::session_hooks::clear_session_hooks;

/// Maximum number of turns for an agent hook
const MAX_AGENT_TURNS: usize = 50;

/// Result of a hook execution
pub enum HookResult {
    Success {
        hook_name: String,
        hook_event: String,
        tool_use_id: String,
    },
    Blocking {
        blocking_error: String,
        command: String,
    },
    Cancelled,
    NonBlockingError {
        hook_name: String,
        hook_event: String,
        tool_use_id: String,
        stderr: String,
        stdout: String,
        exit_code: i32,
    },
}

/// Represents an agent hook configuration
pub struct AgentHook {
    /// The prompt to send to the agent
    pub prompt: String,
    /// Optional timeout in seconds
    pub timeout: Option<u64>,
    /// Optional model override
    pub model: Option<String>,
}

/// Execute an agent-based hook using a multi-turn LLM query
pub async fn exec_agent_hook(
    hook: &AgentHook,
    hook_name: &str,
    hook_event: &str,
    json_input: &str,
    signal: tokio::sync::watch::Receiver<bool>,
    tool_use_context: Arc<crate::utils::hooks::can_use_tool::ToolUseContext>,
    tool_use_id: Option<String>,
    _messages: &[Message],
    agent_name: Option<&str>,
) -> HookResult {
    let effective_tool_use_id = tool_use_id.unwrap_or_else(|| format!("hook-{}", Uuid::new_v4()));

    // Get transcript path from context
    let transcript_path = format!("session_{}_transcript.json", tool_use_context.session_id);

    let hook_start = std::time::Instant::now();

    // Replace $ARGUMENTS with the JSON input
    let processed_prompt = add_arguments_to_prompt(&hook.prompt, json_input);
    log_for_debugging(&format!(
        "Hooks: Processing agent hook with prompt: {}",
        processed_prompt.chars().take(200).collect::<String>()
    ));

    // Create user message
    let user_message = create_user_message(&processed_prompt);
    let mut agent_messages = vec![user_message];

    log_for_debugging(&format!(
        "Hooks: Starting agent query with {} messages",
        agent_messages.len()
    ));

    // Setup timeout
    let hook_timeout_ms = hook.timeout.map_or(60_000, |t| t * 1000);

    // Create abort controller
    let (abort_tx, abort_rx) = tokio::sync::watch::channel(false);

    // Combine parent signal with timeout
    let abort_tx_clone = abort_tx.clone();
    let timeout_handle = tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(hook_timeout_ms)).await;
        let _ = abort_tx_clone.send(true);
    });

    // Get model
    let model = hook.model.clone().unwrap_or_else(get_small_fast_model);

    // Create unique agent ID for this hook agent
    let hook_agent_id = format!("hook-agent-{}", Uuid::new_v4());

    // Create a modified tool use context for the agent
    let agent_tool_use_context = Arc::new(crate::utils::hooks::can_use_tool::ToolUseContext {
        session_id: format!("{}-{}", tool_use_context.session_id, hook_agent_id),
        cwd: tool_use_context.cwd.clone(),
        is_non_interactive_session: true,
        options: Some(crate::utils::hooks::can_use_tool::ToolUseContextOptions {
            tools: Some(Vec::new()), // Would include filtered tools + structured output tool
        }),
    });

    // Register a session-level stop hook to enforce structured output
    register_structured_output_enforcement_impl(&hook_agent_id);

    let mut structured_output_result: Option<serde_json::Value> = None;
    let mut turn_count = 0;
    let mut hit_max_turns = false;

    // Simulate multi-turn query loop
    // In the TS version, this uses query() for multi-turn execution
    // Here we'd use the crate's query function
    for message in simulate_query_loop(&agent_messages, &transcript_path, &model).await {
        // Skip streaming events
        if message.get("type") == Some(&serde_json::json!("stream_event"))
            || message.get("type") == Some(&serde_json::json!("stream_request_start"))
        {
            continue;
        }

        // Count assistant turns
        if message.get("type") == Some(&serde_json::json!("assistant")) {
            turn_count += 1;

            // Check if we've hit the turn limit
            if turn_count >= MAX_AGENT_TURNS {
                hit_max_turns = true;
                log_for_debugging(&format!(
                    "Hooks: Agent turn {} hit max turns, aborting",
                    turn_count
                ));
                let _ = abort_tx.send(true);
                break;
            }
        }

        // Check for structured output in attachments
        if let Some(attachment) = message.get("attachment") {
            if let Some(attachment_type) = attachment.get("type") {
                if attachment_type == "structured_output" {
                    if let Some(data) = attachment.get("data") {
                        // Validate against hook response schema
                        if let Ok(parsed) = serde_json::from_value::<
                            crate::utils::hooks::hook_helpers::HookResponse,
                        >(data.clone())
                        {
                            structured_output_result = Some(data.clone());
                            log_for_debugging(&format!(
                                "Hooks: Got structured output: {}",
                                serde_json::to_string(data).unwrap_or_default()
                            ));
                            // Got structured output, abort and exit
                            let _ = abort_tx.send(true);
                            break;
                        }
                    }
                }
            }
        }

        // Check abort signal
        if *abort_rx.borrow() {
            break;
        }
    }

    timeout_handle.abort();

    // Clean up the session hook we registered for this agent
    clear_session_hooks_impl(&hook_agent_id);

    // Check if we got a result
    if structured_output_result.is_none() {
        if hit_max_turns {
            log_for_debugging(&format!(
                "Hooks: Agent hook did not complete within {} turns",
                MAX_AGENT_TURNS
            ));
            log_event(
                "tengu_agent_stop_hook_max_turns",
                &serde_json::json!({
                    "duration_ms": hook_start.elapsed().as_millis(),
                    "turn_count": turn_count,
                    "agent_name": agent_name.unwrap_or("unknown"),
                }),
            );
            return HookResult::Cancelled;
        }

        log_for_debugging("Hooks: Agent hook did not return structured output");
        log_event(
            "tengu_agent_stop_hook_error",
            &serde_json::json!({
                "duration_ms": hook_start.elapsed().as_millis(),
                "turn_count": turn_count,
                "error_type": 1, // 1 = no structured output
                "agent_name": agent_name.unwrap_or("unknown"),
            }),
        );
        return HookResult::Cancelled;
    }

    // Return result based on structured output
    let result = structured_output_result.unwrap();
    if let Some(ok) = result.get("ok").and_then(|v| v.as_bool()) {
        if !ok {
            let reason = result
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            log_for_debugging(&format!(
                "Hooks: Agent hook condition was not met: {}",
                reason
            ));
            return HookResult::Blocking {
                blocking_error: format!("Agent hook condition was not met: {}", reason),
                command: hook.prompt.clone(),
            };
        }

        // Condition was met
        log_for_debugging("Hooks: Agent hook condition was met");
        log_event(
            "tengu_agent_stop_hook_success",
            &serde_json::json!({
                "duration_ms": hook_start.elapsed().as_millis(),
                "turn_count": turn_count,
                "agent_name": agent_name.unwrap_or("unknown"),
            }),
        );
        return HookResult::Success {
            hook_name: hook_name.to_string(),
            hook_event: hook_event.to_string(),
            tool_use_id: effective_tool_use_id,
        };
    }

    HookResult::Cancelled
}

/// Create a user message with the given content
fn create_user_message(content: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "message": {
            "content": content
        }
    })
}

/// Get the small fast model (simplified)
fn get_small_fast_model() -> String {
    "claude-3-haiku-20240307".to_string()
}

/// Execute a real multi-turn agent query loop using the QueryEngine.
/// Collects events via on_event callback and returns them as JSON messages
/// compatible with the exec_agent_hook consumer (assistant turns, structured
/// output attachments, done event).
async fn simulate_query_loop(
    messages: &[serde_json::Value],
    transcript_path: &str,
    model: &str,
) -> Vec<serde_json::Value> {
    use crate::agent::register_all_tool_executors;
    use crate::query_engine::{QueryEngine, QueryEngineConfig};
    use crate::types::AgentEvent;

    // Channel for streaming events from the spawned query task back to this function.
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<AgentEvent>(256);

    // Build system prompt matching the TypeScript version's instruction template.
    let system_prompt = format!(
        "You are verifying a stop condition in Claude Code. Your task is to verify that \
         the agent completed the given plan. The conversation transcript is available at: \
         {transcript_path}\nYou can read this file to analyze the conversation history if needed.

Use the available tools to inspect the codebase and verify the condition.
Use as few steps as possible - be efficient and direct.

When done, return your result using the {tool} tool with:
- ok: true if the condition is met
- ok: false with reason if the condition is not met",
        transcript_path = transcript_path,
        tool = SYNTHETIC_OUTPUT_TOOL_NAME,
    );

    // Extract prompt text from input messages (same shape as create_user_message output).
    let prompt = messages
        .iter()
        .filter_map(|m| {
            Some(
                m.get("message")
                    .and_then(|msg| msg.get("content"))
                    .or_else(|| m.get("content"))?
                    .as_str()?
                    .to_string(),
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    // Resolve API credentials.
    let api_key = std::env::var("AI_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .ok();

    // If no API key is available, drop the sender so the receiver closes immediately
    // and the caller's for-await loop exits cleanly.
    if api_key.is_none() {
        log_for_debugging("Hooks: No API key available, skipping agent query");
        drop(sender);
        return Vec::new();
    }

    let api_key = api_key.unwrap();

    // Create abort controller that will be cloned into the engine.
    let abort_controller = crate::utils::abort_controller::create_abort_controller_default();

    // Build the QueryEngine config with an on_event callback that forwards every
    // AgentEvent through the mpsc channel.
    let on_event = {
        let ch = sender.clone();
        Some(Arc::new({
            move |event: AgentEvent| {
                let _ = ch.blocking_send(event);
            }
        }) as Arc<dyn Fn(AgentEvent) + Send + Sync>)
    };

    let mut engine = QueryEngine::new(QueryEngineConfig {
        cwd: std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string()),
        model: model.to_string(),
        api_key: Some(api_key),
        base_url: std::env::var("AI_API_BASE_URL").ok(),
        tools: Vec::new(),
        system_prompt: Some(system_prompt),
        max_turns: 5,
        max_budget_usd: None,
        max_tokens: 4096,
        fallback_model: None,
        user_context: std::collections::HashMap::new(),
        system_context: std::collections::HashMap::new(),
        can_use_tool: None,
        on_event,
        thinking: Some(crate::types::api_types::ThinkingConfig::Disabled),
        abort_controller: None,
        token_budget: None,
        agent_id: Some("hook-agent".to_string()),
        loaded_nested_memory_paths: HashSet::new(),
    });

    // Spawn the query task so events flow asynchronously through the channel.
    let ac = Arc::new(abort_controller);
    let task = tokio::spawn({
        let ac = Arc::clone(&ac);
        async move {
            // Register all built-in tool executors (includes StructuredOutput).
            register_all_tool_executors(&mut engine);

            // Override the engine's abort controller so the caller's abort signal
            // can stop the query loop.
            // (The engine stores its own AbortController; we signal it via the
            // watch channel in exec_agent_hook which the loop checks.)

            let _ = engine.submit_message(&prompt).await;

            // Signal completion so the receiver loop can exit.
            // The sender will also be dropped when this task ends.
        }
    });

    // Wait for the query task with a timeout to prevent indefinite hangs.
    let task_handle = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        task,
    );

    // Spawn a background task that waits for the query and then drops the sender.
    let sender_for_cleanup = sender.clone();
    let _cleanup = tokio::spawn(async move {
        let _ = task_handle.await;
        drop(sender_for_cleanup);
    });

    // Collect events from the channel, converting each AgentEvent to the JSON
    // message format expected by the caller's loop.
    let mut result = Vec::new();
    while let Some(event) = receiver.recv().await {
        let json = match event {
            AgentEvent::Thinking { .. } => {
                // Maps to "assistant" type for turn counting.
                serde_json::json!({ "type": "assistant" })
            }
            AgentEvent::ToolStart {
                tool_name, input, ..
            } if tool_name == SYNTHETIC_OUTPUT_TOOL_NAME => {
                // Structured output tool called — emit as attachment so the
                // caller detects it and breaks the loop.
                serde_json::json!({
                    "type": "attachment",
                    "attachment": {
                        "type": "structured_output",
                        "data": input,
                    }
                })
            }
            AgentEvent::ToolStart { .. } => {
                // Other tool starts — mapped to generic stream_event.
                serde_json::json!({ "type": "stream_event" })
            }
            AgentEvent::ToolComplete { .. }
            | AgentEvent::ToolError { .. }
            | AgentEvent::ContentBlockStart { .. }
            | AgentEvent::ContentBlockDelta { .. }
            | AgentEvent::ContentBlockStop { .. }
            | AgentEvent::MessageStart { .. }
            | AgentEvent::MessageStop
            | AgentEvent::RequestStart
            | AgentEvent::StreamRequestEnd
            | AgentEvent::RateLimitStatus { .. }
            | AgentEvent::MaxTurnsReached { .. }
            | AgentEvent::Tombstone { .. } => {
                // Streaming and internal events — mapped to generic stream_event.
                serde_json::json!({ "type": "stream_event" })
            }
            AgentEvent::Done { .. } => {
                // Query loop finished — emit done event.
                serde_json::json!({ "type": "done" })
            }
        };
        result.push(json);
    }

    result
}

/// No-op set_app_state for use with session hook functions that require a
/// state setter.  The real session-hook state lives in an internal static,
/// so this placeholder is sufficient.
fn noop_set_app_state(_updater: &dyn Fn(&mut serde_json::Value)) {
    // No-op — internal SESSION_HOOKS_STATE handles the actual storage.
}

/// Register structured output enforcement for the given session/agent ID.
/// Wraps the hook_helpers function with a no-op set_app_state.
fn register_structured_output_enforcement_impl(session_id: &str) {
    crate::utils::hooks::hook_helpers::register_structured_output_enforcement(
        &noop_set_app_state,
        session_id,
    );
}

/// Clear session hooks for the given session/agent ID.
/// Wraps the session_hooks function with a no-op set_app_state.
fn clear_session_hooks_impl(session_id: &str) {
    clear_session_hooks(&noop_set_app_state, session_id);
}

/// Log event for analytics (simplified)
fn log_event(event_name: &str, _metadata: &serde_json::Value) {
    log::debug!("Analytics event: {}", event_name);
}

/// Log for debugging
fn log_for_debugging(msg: &str) {
    log::debug!("{}", msg);
}
