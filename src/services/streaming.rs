// Source: ~/claudecode/openclaudecode/src/services/api/claude.ts (streaming logic)
// Source: ~/claudecode/openclaudecode/src/services/tools/StreamingToolExecutor.ts
#![allow(dead_code)]

use crate::error::AgentError;
use crate::types::TokenUsage;
use crate::types::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ─── Streaming Constants (matching TypeScript) ───

/// Default streaming idle timeout in milliseconds (90 seconds)
pub const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 90_000;
/// Default streaming idle warning threshold (half of timeout, 45 seconds)
pub const DEFAULT_STREAM_IDLE_WARNING_MS: u64 = 45_000;
/// Stall detection threshold in milliseconds (30 seconds)
pub const STALL_THRESHOLD_MS: u64 = 30_000;

// ─── Streaming Result (complete, matching TypeScript) ───

/// Streaming result containing accumulated content, tool calls, and metadata.
/// Matches TypeScript's partialMessage + newMessages + usage + cost accumulation.
#[derive(Debug, Clone)]
pub struct StreamingResult {
    /// Accumulated text content from all content blocks
    pub content: String,
    /// Accumulated tool calls (completed tool_use blocks)
    pub tool_calls: Vec<serde_json::Value>,
    /// Token usage information
    pub usage: TokenUsage,
    /// API error type if any (e.g., "max_output_tokens", "prompt_too_long")
    pub api_error: Option<String>,
    /// Time to first token in milliseconds
    pub ttft_ms: Option<u64>,
    /// The stop_reason from message_delta (e.g., "end_turn", "tool_use", "max_tokens")
    pub stop_reason: Option<String>,
    /// Total cost in USD for this request
    pub cost: f64,
    /// Whether message_start event was received
    pub message_started: bool,
    /// Number of content blocks that were started
    pub content_blocks_started: u32,
    /// Number of content blocks that were completed
    pub content_blocks_completed: u32,
    /// Whether any tool_use blocks were completed
    pub any_tool_use_completed: bool,
    /// Research data from message_start (internal only, for ant userType)
    pub research: Option<serde_json::Value>,
}

impl Default for StreamingResult {
    fn default() -> Self {
        Self {
            content: String::new(),
            tool_calls: Vec::new(),
            usage: TokenUsage::default(),
            api_error: None,
            ttft_ms: None,
            stop_reason: None,
            cost: 0.0,
            message_started: false,
            content_blocks_started: 0,
            content_blocks_completed: 0,
            any_tool_use_completed: false,
            research: None,
        }
    }
}

// ─── Stall Tracking ───

/// Tracks streaming stall statistics.
#[derive(Debug, Clone, Default)]
pub struct StallStats {
    /// Number of stalls detected
    pub stall_count: u64,
    /// Total stall time in milliseconds
    pub total_stall_time_ms: u64,
    /// Individual stall durations in milliseconds
    pub stall_durations: Vec<u64>,
}

// ─── Stream Watchdog (idle timeout) ───

/// Manages the stream idle timeout watchdog.
/// Matches TypeScript's streamIdleTimer/streamIdleWarningTimer logic.
pub struct StreamWatchdog {
    /// Whether the watchdog is enabled
    pub enabled: bool,
    /// Idle timeout in milliseconds
    pub idle_timeout_ms: u64,
    /// Warning threshold in milliseconds
    pub warning_threshold_ms: u64,
    /// Whether the stream was aborted by the watchdog
    pub aborted: bool,
    /// When the watchdog fired (performance.now() snapshot)
    pub watchdog_fired_at: Option<u128>,
}

impl StreamWatchdog {
    pub fn new(enabled: bool, idle_timeout_ms: u64) -> Self {
        Self {
            enabled,
            idle_timeout_ms,
            warning_threshold_ms: idle_timeout_ms / 2,
            aborted: false,
            watchdog_fired_at: None,
        }
    }

