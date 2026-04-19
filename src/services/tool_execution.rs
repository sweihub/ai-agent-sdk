// Source: /data/home/swei/claudecode/openclaudecode/src/services/tools/toolExecution.ts
//! Tool execution module - handles running individual tools.
//!
//! Translated from TypeScript toolExecution.ts

use crate::types::Message;
use crate::tools::ToolDefinition;

/// Minimum total hook duration (ms) to show inline timing summary
pub const HOOK_TIMING_DISPLAY_THRESHOLD_MS: u64 = 500;

/// Log a debug warning when hooks/permission-decision block for this long.
/// Matches BashTool's PROGRESS_THRESHOLD_MS.
pub const SLOW_PHASE_LOG_THRESHOLD_MS: u64 = 2000;

/// Classify a tool execution error into a telemetry-safe string.
///
/// In minified/external builds, error.constructor.name is mangled into
/// short identifiers like "nJT" or "Chq" — useless for diagnostics.
/// This function extracts structured, telemetry-safe information instead:
/// - TelemetrySafeError: use its telemetryMessage (already vetted)
/// - Node.js fs errors: log the error code (ENOENT, EACCES, etc.)
/// - Known error types: use their unminified name
/// - Fallback: "Error" (better than a mangled 3-char identifier)
pub fn classify_tool_error(error: &(dyn std::error::Error + 'static)) -> String {
    // Check for specific error types and extract telemetry-safe information

    // Try to get the error name/type
    let error_name = std::any::type_name_of_val(error);

    // For standard errors, check for errno codes
    if let Some(downcast) = error.downcast_ref::<std::io::Error>() {
        let errno = downcast.raw_os_error();
        if let Some(code) = errno {
            return format!("Error:{}", code);
        }
    }

    // Check if error name is meaningful (more than 3 chars)
    let name_len = error_name.len();
    if name_len > 3 && !error_name.contains("std::io::Error") {
        // Return a truncated type name as fallback
        let short_name = error_name
            .rsplit("::")
            .next()
            .unwrap_or(error_name)
            .chars()
            .take(60)
            .collect::<String>();
        return short_name;
    }

    "Error".to_string()
}

/// Classify tool error from a string message (simpler version)
pub fn classify_tool_error_from_message(message: &str) -> String {
    let lower = message.to_lowercase();

    // Check for known error patterns
    if lower.contains("enoent") || lower.contains("file not found") {
        return "Error:ENOENT".to_string();
    }
    if lower.contains("eacces") || lower.contains("permission denied") {
        return "Error:EACCES".to_string();
    }
    if lower.contains("timeout") {
        return "Error:ETIMEDOUT".to_string();
    }

    // Default
    "Error".to_string()
}

/// Build a hint message when a deferred tool's schema was not sent to the API.
/// This helps the model understand why input validation failed.
/// Returns None if the hint should not be shown.
pub fn build_schema_not_sent_hint(
    tool_name: &str,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Option<String> {
    // Check if tool is available in the tools list
    let tool_available = tools.iter().any(|t| t.name == tool_name);
    if tool_available {
        return None;
    }

    // Check if tool was previously discovered in messages
    let discovered_in_messages = messages.iter().any(|m| {
        m.content.contains(tool_name)
    });

    if discovered_in_messages {
        return Some(format!(
            "\n\nThis tool's schema was not sent to the API — it was not in the discovered-tool set derived from message history. \
            Without the schema in your prompt, typed parameters (arrays, numbers, booleans) get emitted as strings and the client-side parser rejects them. \
            Load the tool first: call tool_search with query \"select:{}\", then retry this call.",
            tool_name
        ));
    }

    None
}

/// Message update type for lazy message generation
#[derive(Debug, Clone)]
pub struct MessageUpdateLazy {
    pub message: Message,
    pub context_modifier: Option<ContextModifier>,
}

/// Context modifier for updating tool context
#[derive(Debug, Clone)]
pub struct ContextModifier {
    pub tool_use_id: String,
}

/// Progress information from tool execution
#[derive(Debug, Clone)]
pub struct ToolProgress {
    pub tool_use_id: String,
    pub data: serde_json::Value,
}

/// Error types for tool execution
#[derive(Debug, Clone)]
pub enum ToolExecutionError {
    /// Tool not found
    ToolNotFound(String),
    /// Input validation failed
    InputValidation(String),
    /// Permission denied
    PermissionDenied(String),
    /// Tool execution failed
    ExecutionFailed(String),
    /// Aborted
    Aborted,
}

impl std::fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolExecutionError::ToolNotFound(name) => write!(f, "No such tool available: {}", name),
            ToolExecutionError::InputValidation(msg) => write!(f, "InputValidationError: {}", msg),
            ToolExecutionError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            ToolExecutionError::ExecutionFailed(msg) => write!(f, "Error calling tool: {}", msg),
            ToolExecutionError::Aborted => write!(f, "Tool execution was aborted"),
        }
    }
}

