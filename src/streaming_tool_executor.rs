// Source: /data/home/swei/claudecode/openclaudecode/src/services/tools/StreamingToolExecutor.ts
//! Streaming tool executor that starts executing tools as they stream in from the API.
//!
//! Translated from TypeScript StreamingToolExecutor.ts.
//! - Concurrent-safe tools can execute in parallel with other concurrent-safe tools
//! - Non-concurrent tools must execute alone (exclusive access)
//! - Results are buffered and emitted in the order tools were received

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, Notify};

use crate::types::{Message, MessageRole, ToolAnnotations, ToolCall, ToolDefinition, ToolInputSchema, ToolResult};
pub use crate::tools::orchestration::ToolMessageUpdate;

/// Status of a tracked tool in the execution queue.
#[derive(Debug, Clone, PartialEq)]
enum ToolStatus {
    Queued,
    Executing,
    Completed,
    Yielded,
}

/// A tool being tracked for execution.
#[derive(Clone)]
struct TrackedTool {
    id: String,
    name: String,
    status: ToolStatus,
    is_concurrency_safe: bool,
    args: serde_json::Value,
    /// Results accumulated from this tool (for get_completed_results)
    results: Vec<ToolMessageUpdate>,
}

/// A boxed executor function that takes tool name, args, and call ID.
type ToolExecutorFn = Arc<dyn Fn(String, serde_json::Value, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult, crate::AgentError>> + Send + Sync>> + Send + Sync>;

/// Shared state for the streaming executor.
struct SharedState {
    tools: Vec<TrackedTool>,
    has_errored: bool,
    discarded: bool,
}

/// Executes tools as they stream in with concurrency control.
pub struct StreamingToolExecutor {
    state: Arc<Mutex<SharedState>>,
    executor: ToolExecutorFn,
    tools_def: Vec<ToolDefinition>,
    sibling_abort: Arc<Notify>,
    /// Channel for delivering results to the consumer.
    result_tx: mpsc::UnboundedSender<ToolMessageUpdate>,
    notify: Arc<Notify>,
}

impl StreamingToolExecutor {
    /// Create a new streaming tool executor.
    pub fn new(
        executor: ToolExecutorFn,
        tools_def: Vec<ToolDefinition>,
    ) -> (Self, mpsc::UnboundedReceiver<ToolMessageUpdate>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                state: Arc::new(Mutex::new(SharedState {
                    tools: Vec::new(),
                    has_errored: false,
                    discarded: false,
                })),
                executor,
                tools_def,
                sibling_abort: Arc::new(Notify::new()),
                result_tx: tx,
                notify: Arc::new(Notify::new()),
            },
            rx,
        )
    }

    /// Add a tool to the execution queue. Will start executing immediately if conditions allow.
    pub fn add_tool(&self, name: String, id: String, args: serde_json::Value) {
        let is_concurrency_safe = self
            .tools_def
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.is_concurrency_safe(&args))
            .unwrap_or(false);

        let known = self.tools_def.iter().any(|t| t.name == name);
        let tool = TrackedTool {
            id: id.clone(),
            name: name.clone(),
            status: ToolStatus::Queued,
            is_concurrency_safe,
            args,
            results: Vec::new(),
        };

        // Push to state and process queue in background
        let state = self.state.clone();
        let sibling_abort = self.sibling_abort.clone();
        let executor = self.executor.clone();
        let tools_def = self.tools_def.clone();
        let result_tx = self.result_tx.clone();
        let notify = self.notify.clone();

        tokio::spawn(async move {
            // Check for unknown tool
            if !known {
                let update = create_synthetic_error(&id, "streaming_fallback", &name);
                let mut guard = state.lock().await;
                guard.tools.push(TrackedTool { status: ToolStatus::Completed, results: Vec::new(), ..tool });
                drop(guard);
                result_tx.send(update).ok();
                notify.notify_one();
                return;
            }

            // Update state
            {
                let mut guard = state.lock().await;
                guard.tools.push(tool);
            }

            // Process queue
            process_queue(state, executor, tools_def, result_tx, notify, sibling_abort).await;
        });
    }

    /// Mark a tool use as complete.
    pub async fn mark_complete(&self, tool_use_id: &str) {
        let mut guard = self.state.lock().await;
        if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == tool_use_id) {
            tool.status = ToolStatus::Completed;
        }
        drop(guard);
        self.notify.notify_one();
    }

    /// Get a tool's concurrency safety flag.
    pub async fn get_is_concurrency_safe(&self, tool_use_id: &str) -> bool {
        let guard = self.state.lock().await;
        guard.tools.iter()
            .find(|t| t.id == tool_use_id)
            .map(|t| t.is_concurrency_safe)
            .unwrap_or(false)
    }

    /// Check if there are unfinished tools.
    pub async fn has_unfinished_tools(&self) -> bool {
        let guard = self.state.lock().await;
        guard.tools.iter().any(|t| t.status != ToolStatus::Completed && t.status != ToolStatus::Yielded)
    }

    /// Check if any tools are currently executing.
    pub async fn has_executing_tools(&self) -> bool {
        let guard = self.state.lock().await;
        guard.tools.iter().any(|t| t.status == ToolStatus::Executing)
    }

    /// Discard all pending and in-progress tools.
    pub async fn discard(&self) {
        let to_cancel: Vec<(String, String)> = {
            let mut guard = self.state.lock().await;
            guard.discarded = true;
            guard.tools.iter()
                .filter(|t| t.status == ToolStatus::Queued || t.status == ToolStatus::Executing)
                .map(|t| (t.id.clone(), t.name.clone()))
                .collect()
        };
        for (id, name) in to_cancel {
            let mut guard = self.state.lock().await;
            if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == id) {
                tool.status = ToolStatus::Completed;
            }
            drop(guard);
            self.result_tx.send(create_synthetic_error(&id, "streaming_fallback", &name)).ok();
        }
        self.notify.notify_one();
    }

    /// Trigger sibling abort (called when Bash tool errors).
    pub async fn trigger_sibling_abort(&self) {
        let mut guard = self.state.lock().await;
        guard.has_errored = true;
        let ids: Vec<(String, String)> = guard.tools.iter()
            .filter(|t| t.status == ToolStatus::Executing)
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect();
        drop(guard);

        self.sibling_abort.notify_waiters();
        for (id, name) in ids {
            let update = create_synthetic_error(&id, "sibling_error", &name);
            self.result_tx.send(update).ok();
        }
        self.notify.notify_one();
    }

    /// Set tool result from external execution.
    pub async fn set_tool_result(&self, tool_call_id: String, result: Result<ToolResult, crate::AgentError>) {
        let message = match result {
            Ok(tool_result) => {
                let msg = Message {
                    role: MessageRole::Tool,
                    content: tool_result.content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: tool_result.is_error,
                    ..Default::default()
                };
                ToolMessageUpdate {
                    message: Some(msg),
                    new_context: None,
                    context_modifier: None,
                }
            }
            Err(e) => {
                let error_content = format!("<tool_use_error>Error: {}</tool_use_error>", e);
                let msg = Message {
                    role: MessageRole::Tool,
                    content: error_content,
                    tool_call_id: Some(tool_call_id.clone()),
                    is_error: Some(true),
                    ..Default::default()
                };
                ToolMessageUpdate {
                    message: Some(msg),
                    new_context: None,
                    context_modifier: None,
                }
            }
        };

        // Mark complete (adds to state if missing)
        self.mark_complete(&tool_call_id).await;
        // Store result for get_completed_results
        self.store_result(&tool_call_id, message.clone()).await;
        // Always send the result to the channel
        self.result_tx.send(message).ok();
        self.notify.notify_one();
    }

    /// Store a result in the tracked tool for get_completed_results iteration.
    async fn store_result(&self, tool_call_id: &str, update: ToolMessageUpdate) {
        let mut guard = self.state.lock().await;
        if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == tool_call_id) {
            tool.results.push(update);
        }
    }

    /// Get completed results that haven't been yielded yet.
    /// Yields progress messages immediately, then results in order.
    /// Stops yielding when encountering a non-concurrency-safe executing tool.
    pub async fn get_completed_results(&self) -> Vec<ToolMessageUpdate> {
        let mut guard = self.state.lock().await;
        // Phase 1: collect indices of tools to yield (read-only)
        let to_yield: Vec<(usize, String)> = guard.tools.iter().enumerate()
            .filter_map(|(i, tool)| {
                if tool.status == ToolStatus::Yielded {
                    return None;
                }
                if tool.status == ToolStatus::Executing && !tool.is_concurrency_safe {
                    return None; // Break here
                }
                if tool.status == ToolStatus::Completed && !tool.results.is_empty() {
                    return Some((i, tool.id.clone()));
                }
                None
            })
            .collect();

        // Phase 2: mark as yielded and collect results
        let mut results = Vec::new();
        for (i, _id) in to_yield {
            if let Some(tool) = guard.tools.get_mut(i) {
                tool.status = ToolStatus::Yielded;
                results.append(&mut tool.results);
            }
        }

        results
    }

    /// Wait for remaining tools and collect their results.
    pub async fn get_remaining_results(&self, result_rx: &mut mpsc::UnboundedReceiver<ToolMessageUpdate>) -> Vec<ToolMessageUpdate> {
        let mut all_results = Vec::new();

        // Collect any results already available
        while let Ok(update) = result_rx.try_recv() {
            all_results.push(update);
        }

        // Wait for all tools to complete
        while self.has_unfinished_tools().await {
            self.notify.notified().await;

            // Collect results from channel
            while let Ok(update) = result_rx.try_recv() {
                all_results.push(update);
            }

            // Small delay to avoid busy loop
            if self.has_executing_tools().await {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }

        // Final collection
        while let Ok(update) = result_rx.try_recv() {
            all_results.push(update);
        }

        // Mark all remaining tools as yielded
        {
            let mut guard = self.state.lock().await;
            for tool in guard.tools.iter_mut() {
                if tool.status != ToolStatus::Yielded {
                    tool.status = ToolStatus::Yielded;
                }
            }
        }

        all_results
    }

    /// Discard all pending and in-progress tools.
    pub async fn discard_sync(&self) {
        let mut guard = self.state.lock().await;
        guard.discarded = true;
        let to_cancel: Vec<(String, String)> = guard.tools.iter()
            .filter(|t| t.status == ToolStatus::Queued || t.status == ToolStatus::Executing)
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect();
        drop(guard);

        for (id, name) in to_cancel {
            let mut guard = self.state.lock().await;
            if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == id) {
                tool.status = ToolStatus::Completed;
            }
            drop(guard);
            self.result_tx.send(create_synthetic_error(&id, "streaming_fallback", &name)).ok();
        }
        self.notify.notify_one();
    }
}

