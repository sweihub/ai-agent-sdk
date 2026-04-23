// Source: /data/home/swei/claudecode/openclaudecode/src/services/tools/toolOrchestration.ts
//! Tool orchestration module for running tools with concurrency control.
//!
//! Translated from TypeScript toolOrchestration.ts

use crate::AgentError;
use crate::constants::env::ai;
use crate::types::{
    Message, MessageRole, ToolAnnotations, ToolCall, ToolDefinition, ToolInputSchema, ToolResult,
};
use futures_util::stream::{self, StreamExt};
use serde::Serialize;

use crate::tool_errors::format_tool_error;
use crate::tool_result_storage::process_tool_result;
use crate::tool_validation::validate_tool_input;

/// Maximum number of concurrent tool executions (matches TypeScript default)
pub const MAX_TOOL_USE_CONCURRENCY: usize = 10;

/// Get max tool use concurrency from environment variable
pub fn get_max_tool_use_concurrency() -> usize {
    std::env::var(ai::MAX_TOOL_USE_CONCURRENCY)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(MAX_TOOL_USE_CONCURRENCY)
}

/// A batch of tool calls that can be executed together
#[derive(Debug, Clone)]
pub struct ToolBatch {
    /// Whether this batch is concurrency-safe (can run in parallel)
    pub is_concurrency_safe: bool,
    /// The tool calls in this batch
    pub blocks: Vec<ToolCall>,
}

/// Modifier for tool use context (for contextModifier support).
#[derive(Debug, Clone)]
pub struct ContextModifier {
    pub tool_use_id: String,
    pub modify_context: fn(crate::types::ToolContext) -> crate::types::ToolContext,
}

/// Message update for tool orchestration
#[derive(Debug, Clone)]
pub struct ToolMessageUpdate {
    /// The message to add to the conversation
    pub message: Option<Message>,
    /// Updated context after this tool (for serial execution)
    pub new_context: Option<crate::types::ToolContext>,
    /// Context modifiers for this tool result (for contextModifier support)
    pub context_modifier: Option<ContextModifier>,
}

/// Partition tool calls into batches where each batch is either:
/// 1. A single non-concurrency-safe tool, or
/// 2. Multiple consecutive concurrency-safe tools
pub fn partition_tool_calls(tool_calls: &[ToolCall], tools: &[ToolDefinition]) -> Vec<ToolBatch> {
    let mut batches: Vec<ToolBatch> = Vec::new();

    for tool_use in tool_calls {
        // Find the tool definition
        let tool = tools.iter().find(|t| t.name == tool_use.name);

        // Check concurrency safety
        // Matches TypeScript: use the tool's isConcurrencySafe method
        // If tool not found or isConcurrencySafe throws, treat as not concurrency-safe
        let is_concurrency_safe = tool
            .map(|t| t.is_concurrency_safe(&tool_use.arguments))
            .unwrap_or(false);

        // Check if we can add to the last batch
        if is_concurrency_safe {
            if let Some(last) = batches.last_mut() {
                if last.is_concurrency_safe {
                    // Add to existing concurrency-safe batch
                    last.blocks.push(tool_use.clone());
                    continue;
                }
            }
        }

        // Create new batch (either non-concurrency-safe or first in a concurrency-safe group)
        batches.push(ToolBatch {
            is_concurrency_safe,
            blocks: vec![tool_use.clone()],
        });
    }

    batches
}

/// Mark a tool use as complete (removes from in-progress set)
pub fn mark_tool_use_as_complete(
    in_progress_ids: &mut std::collections::HashSet<String>,
    tool_use_id: &str,
) {
    in_progress_ids.remove(tool_use_id);
}

