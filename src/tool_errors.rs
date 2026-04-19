//! Tool error formatting utilities.
//!
//! Re-exports and enhances utilities from utils/tool_errors.rs.

pub use crate::utils::tool_errors::*;

use crate::AgentError;

/// Format an AgentError for display in tool result messages.
/// Matches TypeScript's formatError from toolErrors.ts.
pub fn format_tool_error(error: &AgentError) -> String {
    match error {
        AgentError::UserAborted => "User rejected tool use".to_string(),
        AgentError::ApiConnectionTimeout(msg) => msg.clone(),
        AgentError::StreamEndedWithoutEvents => "Stream ended without events".to_string(),
        AgentError::Stream404CreationError(msg) => msg.clone(),
        AgentError::Tool(msg) if msg.contains("exit") || msg.contains("Exit") => {
            // Extract exit code, stderr, stdout pattern
            // Format: "Exit code N: stderr\nstdout"
            let parts: Vec<&str> = msg.splitn(2, ": ").collect();
            if parts.len() == 2 {
                let exit_info = parts[0];
                let detail = parts[1];
                let stderr_start = detail.find("stderr:");
                let stdout_start = detail.find("stdout:");

                let mut result = vec![exit_info.to_string()];
                if let Some(pos) = stderr_start {
                    let after_stderr = &detail[pos..];
                    let stderr_end = after_stderr.find("stdout:").unwrap_or(after_stderr.len());
                    let stderr_content = after_stderr[..stderr_end].trim_start_matches("stderr:").trim();
                    if !stderr_content.is_empty() {
                        result.push(stderr_content.to_string());
                    }
                }
                if let Some(pos) = stdout_start {
                    let stdout_content = detail[pos..].trim_start_matches("stdout:").trim();
                    if !stdout_content.is_empty() {
                        result.push(stdout_content.to_string());
                    }
                }
                return result.join("\n");
            }
            msg.clone()
        }
        AgentError::Tool(msg) => msg.clone(),
        AgentError::Internal(msg) if msg.contains("Interrupt") => msg.clone(),
        _ => error.to_string(),
    }
    .chars()
    .take(10_000)
    .collect()
}

/// Format input validation error for tool call.
pub fn format_input_validation_error(tool_name: &str, details: &str) -> String {
    let issue_word = if details.contains('\n') { "issues" } else { "issue" };
    format!(
        "{} failed due to the following {}:\n{}",
        tool_name,
        issue_word,
        details
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tool_error_user_aborted() {
        let err = AgentError::UserAborted;
        let formatted = format_tool_error(&err);
        assert_eq!(formatted, "User rejected tool use");
    }

    #[test]
    fn test_format_tool_error_tool() {
        let err = AgentError::Tool("ls: command not found".to_string());
        let formatted = format_tool_error(&err);
        assert_eq!(formatted, "ls: command not found");
    }
}