impl std::error::Error for ToolExecutionError {}

/// Create a tool result message for errors
pub fn create_tool_error_message(
    tool_use_id: &str,
    error: &str,
    is_error: bool,
) -> Message {
    Message {
        role: crate::types::MessageRole::Tool,
        content: format!("<tool_use_error>{}</tool_use_error>", error),
        tool_call_id: Some(tool_use_id.to_string()),
        is_error: Some(is_error),
        ..Default::default()
    }
}

/// Create a progress message during tool execution
pub fn create_progress_message(
    tool_use_id: &str,
    data: serde_json::Value,
) -> Message {
    Message {
        role: crate::types::MessageRole::User,
        content: serde_json::json!({
            "type": "progress",
            "tool_use_id": tool_use_id,
            "data": data,
        }).to_string(),
        ..Default::default()
    }
}

/// Format a tool input validation error
pub fn format_input_validation_error(
    tool_name: &str,
    error_message: &str,
) -> String {
    format!("Error parsing {} input: {}", tool_name, error_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tool_error_io() {
        // Create a simple IO error to test
        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let classified = classify_tool_error(&error);
        // Should contain the errno code
        assert!(classified.contains("Error:") || classified == "Error");
    }

    #[test]
    fn test_classify_tool_error_from_message() {
        assert_eq!(classify_tool_error_from_message("File not found"), "Error:ENOENT");
        assert_eq!(classify_tool_error_from_message("Permission denied"), "Error:EACCES");
        assert_eq!(classify_tool_error_from_message("timeout error"), "Error:ETIMEDOUT");
        assert_eq!(classify_tool_error_from_message("Some other error"), "Error");
    }

    #[test]
    fn test_build_schema_not_sent_hint_tool_available() {
        let tools = vec![ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            input_schema: crate::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
            },
            annotations: None,
            should_defer: None,
            always_load: None,
            is_mcp: None,
            search_hint: None,
        aliases: None,
        }];
        let messages = vec![];

        let hint = build_schema_not_sent_hint("test_tool", &messages, &tools);
        // Tool is available, should return None
        assert!(hint.is_none());
    }

    #[test]
    fn test_build_schema_not_sent_hint_discovered() {
        let tools = vec![];
        let messages = vec![Message {
            role: crate::types::MessageRole::Assistant,
            content: "Using discovered_tool".to_string(),
            ..Default::default()
        }];

        let hint = build_schema_not_sent_hint("discovered_tool", &messages, &tools);
        // Tool was mentioned in messages but not in tools list
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("discovered_tool"));
    }

    #[test]
    fn test_create_tool_error_message() {
        let msg = create_tool_error_message("tool_123", "Test error", true);
        assert!(msg.content.contains("tool_use_error"));
        assert!(msg.content.contains("Test error"));
        assert!(msg.is_error == Some(true));
    }

    #[test]
    fn test_format_input_validation_error() {
        let error = format_input_validation_error("Read", "expected string, got number");
        assert!(error.contains("Read"));
        assert!(error.contains("expected string"));
    }

    #[test]
    fn test_constants() {
        assert_eq!(HOOK_TIMING_DISPLAY_THRESHOLD_MS, 500);
        assert_eq!(SLOW_PHASE_LOG_THRESHOLD_MS, 2000);
    }
}