/// Run tools serially (for non-concurrency-safe tools)
/// This matches TypeScript's runToolsSerially function
pub async fn run_tools_serially<F, Fut>(
    tool_calls: Vec<ToolCall>,
    tool_context: crate::types::ToolContext,
    tools: Vec<ToolDefinition>,
    mut executor: F,
    project_dir: Option<String>,
    session_id: Option<String>,
) -> Vec<ToolMessageUpdate>
where
    F: FnMut(String, serde_json::Value, String) -> Fut + Send,
    Fut: Future<Output = Result<crate::types::ToolResult, AgentError>> + Send,
{
    let mut updates = Vec::new();
    let mut current_context = tool_context;
    let mut in_progress_ids = std::collections::HashSet::new();

    for tool_call in tool_calls {
        let tool_name = tool_call.name.clone();
        let tool_args = tool_call.arguments.clone();
        let tool_call_id = tool_call.id.clone();

        // Mark tool as in progress
        in_progress_ids.insert(tool_call_id.clone());

        // Check abort signal before executing each tool (interruptBehavior: block default)
        if current_context.abort_signal.is_aborted() {
            let error_content =
                "<tool_use_error>Tool execution aborted by user interrupt</tool_use_error>"
                    .to_string();
            updates.push(ToolMessageUpdate {
                message: Some(Message {
                    role: MessageRole::Tool,
                    content: error_content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: Some(true),
                    ..Default::default()
                }),
                new_context: Some(current_context.clone()),
                context_modifier: None,
            });
            mark_tool_use_as_complete(&mut in_progress_ids, &tool_call_id);
            continue;
        }

        // Input validation (matches TS Zod schema validation)
        if let Err(validation_err) = validate_tool_input(&tool_name, &tool_args, &tools) {
            let error_content = format!(
                "<tool_use_error>InputValidationError: {}</tool_use_error>",
                validation_err
            );
            updates.push(ToolMessageUpdate {
                message: Some(Message {
                    role: MessageRole::Tool,
                    content: error_content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: Some(true),
                    ..Default::default()
                }),
                new_context: Some(current_context.clone()),
                context_modifier: None,
            });
            mark_tool_use_as_complete(&mut in_progress_ids, &tool_call_id);
            continue;
        }

        // Execute the tool (pass tool_call_id)
        match executor(tool_name.clone(), tool_args.clone(), tool_call_id.clone()).await {
            Ok(mut result) => {
                // Large result persistence (matches TS processToolResultBlock)
                let persisted = process_tool_result(
                    &result.content,
                    &tool_name,
                    &tool_call_id,
                    project_dir.as_deref(),
                    session_id.as_deref(),
                    None, // Use default threshold
                );
                result.content = persisted.0;
                result.was_persisted = Some(persisted.1);

                let message = Message {
                    role: MessageRole::Tool,
                    content: result.content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: result.is_error,
                    ..Default::default()
                };

                updates.push(ToolMessageUpdate {
                    message: Some(message),
                    new_context: Some(current_context.clone()),
                    context_modifier: None,
                });
            }
            Err(e) => {
                // Format error using tool_errors (matches TS formatError)
                let error_content = format!(
                    "<tool_use_error>Error: {}</tool_use_error>",
                    format_tool_error(&e)
                );
                let message = Message {
                    role: MessageRole::Tool,
                    content: error_content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: Some(true),
                    ..Default::default()
                };

                updates.push(ToolMessageUpdate {
                    message: Some(message),
                    new_context: Some(current_context.clone()),
                    context_modifier: None,
                });
            }
        }

        // Mark tool as complete
        mark_tool_use_as_complete(&mut in_progress_ids, &tool_call_id);
    }

    updates
}