/// Process the queue, starting execution for queued tools if allowed.
async fn process_queue(
    state: Arc<Mutex<SharedState>>,
    executor: ToolExecutorFn,
    _tools_def: Vec<ToolDefinition>,
    result_tx: mpsc::UnboundedSender<ToolMessageUpdate>,
    notify: Arc<Notify>,
    sibling_abort: Arc<Notify>,
) {
    // Snapshot state
    let snapshot: Vec<(String, String, serde_json::Value, bool, bool, bool)> = {
        let guard = state.lock().await;
        guard.tools.iter()
            .map(|t| {
                let is_queued = t.status == ToolStatus::Queued;
                let is_executing = t.status == ToolStatus::Executing;
                (t.id.clone(), t.name.clone(), t.args.clone(), t.is_concurrency_safe, is_queued, is_executing)
            })
            .collect()
    };

    // Find tools that can run
    let mut can_run: Vec<(String, String, serde_json::Value, bool)> = Vec::new();
    for (id, name, args, is_safe, is_queued, is_executing) in &snapshot {
        if !is_queued {
            continue;
        }
        let blocked = snapshot.iter().any(|(_, _, _, other_safe, _, other_exec)| {
            *other_exec && !*other_safe
        });
        if blocked && !*is_safe {
            // Non-safe blocked by another executing — skip (will be picked by the executing one)
            continue;
        }
        can_run.push((id.clone(), name.clone(), args.clone(), *is_safe));
    }

    for (id, name, args, is_safe) in can_run {
        execute_tool(state.clone(), id.clone(), name.clone(), args, is_safe, executor.clone(), sibling_abort.clone(), result_tx.clone(), notify.clone()).await;
        if !is_safe {
            break;
        }
    }

    notify.notify_one();
}