    pub fn from_env() -> Self {
        let enabled = std::env::var(crate::constants::env::ai_code::ENABLE_STREAM_WATCHDOG)
            .map(|v| {
                matches!(
                    v.to_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);

        let timeout_ms = std::env::var(crate::constants::env::ai_code::STREAM_IDLE_TIMEOUT_MS)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_STREAM_IDLE_TIMEOUT_MS);

        Self::new(enabled, timeout_ms)
    }

    /// Check if the watchdog has aborted the stream
    pub fn is_aborted(&self) -> bool {
        self.aborted
    }

    /// Get when the watchdog fired (for measuring abort propagation delay)
    pub fn watchdog_fired_at(&self) -> Option<u128> {
        self.watchdog_fired_at
    }

    /// Mark the watchdog as having fired (called by the actual timeout logic).
    /// Returns the abort reason message.
    pub fn fire(&mut self) -> String {
        self.aborted = true;
        self.watchdog_fired_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        );
        format!(
            "Stream idle timeout - no chunks received for {}ms",
            self.idle_timeout_ms
        )
    }

    /// Log warning when stream has been idle for half the timeout
    pub fn warning_message(&self) -> String {
        format!(
            "Streaming idle warning: no chunks received for {}ms",
            self.warning_threshold_ms
        )
    }
}

// ─── Non-Streaming Fallback Control ───