/// Run tools concurrently (for concurrency-safe tools)
/// Uses the all() generator pattern from TypeScript with concurrency limit
pub async fn run_tools_concurrently<F, Fut>(
    tool_calls: Vec<ToolCall>,
    tool_context: crate::types::ToolContext,
    tools: Vec<ToolDefinition>,
    mut executor: F,
    project_dir: Option<String>,
    session_id: Option<String>,
) -> Vec<ToolMessageUpdate>
where
    F: FnMut(String, serde_json::Value, String) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<crate::types::ToolResult, AgentError>> + Send,
{
    let max_concurrency = get_max_tool_use_concurrency();
    let mut updates = Vec::new();

    // Create a stream of tool executions
    let executions: Vec<_> = tool_calls
        .into_iter()
        .map(|tool_call| {
            let mut exec = executor.clone();
            let tool_name = tool_call.name.clone();
            let tool_args = tool_call.arguments.clone();
            let tool_call_id = tool_call.id.clone();
            let tools = tools.clone();
            let project_dir = project_dir.clone();
            let session_id = session_id.clone();
            let abort_signal = tool_context.abort_signal.clone();

            async move {
                // Check abort signal before concurrent tool execution (all concurrent tools treated as cancel)
                if abort_signal.is_aborted() {
                    return (
                        tool_call_id,
                        Err(AgentError::Tool("Tool execution aborted by user interrupt".to_string())),
                    );
                }

                // Input validation
                if let Err(validation_err) = validate_tool_input(&tool_name, &tool_args, &tools) {
                    let error_content = format!(
                        "<tool_use_error>InputValidationError: {}</tool_use_error>",
                        validation_err
                    );
                    return (
                        tool_call_id,
                        Err(AgentError::Tool(format!(
                            "InputValidationError: {}",
                            validation_err
                        ))),
                    );
                }
                let result = exec(tool_name.clone(), tool_args, tool_call_id.clone()).await;
                (tool_call_id, result)
            }
        })
        .collect();

    // Run with bounded concurrency using buffer_unordered
    let mut stream = stream::iter(executions).buffer_unordered(max_concurrency);

    while let Some((tool_call_id, result)) = stream.next().await {
        match result {
            Ok(tool_result) => {
                // Large result persistence
                let (content, _) = process_tool_result(
                    &tool_result.content,
                    "", // tool name not tracked in concurrent path
                    &tool_call_id,
                    project_dir.as_deref(),
                    session_id.as_deref(),
                    None,
                );
                let message = Message {
                    role: MessageRole::Tool,
                    content,
                    tool_call_id: Some(tool_call_id),
                    ..Default::default()
                };

                updates.push(ToolMessageUpdate {
                    message: Some(message),
                    new_context: None,
                    context_modifier: None,
                });
            }
            Err(e) => {
                let error_content = format!(
                    "<tool_use_error>Error: {}</tool_use_error>",
                    format_tool_error(&e)
                );
                let message = Message {
                    role: MessageRole::Tool,
                    content: error_content,
                    tool_call_id: Some(tool_call_id),
                    is_error: Some(true),
                    ..Default::default()
                };

                updates.push(ToolMessageUpdate {
                    message: Some(message),
                    new_context: None,
                    context_modifier: None,
                });
            }
        }
    }

    updates
}