/// Execute a single tool in the background.
async fn execute_tool(
    state: Arc<Mutex<SharedState>>,
    id: String,
    name: String,
    args: serde_json::Value,
    _is_concurrency_safe: bool,
    executor: ToolExecutorFn,
    sibling_abort: Arc<Notify>,
    result_tx: mpsc::UnboundedSender<ToolMessageUpdate>,
    notify: Arc<Notify>,
) {
    // Pre-flight checks
    let guard = state.lock().await;
    if guard.discarded {
        drop(guard);
        result_tx.send(create_synthetic_error(&id, "streaming_fallback", &name)).ok();
        return;
    }
    if guard.has_errored {
        drop(guard);
        result_tx.send(create_synthetic_error(&id, "sibling_error", &name)).ok();
        return;
    }
    drop(guard);

    // Mark as executing
    {
        let mut guard = state.lock().await;
        if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == id) {
            tool.status = ToolStatus::Executing;
        }
    }

    // Wait for sibling abort before starting
    {
        let sab = sibling_abort.clone();
        sab.notified().await;
    }

    // Execute the tool
    let result = executor(name.clone(), args.clone(), id.clone()).await;

    // Mark complete and send result
    {
        let mut guard = state.lock().await;
        if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == id) {
            tool.status = ToolStatus::Completed;
        }
        // Check for Bash error cascade
        if let Ok(tool_result) = &result {
            if tool_result.is_error == Some(true) && name == "Bash" {
                guard.has_errored = true;
                let siblings: Vec<(String, String)> = guard.tools.iter()
                    .filter(|t| t.status == ToolStatus::Executing)
                    .map(|t| (t.id.clone(), t.name.clone()))
                    .collect();
                drop(guard);
                sibling_abort.notify_waiters();
                for (sid, sname) in siblings {
                    result_tx.send(create_synthetic_error(&sid, "sibling_error", &sname)).ok();
                }
                notify.notify_one();
                return;
            }
        }
        drop(guard);
    }

    // Send result
    let message = match result {
        Ok(tool_result) => ToolMessageUpdate {
            message: Some(Message {
                role: MessageRole::Tool,
                content: tool_result.content,
                tool_call_id: Some(id.clone()),
                is_error: tool_result.is_error,
                ..Default::default()
            }),
            new_context: None,
            context_modifier: None,
        },
        Err(e) => ToolMessageUpdate {
            message: Some(Message {
                role: MessageRole::Tool,
                content: format!("<tool_use_error>Error: {}</tool_use_error>", e),
                tool_call_id: Some(id.clone()),
                is_error: Some(true),
                ..Default::default()
            }),
            new_context: None,
            context_modifier: None,
        },
    };
    result_tx.send(message.clone()).ok();
    // Also store in state for get_completed_results
    {
        let mut guard = state.lock().await;
        if let Some(tool) = guard.tools.iter_mut().find(|t| t.id == id) {
            tool.results.push(message);
        }
    }
    notify.notify_one();
}