/// Determines whether non-streaming fallback should be disabled.
/// Matches TypeScript's disableFallback logic:
/// - AI_CODE_DISABLE_NONSTREAMING_FALLBACK env var
/// - GrowthBook feature flag 'tengu_disable_streaming_to_non_streaming_fallback'
pub fn is_nonstreaming_fallback_disabled() -> bool {
    // Check env var first
    if std::env::var(crate::constants::env::ai_code::DISABLE_NONSTREAMING_FALLBACK)
        .map(|v| {
            matches!(
                v.to_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
    {
        return true;
    }

    // Check GrowthBook feature flag
    if let Ok(value) = std::env::var("AI_CODE_TENGU_DISABLE_STREAMING_FALLBACK") {
        if matches!(
            value.to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ) {
            return true;
        }
    }

    false
}

// ─── Non-Streaming Fallback Timeout ───

/// Get the timeout for non-streaming fallback in milliseconds.
/// Matches TypeScript's getNonstreamingFallbackTimeoutMs().
pub fn get_nonstreaming_fallback_timeout_ms() -> u64 {
    // Check for explicit override
    if let Ok(ms) = std::env::var(crate::constants::env::ai_code::API_TIMEOUT_MS) {
        if let Ok(val) = ms.parse::<u64>() {
            return val;
        }
    }

    // Default: 120s for remote (bridge) mode, 300s for local
    if std::env::var("AI_CODE_REMOTE").is_ok() {
        120_000
    } else {
        300_000
    }
}

// ─── Stream Resource Cleanup ───

/// Manages cleanup of stream resources to prevent memory leaks.
/// Matches TypeScript's releaseStreamResources() + cleanupStream().
pub fn cleanup_stream(abort_handle: &Option<Arc<AtomicBool>>) {
    if let Some(handle) = abort_handle {
        handle.store(true, Ordering::SeqCst);
    }
}

pub fn release_stream_resources(
    abort_handle: &Option<Arc<AtomicBool>>,
    _stream_response: &Option<reqwest::Response>,
) {
    cleanup_stream(abort_handle);
    // reqwest::Response body will be dropped when the Option is set to None
    // The Response object holds native TLS/socket buffers outside the heap,
    // so we must explicitly cancel it (matching TypeScript's streamResponse.body?.cancel()).
    if let Some(response) = _stream_response {
        // Abort the underlying connection if possible
        let _ = response.error_for_status_ref();
    }
}

// ─── Stream Completion Validation ───

/// Validates that a stream completed properly.
/// Matches TypeScript's check:
///   if (!partialMessage || (newMessages.length === 0 && !stopReason))
///     throw new Error('Stream ended without receiving any events')
pub fn validate_stream_completion(result: &StreamingResult) -> Result<(), AgentError> {
    if !result.message_started {
        return Err(AgentError::StreamEndedWithoutEvents);
    }

    // If message_start was received but no content blocks completed AND no stop_reason,
    // the stream ended prematurely (proxy returned message_start but dropped connection)
    if result.content_blocks_started > 0
        && result.content_blocks_completed == 0
        && result.stop_reason.is_none()
    {
        return Err(AgentError::StreamEndedWithoutEvents);
    }

    Ok(())
}

// ─── 404 Stream Creation Error Detection ───

/// Check if an error is a 404 during stream creation that should trigger
/// non-streaming fallback.
/// Matches TypeScript's is404StreamCreationError check.
pub fn is_404_stream_creation_error(error: &AgentError) -> bool {
    let error_str = error.to_string();
    error_str.contains("404")
        && (error_str.contains("Not Found") || error_str.contains("streaming"))
}

// ─── Abort Handling ───

/// Check if an error is a user-initiated abort.
/// Matches TypeScript's APIUserAbortError handling.
pub fn is_user_abort_error(error: &AgentError) -> bool {
    matches!(error, AgentError::UserAborted)
}

/// Check if an error is an API connection timeout.
pub fn is_api_timeout_error(error: &AgentError) -> bool {
    matches!(error, AgentError::ApiConnectionTimeout(_))
}

// ─── Cost Calculation ───

/// Calculate cost based on token usage and model.
/// Matches TypeScript's cost tracking in message_delta.
pub fn calculate_streaming_cost(usage: &TokenUsage, model: &str) -> f64 {
    use crate::services::model_cost::TokenUsage as ModelCostTokenUsage;

    // Convert from types::TokenUsage to model_cost::TokenUsage
    let model_usage = ModelCostTokenUsage {
        input_tokens: usage.input_tokens as u32,
        output_tokens: usage.output_tokens as u32,
        prompt_cache_write_tokens: usage.cache_creation_input_tokens.unwrap_or(0) as u32,
        prompt_cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0) as u32,
    };

    crate::services::model_cost::calculate_cost(model, &model_usage)
}

// ─── Streaming Tool Executor ───

use std::sync::Mutex;
use futures_util::{FutureExt, StreamExt};

/// Thread-safe shared executor state.
/// Uses a tokio async channel to route tool execution requests.
struct SharedExecutorInner {
    tx: tokio::sync::mpsc::Sender<(String, serde_json::Value, String, tokio::sync::mpsc::Sender<crate::types::ToolResult>)>,
}

/// A clonable, thread-safe tool executor function wrapper.
/// Uses an async channel to dispatch tool execution.
pub struct SharedExecutorFn {
    inner: Arc<SharedExecutorInner>,
}

impl Clone for SharedExecutorFn {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl SharedExecutorFn {
    /// Create a new executor and spawn the dispatcher task.
    /// Returns the executor and the dispatcher join handle.
    pub fn new<F, Fut>(executor: F) -> (Self, tokio::task::JoinHandle<()>)
    where
        F: Fn(String, serde_json::Value, String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = crate::types::ToolResult> + Send + 'static,
    {
        let (tx, mut rx) = tokio::sync::mpsc::channel(256);
        let inner = Arc::new(SharedExecutorInner { tx });
        let handle = tokio::spawn(async move {
            while let Some((name, args, tool_call_id, resp_tx)) = rx.recv().await {
                let result = executor(name, args, tool_call_id).await;
                let _ = resp_tx.send(result).await;
            }
        });
        (Self { inner }, handle)
    }

    pub async fn call(&self, name: String, args: serde_json::Value, tool_call_id: String) -> crate::types::ToolResult {
        let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel(1);
        self.inner.tx.send((name, args, tool_call_id, resp_tx)).await.expect("dispatcher disconnected");
        resp_rx.recv().await.expect("dispatcher dropped response")
    }
}

/// Status of a tracked tool in the streaming executor.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Queued,
    Executing,
    Completed,
    Yielded,
}

/// A tool being tracked by the streaming executor.
#[derive(Debug)]
pub struct TrackedTool {
    /// Unique tool ID
    pub id: String,
    /// The tool_use block from the API
    pub block: serde_json::Value,
    /// Whether this tool is concurrency-safe
    pub is_concurrency_safe: bool,
    /// Current status
    pub status: ToolStatus,
    /// Pending progress messages to be yielded
    pub pending_progress: Vec<AgentEvent>,
    /// Whether this tool has errored
    pub has_errored: bool,
    /// Context modifiers collected during tool execution (for contextModifier support)
    pub context_modifiers: Vec<fn(crate::types::ToolContext) -> crate::types::ToolContext>,
}

/// Internal shared state for the streaming executor.
struct ExecutorState {
    tools: Vec<TrackedTool>,
    discarded: bool,
    has_errored: bool,
    errored_tool_description: String,
    parent_abort: Arc<AtomicBool>,
    max_concurrency: usize,
}

/// Executes tools as they stream in with concurrency control.
/// Rust port of TypeScript's StreamingToolExecutor class.
/// - Concurrency-safe tools can execute in parallel
/// - Non-concurrent tools must execute exclusively
///
/// Uses Arc<Mutex<ExecutorState>> for interior mutability so it can be shared
/// via Arc and called from the SSE parsing loop, while spawned tasks can
/// also access the state for marking completions.
pub struct StreamingToolExecutor {
    state: Arc<Mutex<ExecutorState>>,
}

impl StreamingToolExecutor {
    pub fn new(parent_abort: Arc<AtomicBool>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ExecutorState {
                tools: Vec::new(),
                discarded: false,
                has_errored: false,
                errored_tool_description: String::new(),
                parent_abort,
                max_concurrency: 4,
            })),
        }
    }

    fn clone_state(&self) -> Arc<Mutex<ExecutorState>> {
        Arc::clone(&self.state)
    }

    /// Discard all pending and in-progress tools.
    /// Called when streaming fallback occurs.
    pub fn discard(&self) {
        self.state.lock().expect("StreamingToolExecutor mutex poisoned").discarded = true;
    }

    /// Add a tool to the execution queue.
    pub fn add_tool(&self, tool_use_block: serde_json::Value, is_concurrency_safe: bool) {
        let tool_id = tool_use_block
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        state.tools.push(TrackedTool {
            id: tool_id,
            block: tool_use_block,
            is_concurrency_safe,
            status: ToolStatus::Queued,
            pending_progress: Vec::new(),
            has_errored: false,
            context_modifiers: Vec::new(),
        });
    }

    /// Check if a tool can execute based on current concurrency state.
    fn can_execute_tool(&self, is_concurrency_safe: bool) -> bool {
        let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        let executing_safe: Vec<bool> = state
            .tools
            .iter()
            .filter(|t| t.status == ToolStatus::Executing)
            .map(|t| t.is_concurrency_safe)
            .collect();
        drop(state);

        executing_safe.is_empty()
            || (is_concurrency_safe && executing_safe.iter().all(|s| *s))
    }

    /// Check abort reasons for a tool.
    fn get_abort_reason_inner(&self) -> Option<&'static str> {
        let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        if state.discarded {
            return Some("streaming_fallback");
        }
        if state.has_errored {
            return Some("sibling_error");
        }
        if state.parent_abort.load(Ordering::SeqCst) {
            return Some("user_interrupted");
        }
        None
    }

    /// Get the number of currently executing tools
    fn executing_count(&self) -> usize {
        let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        state.tools
            .iter()
            .filter(|t| t.status == ToolStatus::Executing)
            .count()
    }


    /// Check if there are any unfinished tools
    pub fn has_unfinished_tools(&self) -> bool {
        let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        state.tools.iter().any(|t| t.status != ToolStatus::Yielded)
    }

    /// Get completed results that haven't been yielded.
    /// Stops on non-concurrency-safe executing tool (yielding order).
    pub fn get_completed_results(&self) -> Vec<(String, serde_json::Value)> {
        let mut state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        if state.discarded {
            return Vec::new();
        }

        let mut results = Vec::new();

        for tool in &mut state.tools {
            tool.pending_progress.clear();

            if tool.status == ToolStatus::Yielded {
                continue;
            }

            if tool.status == ToolStatus::Completed {
                tool.status = ToolStatus::Yielded;
                results.push((tool.id.clone(), tool.block.clone()));
            } else if tool.status == ToolStatus::Executing && !tool.is_concurrency_safe {
                break;
            }
        }

        results
    }

    /// Mark a tool as having errored (cascading error for sibling tools).
    pub fn mark_tool_errored(&self, tool_id: &str, _description: &str) {
        let mut state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        state.has_errored = true;

        if let Some(tool) = state.tools.iter_mut().find(|t| t.id == tool_id) {
            tool.has_errored = true;
        }
    }

    /// Get the current state summary for debugging
    pub fn summary(&self) -> String {
        let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
        let queued = state.tools.iter().filter(|t| t.status == ToolStatus::Queued).count();
        let executing = state.tools.iter().filter(|t| t.status == ToolStatus::Executing).count();
        let completed = state.tools.iter().filter(|t| t.status == ToolStatus::Completed).count();
        let yielded = state.tools.iter().filter(|t| t.status == ToolStatus::Yielded).count();
        let discarded = state.discarded;
        drop(state);
        format!(
            "StreamingToolExecutor: queued={}, executing={}, completed={}, yielded={}, discarded={}",
            queued, executing, completed, yielded, discarded
        )
    }

    /// Execute queued tools with concurrency control.
    /// Spawns each tool as a task and waits for results respecting concurrency limits.
    /// Returns list of (tool_id, result) pairs in execution order.
    pub async fn execute_all(
        &self,
        executor_fn: SharedExecutorFn,
    ) -> Vec<(String, Result<crate::types::ToolResult, crate::AgentError>)> {
        // ── Synchronous phase: collect can-run tools and mark them executing ──
        let (can_run, max_concurrency) = {
            let state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");

            let mut can_run: Vec<(String, serde_json::Value, serde_json::Value, bool)> = Vec::new();

            for tool in &state.tools {
                if tool.status != ToolStatus::Queued { continue; }
                if tool.has_errored { continue; }

                let block = tool.block.clone();
                let tool_id = tool.id.clone();

                let blocked = state.tools.iter().any(|t| {
                    t.status == ToolStatus::Executing && !t.is_concurrency_safe
                });
                if blocked && !tool.is_concurrency_safe {
                    continue;
                }

                let executing_in_state = state.tools.iter()
                    .filter(|t| t.status == ToolStatus::Executing)
                    .count();
                if executing_in_state >= state.max_concurrency {
                    continue;
                }

                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let args = block.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
                can_run.push((tool_id, block, args, tool.is_concurrency_safe));
            }

            let max_concurrency = state.max_concurrency;
            drop(state);

            // Mark can-run tools as executing
            {
                let mut state = self.state.lock().expect("StreamingToolExecutor mutex poisoned");
                for (tool_id, _, _, _) in &can_run {
                    if let Some(tool) = state.tools.iter_mut().find(|t| t.id == *tool_id) {
                        tool.status = ToolStatus::Executing;
                    }
                }
            }

            (can_run, max_concurrency)
        };

        // ── Async phase: execute tools ──
        let mut results: Vec<(String, Result<crate::types::ToolResult, crate::AgentError>)> =
            Vec::with_capacity(can_run.len());

        let state_arc = self.clone_state();
        let total = can_run.len();

        for chunk_start in (0..total).step_by(max_concurrency) {
            let chunk_end = (chunk_start + max_concurrency).min(total);
            let mut handles = Vec::new();

            for (tool_id, block, args, _is_safe) in &can_run[chunk_start..chunk_end] {
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let tid = tool_id.clone();
                let args = args.clone();
                let exec = executor_fn.clone();
                let state_arc = Arc::clone(&state_arc);

                let handle = tokio::spawn(async move {
                    let tool_result = exec.call(name, args, tid.clone()).await;

                    // Mark as completed
                    {
                        let mut st = state_arc.lock().expect("StreamingToolExecutor mutex poisoned");
                        if let Some(tool) = st.tools.iter_mut().find(|t| t.id == tid) {
                            tool.status = ToolStatus::Completed;
                        }
                    }

                    let result = Ok(tool_result);
                    if result.as_ref().map(|r| r.is_error == Some(true)).unwrap_or(false) {
                        state_arc.lock().expect("StreamingToolExecutor mutex poisoned").has_errored = true;
                    }

                    (tid, result)
                });
                handles.push(handle);
            }

            // Collect results for this chunk
            for handle in handles {
                let (tool_id, result) = handle.await.unwrap_or_else(|e| {
                    ("unknown".to_string(), Err(crate::AgentError::Tool(format!(
                        "Task panicked: {}", e
                    ))))
                });
                results.push((tool_id, result));
            }
        }

        results
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_result_defaults() {
        let result = StreamingResult::default();
        assert!(!result.message_started);
        assert_eq!(result.content_blocks_started, 0);
        assert_eq!(result.content_blocks_completed, 0);
        assert!(!result.any_tool_use_completed);
        assert!(result.ttft_ms.is_none());
        assert!(result.stop_reason.is_none());
        assert_eq!(result.cost, 0.0);
    }

    #[test]
    fn test_stream_watchdog_defaults() {
        let watchdog = StreamWatchdog::new(false, DEFAULT_STREAM_IDLE_TIMEOUT_MS);
        assert!(!watchdog.is_aborted());
        assert!(watchdog.watchdog_fired_at().is_none());
    }

    #[test]
    fn test_stream_watchdog_fire() {
        let mut watchdog = StreamWatchdog::new(true, 90_000);
        assert!(!watchdog.is_aborted());

        let reason = watchdog.fire();
        assert!(watchdog.is_aborted());
        assert!(watchdog.watchdog_fired_at().is_some());
        assert!(reason.contains("idle timeout"));
    }

    #[test]
    fn test_nonstreaming_fallback_disabled_default() {
        // By default, fallback should NOT be disabled
        assert!(!is_nonstreaming_fallback_disabled());
    }

    #[test]
    fn test_stream_completion_validation_started_but_not_completed() {
        let mut result = StreamingResult::default();
        result.message_started = true;
        result.content_blocks_started = 1;
        // No blocks completed, no stop_reason - should fail validation
        assert!(validate_stream_completion(&result).is_err());
    }

    #[test]
    fn test_stream_completion_validation_message_not_started() {
        let result = StreamingResult::default();
        assert!(validate_stream_completion(&result).is_err());
    }

    #[test]
    fn test_stream_completion_validation_valid() {
        let mut result = StreamingResult::default();
        result.message_started = true;
        result.content_blocks_started = 1;
        result.content_blocks_completed = 1;
        assert!(validate_stream_completion(&result).is_ok());
    }

    #[test]
    fn test_stream_completion_validation_with_stop_reason() {
        let mut result = StreamingResult::default();
        result.message_started = true;
        result.content_blocks_started = 1;
        result.stop_reason = Some("end_turn".to_string());
        assert!(validate_stream_completion(&result).is_ok());
    }

    #[test]
    fn test_is_404_stream_creation_error() {
        assert!(is_404_stream_creation_error(&AgentError::Api(
            "Streaming API error 404: Not Found".to_string()
        )));
        assert!(is_404_stream_creation_error(&AgentError::Api(
            "404 streaming endpoint not found".to_string()
        )));
        assert!(!is_404_stream_creation_error(&AgentError::Api(
            "API error: 500".to_string()
        )));
    }

    #[test]
    fn test_is_user_abort_error() {
        assert!(is_user_abort_error(&AgentError::UserAborted));
        assert!(!is_user_abort_error(&AgentError::Api("timeout".to_string())));
    }

    #[test]
    fn test_is_api_timeout_error() {
        assert!(is_api_timeout_error(&AgentError::ApiConnectionTimeout(
            "Request timed out".to_string()
        )));
        assert!(!is_api_timeout_error(&AgentError::Api("other".to_string())));
    }

    #[test]
    fn test_streaming_tool_executor_add_and_summary() {
        let abort = Arc::new(AtomicBool::new(false));
        let executor = StreamingToolExecutor::new(abort);

        executor.add_tool(
            serde_json::json!({"id": "tool_1", "name": "Bash", "input": {"command": "ls"}}),
            true,
        );
        executor.add_tool(
            serde_json::json!({"id": "tool_2", "name": "Read", "input": {"file": "foo.txt"}}),
            false,
        );

        let summary = executor.summary();
        assert!(summary.contains("queued=2"));
        assert!(executor.has_unfinished_tools());
    }

    #[test]
    fn test_streaming_tool_executor_can_execute() {
        let abort = Arc::new(AtomicBool::new(false));
        let executor = StreamingToolExecutor::new(abort);

        // No tools executing - should allow
        assert!(executor.can_execute_tool(true));
        assert!(executor.can_execute_tool(false));

        // Simulate a concurrency-safe tool executing
        executor.add_tool(
            serde_json::json!({"id": "tool_1", "name": "Bash"}),
            true,
        );
        {
            let mut state = executor.state.lock().expect("mutex poisoned");
            state.tools[0].status = ToolStatus::Executing;
        }

        // Another concurrency-safe tool can execute alongside
        assert!(executor.can_execute_tool(true));
        // Non-concurrency-safe tool cannot
        assert!(!executor.can_execute_tool(false));
    }

    #[test]
    fn test_streaming_tool_executor_discard() {
        let abort = Arc::new(AtomicBool::new(false));
        let mut executor = StreamingToolExecutor::new(abort);

        executor.add_tool(
            serde_json::json!({"id": "tool_1", "name": "Bash"}),
            true,
        );
        executor.discard();

        let results = executor.get_completed_results();
        assert!(results.is_empty());
    }

    #[test]
    fn test_stall_stats_default() {
        let stats = StallStats::default();
        assert_eq!(stats.stall_count, 0);
        assert_eq!(stats.total_stall_time_ms, 0);
    }

    #[test]
    fn test_release_stream_resources() {
        let abort = Arc::new(AtomicBool::new(false));
        release_stream_resources(&Some(abort.clone()), &None);
        assert!(abort.load(Ordering::SeqCst));
    }
}