/// Run all tools with proper partitioning and concurrency
/// This is the main entry point that matches TypeScript's runTools()
pub async fn run_tools<F, Fut>(
    tool_calls: Vec<ToolCall>,
    tools: Vec<ToolDefinition>,
    tool_context: crate::types::ToolContext,
    executor: F,
    project_dir: Option<String>,
    session_id: Option<String>,
) -> Vec<ToolMessageUpdate>
where
    F: FnMut(String, serde_json::Value, String) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<crate::types::ToolResult, AgentError>> + Send,
{
    let batches = partition_tool_calls(&tool_calls, &tools);
    let mut all_updates = Vec::new();
    let mut current_context = tool_context;

    for batch in batches {
        let tools_clone = tools.clone();
        let project_dir_clone = project_dir.clone();
        let session_id_clone = session_id.clone();

        if batch.is_concurrency_safe {
            // Run concurrency-safe batch concurrently
            let updates = run_tools_concurrently(
                batch.blocks,
                current_context.clone(),
                tools_clone,
                executor.clone(),
                project_dir_clone,
                session_id_clone,
            )
            .await;
            all_updates.extend(updates);
        } else {
            // Run non-concurrency-safe batch serially
            let updates = run_tools_serially(
                batch.blocks,
                current_context.clone(),
                tools_clone,
                executor.clone(),
                project_dir_clone,
                session_id_clone,
            )
            .await;

            // Update context after serial execution
            if let Some(last_update) = updates.last() {
                if let Some(ctx) = &last_update.new_context {
                    current_context = ctx.clone();
                }
            }

            all_updates.extend(updates);
        }
    }

    all_updates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolInputSchema;

    fn create_test_tool(name: &str, concurrency_safe: bool) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("Test tool {}", name),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
            },
            annotations: if concurrency_safe {
                Some(ToolAnnotations {
                    concurrency_safe: Some(true),
                    ..Default::default()
                })
            } else {
                None
            },
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
            aliases: None,
            user_facing_name: None,
            interrupt_behavior: None,
        }
    }

    #[test]
    fn test_get_max_tool_use_concurrency_default() {
        // Without env var, should return default
        // Note: In real env with no var set, returns default
        assert_eq!(get_max_tool_use_concurrency(), MAX_TOOL_USE_CONCURRENCY);
    }

    #[test]
    fn test_get_max_tool_use_concurrency_value() {
        // Just test that the function returns a value
        let result = get_max_tool_use_concurrency();
        assert!(result > 0);
    }

    #[test]
    fn test_partition_tool_calls_all_non_safe() {
        let tool_calls = vec![
            ToolCall {
                id: "1".to_string(),
                r#type: "function".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "2".to_string(),
                r#type: "function".to_string(),
                name: "Edit".to_string(),
                arguments: serde_json::json!({}),
            },
        ];
        let tools = vec![
            create_test_tool("Bash", false),
            create_test_tool("Edit", false),
        ];

        let batches = partition_tool_calls(&tool_calls, &tools);
        assert_eq!(batches.len(), 2);
        assert!(!batches[0].is_concurrency_safe);
        assert!(!batches[1].is_concurrency_safe);
    }

    #[test]
    fn test_partition_tool_calls_mixed() {
        let tool_calls = vec![
            ToolCall {
                id: "1".to_string(),
                r#type: "function".to_string(),
                name: "Read".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "2".to_string(),
                r#type: "function".to_string(),
                name: "Glob".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "3".to_string(),
                r#type: "function".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "4".to_string(),
                r#type: "function".to_string(),
                name: "Grep".to_string(),
                arguments: serde_json::json!({}),
            },
        ];
        let tools = vec![
            create_test_tool("Read", true),
            create_test_tool("Glob", true),
            create_test_tool("Bash", false),
            create_test_tool("Grep", true),
        ];

        let batches = partition_tool_calls(&tool_calls, &tools);
        // Should be: [Read,Glob] (concurrency safe), [Bash] (non-safe), [Grep] (concurrency safe)
        assert_eq!(batches.len(), 3);
        assert!(batches[0].is_concurrency_safe);
        assert_eq!(batches[0].blocks.len(), 2);
        assert!(!batches[1].is_concurrency_safe);
        assert!(batches[2].is_concurrency_safe);
    }

    #[test]
    fn test_partition_tool_calls_with_unknown_tool() {
        let tool_calls = vec![ToolCall {
            id: "1".to_string(),
            r#type: "function".to_string(),
            name: "UnknownTool".to_string(),
            arguments: serde_json::json!({}),
        }];
        let tools = vec![];

        let batches = partition_tool_calls(&tool_calls, &tools);
        assert_eq!(batches.len(), 1);
        // Unknown tools should be treated as not concurrency-safe
        assert!(!batches[0].is_concurrency_safe);
    }

    #[tokio::test]
    async fn test_run_tools_serially() {
        let tool_calls = vec![ToolCall {
            id: "1".to_string(),
            r#type: "function".to_string(),
            name: "test".to_string(),
            arguments: serde_json::json!({}),
        }];

        let tool_context = crate::types::ToolContext::default();
        let tools = vec![create_test_tool("test", false)];

        let executor = |_name: String, _args: serde_json::Value, _tool_call_id: String| async {
            Ok(crate::types::ToolResult {
                result_type: "tool_result".to_string(),
                tool_use_id: "1".to_string(),
                content: "success".to_string(),
                is_error: Some(false),
                was_persisted: Some(false),
            })
        };

        let updates =
            run_tools_serially(tool_calls, tool_context, tools, executor, None, None).await;
        assert_eq!(updates.len(), 1);
        assert!(updates[0].message.is_some());
    }

    #[tokio::test]
    async fn test_run_tools_concurrently() {
        let tool_calls = vec![
            ToolCall {
                id: "1".to_string(),
                r#type: "function".to_string(),
                name: "test1".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "2".to_string(),
                r#type: "function".to_string(),
                name: "test2".to_string(),
                arguments: serde_json::json!({}),
            },
        ];

        let tool_context = crate::types::ToolContext::default();
        let tools = vec![
            create_test_tool("test1", true),
            create_test_tool("test2", true),
        ];

        let executor = |_name: String, _args: serde_json::Value, _tool_call_id: String| async {
            Ok(crate::types::ToolResult {
                result_type: "tool_result".to_string(),
                tool_use_id: "1".to_string(),
                content: "success".to_string(),
                is_error: Some(false),
                was_persisted: Some(false),
            })
        };

        let updates =
            run_tools_concurrently(tool_calls, tool_context, tools, executor, None, None).await;
        assert_eq!(updates.len(), 2);
    }

    #[tokio::test]
    async fn test_run_tools_with_partitioning() {
        let tool_calls = vec![
            ToolCall {
                id: "1".to_string(),
                r#type: "function".to_string(),
                name: "Read".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "2".to_string(),
                r#type: "function".to_string(),
                name: "Glob".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "3".to_string(),
                r#type: "function".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({}),
            },
        ];
        let tools = vec![
            create_test_tool("Read", true),
            create_test_tool("Glob", true),
            create_test_tool("Bash", false),
        ];

        let tool_context = crate::types::ToolContext::default();

        let executor = |_name: String, _args: serde_json::Value, _tool_call_id: String| async {
            Ok(crate::types::ToolResult {
                result_type: "tool_result".to_string(),
                tool_use_id: "1".to_string(),
                content: "success".to_string(),
                is_error: Some(false),
                was_persisted: Some(false),
            })
        };

        let updates = run_tools(tool_calls, tools, tool_context, executor, None, None).await;
        assert_eq!(updates.len(), 3);
    }

    #[test]
    fn test_mark_tool_use_as_complete() {
        let mut in_progress = std::collections::HashSet::new();
        in_progress.insert("tool1".to_string());
        in_progress.insert("tool2".to_string());

        mark_tool_use_as_complete(&mut in_progress, "tool1");

        assert!(!in_progress.contains("tool1"));
        assert!(in_progress.contains("tool2"));
    }

    #[tokio::test]
    async fn test_run_tools_serially_aborted() {
        use crate::utils::abort_controller::create_abort_controller_default;

        let tool_calls = vec![ToolCall {
            id: "1".to_string(),
            r#type: "function".to_string(),
            name: "test".to_string(),
            arguments: serde_json::json!({}),
        }];

        let controller = create_abort_controller_default();
        controller.abort(None); // Pre-abort
        let abort_signal = controller.signal().clone();

        let tool_context = crate::types::ToolContext {
            cwd: "/tmp".to_string(),
            abort_signal,
        };
        let tools = vec![create_test_tool("test", false)];

        let executor = |_name: String, _args: serde_json::Value, _tool_call_id: String| async {
            Ok(crate::types::ToolResult {
                result_type: "tool_result".to_string(),
                tool_use_id: "1".to_string(),
                content: "should not reach".to_string(),
                is_error: Some(false),
                was_persisted: Some(false),
            })
        };

        let updates =
            run_tools_serially(tool_calls, tool_context, tools, executor, None, None).await;
        assert_eq!(updates.len(), 1);
        let msg = updates[0].message.as_ref().unwrap();
        assert!(msg.is_error == Some(true));
        assert!(msg.content.contains("aborted"));
    }

    #[tokio::test]
    async fn test_run_tools_concurrently_aborted() {
        use crate::utils::abort_controller::create_abort_controller_default;

        let tool_calls = vec![ToolCall {
            id: "1".to_string(),
            r#type: "function".to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({}),
        }];

        let controller = create_abort_controller_default();
        controller.abort(None); // Pre-abort
        let abort_signal = controller.signal().clone();

        let tool_context = crate::types::ToolContext {
            cwd: "/tmp".to_string(),
            abort_signal,
        };
        let tools = vec![create_test_tool("Read", true)];

        let executor = |_name: String, _args: serde_json::Value, _tool_call_id: String| async {
            Ok(crate::types::ToolResult {
                result_type: "tool_result".to_string(),
                tool_use_id: "1".to_string(),
                content: "should not reach".to_string(),
                is_error: Some(false),
                was_persisted: Some(false),
            })
        };

        let updates = run_tools_concurrently(
            tool_calls, tool_context, tools, executor, None, None,
        )
        .await;
        assert_eq!(updates.len(), 1);
        let msg = updates[0].message.as_ref().unwrap();
        assert!(msg.is_error == Some(true));
    }
}