/// Create a synthetic error message for cancelled/aborted tools.
fn create_synthetic_error(reason: &str, tool_call_id: &str, tool_name: &str) -> ToolMessageUpdate {
    let message = match reason {
        "streaming_fallback" => Message {
            role: MessageRole::User,
            content: format!("Streaming fallback - tool '{}' execution discarded", tool_name),
            ..Default::default()
        },
        "sibling_error" => Message {
            role: MessageRole::User,
            content: format!("Cancelled: parallel tool call '{}' errored", tool_name),
            ..Default::default()
        },
        "user_interrupted" => Message {
            role: MessageRole::User,
            content: "User rejected tool use".to_string(),
            ..Default::default()
        },
        _ => Message {
            role: MessageRole::User,
            content: format!("Tool '{}' error", tool_name),
            ..Default::default()
        },
    };

    ToolMessageUpdate {
        message: Some(message),
        new_context: None,
        context_modifier: None,
    }
}

/// Get tool concurrency info for the streaming executor.
pub fn get_tool_concurrency_info(
    tool_calls: &[ToolCall],
    tools: &[ToolDefinition],
) -> Vec<(String, String, bool, serde_json::Value)> {
    tool_calls
        .iter()
        .map(|tc| {
            let is_safe = tools
                .iter()
                .find(|t| t.name == tc.name)
                .map(|t| t.is_concurrency_safe(&tc.arguments))
                .unwrap_or(false);
            (tc.id.clone(), tc.name.clone(), is_safe, tc.arguments.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_create_executor() {
        let executor: ToolExecutorFn = Arc::new(|_name, _args, _id| {
            Box::pin(async {
                Ok(ToolResult {
                    result_type: "tool_result".to_string(),
                    tool_use_id: "1".to_string(),
                    content: "ok".to_string(),
                    is_error: Some(false),
                was_persisted: None,
                })
            })
        });
        let exe = StreamingToolExecutor::new(executor, vec![]);
        exe.0.add_tool("Bash".to_string(), "tool1".to_string(), serde_json::json!({}));
        // Give spawned task a moment to complete
        sleep(Duration::from_millis(50)).await;
        assert_eq!(exe.0.state.lock().await.tools.len(), 1);
    }

    #[tokio::test]
    async fn test_mark_complete() {
        let executor: ToolExecutorFn = Arc::new(|_name, _args, _id| {
            Box::pin(async { Ok(ToolResult { result_type: "t".into(), tool_use_id: "1".into(), content: "ok".into(), is_error: Some(false), was_persisted: None }) })
        });
        let exe = StreamingToolExecutor::new(executor, vec![]);
        exe.0.add_tool("Bash".to_string(), "tool1".to_string(), serde_json::json!({}));
        exe.0.mark_complete("tool1").await;
        sleep(Duration::from_millis(50)).await;
        let guard = exe.0.state.lock().await;
        assert_eq!(guard.tools[0].status, ToolStatus::Completed);
    }

    #[tokio::test]
    async fn test_discard() {
        let executor: ToolExecutorFn = Arc::new(|_name, _args, _id| {
            Box::pin(async { Ok(ToolResult { result_type: "t".into(), tool_use_id: "1".into(), content: "ok".into(), is_error: Some(false), was_persisted: None }) })
        });
        let (exe, mut rx) = StreamingToolExecutor::new(executor, vec![]);
        // Add 2 tools
        exe.add_tool("Bash".to_string(), "tool1".to_string(), serde_json::json!({}));
        exe.add_tool("Glob".to_string(), "tool2".to_string(), serde_json::json!({}));
        // Small delay for spawned tasks
        sleep(Duration::from_millis(50)).await;

        exe.discard().await;

        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(count >= 1);
    }

    #[tokio::test]
    async fn test_trigger_sibling_abort() {
        let executor: ToolExecutorFn = Arc::new(|_name, _args, _id| {
            Box::pin(async { Ok(ToolResult { result_type: "t".into(), tool_use_id: "1".into(), content: "ok".into(), is_error: Some(false), was_persisted: None }) })
        });
        let (exe, mut rx) = StreamingToolExecutor::new(executor, vec![]);
        exe.add_tool("Bash".to_string(), "tool1".to_string(), serde_json::json!({}));
        exe.add_tool("Glob".to_string(), "tool2".to_string(), serde_json::json!({}));
        sleep(Duration::from_millis(50)).await;

        // Manually set executing status
        {
            let mut guard = exe.state.lock().await;
            if let Some(t) = guard.tools.iter_mut().find(|t| t.id == "tool1") {
                t.status = ToolStatus::Executing;
            }
            if let Some(t) = guard.tools.iter_mut().find(|t| t.id == "tool2") {
                t.status = ToolStatus::Executing;
            }
        }

        exe.trigger_sibling_abort().await;

        let guard = exe.state.lock().await;
        assert!(guard.has_errored);

        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert!(count >= 1);
    }

    #[tokio::test]
    async fn test_set_tool_result() {
        let executor: ToolExecutorFn = Arc::new(|_name, _args, _id| {
            Box::pin(async {
                Ok(ToolResult {
                    result_type: "tool_result".to_string(),
                    tool_use_id: "1".to_string(),
                    content: "command output".to_string(),
                    is_error: Some(false),
                was_persisted: None,
                })
            })
        });
        let (exe, mut rx) = StreamingToolExecutor::new(executor, vec![]);
        exe.add_tool("Bash".to_string(), "tool1".to_string(), serde_json::json!({}));

        exe.set_tool_result("tool1".to_string(), Ok(ToolResult {
            result_type: "tool_result".to_string(),
            tool_use_id: "tool1".to_string(),
            content: "command output".to_string(),
            is_error: Some(false),
                was_persisted: None,
        })).await;

        let update = rx.recv().await;
        assert!(update.is_some());
        let msg = update.unwrap().message.unwrap();
        assert_eq!(msg.content, "command output");
    }

    #[test]
    fn test_get_tool_concurrency_info() {
        let tools = vec![
            ToolDefinition {
                name: "Bash".to_string(),
                description: "Execute commands".to_string(),
                input_schema: ToolInputSchema { schema_type: "object".to_string(), properties: serde_json::json!({}), required: None },
                annotations: Some(ToolAnnotations { concurrency_safe: Some(true), ..Default::default() }),
                should_defer: None, always_load: None, is_mcp: None, search_hint: None,
            aliases: None,
            },
        ];
        let calls = vec![ToolCall {
            id: "1".to_string(),
            r#type: "function".to_string(),
            name: "Bash".to_string(),
            arguments: serde_json::json!({}),
        }];
        let info = get_tool_concurrency_info(&calls, &tools);
        assert_eq!(info.len(), 1);
        assert!(info[0].2);
    }
}
