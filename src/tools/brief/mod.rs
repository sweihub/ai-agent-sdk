// Source: ~/claudecode/openclaudecode/src/tools/BriefTool/BriefTool.ts
//! Brief tool (SendUserMessage) — primary visible output channel.
//!
//! Sends messages to the user that they will actually read.
//! Supports attachments and proactive/normal status labels.

use crate::error::AgentError;
use crate::types::*;

pub mod prompt;
use prompt::{BRIEF_TOOL_NAME, DESCRIPTION};

/// BriefTool — send a message to the user
pub struct BriefTool;

impl BriefTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        BRIEF_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "Brief".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["message"].as_str().map(|s| s.chars().take(50).collect()))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        content["content"].as_str().map(|s| s.to_string())
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "message": {
                    "type": "string",
                    "description": "The message for the user. Supports markdown formatting."
                },
                "attachments": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional file paths (absolute or relative to cwd) to attach. Use for photos, screenshots, diffs, logs, or any file the user should see alongside your message."
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Use 'proactive' when you're surfacing something the user hasn't asked for and needs to see now — task completion while they're away, a blocker you hit, an unsolicited status update. Use 'normal' when replying to something the user just said."
                }
            }),
            required: Some(vec!["message".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let message = input["message"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing required parameter: message".to_string()))?
            .to_string();

        // Resolve attachments (best effort)
        let attachments = resolve_attachments(&input, &context.cwd).await;
        let resolved_count = attachments.len();

        let suffix = if resolved_count > 0 {
            format!(" ({resolved_count} attachment(s) included)")
        } else {
            String::new()
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Message delivered to user.{suffix}: {message}"),
            is_error: Some(false),
            was_persisted: Some(true),
        })
    }
}

/// Resolve attachment paths into metadata
async fn resolve_attachments(
    input: &serde_json::Value,
    cwd: &str,
) -> Vec<serde_json::Value> {
    let attachments = match input.get("attachments") {
        Some(a) if a.is_array() => a.as_array().cloned().unwrap_or_default(),
        _ => return vec![],
    };

    let mut resolved = Vec::new();
    for path_value in attachments {
        let path_str = match path_value.as_str() {
            Some(p) => p.to_string(),
            None => continue,
        };

        // Resolve relative paths
        let full_path = if path_str.starts_with('/') {
            path_str.clone()
        } else {
            format!("{cwd}/{path_str}")
        };

        // Check if file exists and get metadata
        let is_image = path_str
            .to_lowercase()
            .ends_with(".png")
            || path_str.to_lowercase().ends_with(".jpg")
            || path_str.to_lowercase().ends_with(".jpeg")
            || path_str.to_lowercase().ends_with(".gif")
            || path_str.to_lowercase().ends_with(".webp")
            || path_str.to_lowercase().ends_with(".bmp");

        let size = tokio::fs::metadata(&full_path)
            .await
            .ok()
            .map(|m| m.len() as i64)
            .unwrap_or(-1);

        if size >= 0 {
            resolved.push(serde_json::json!({
                "path": full_path,
                "size": size,
                "isImage": is_image,
            }));
        }
    }
    resolved
}

impl Default for BriefTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brief_tool_name() {
        let tool = BriefTool::new();
        assert_eq!(tool.name(), BRIEF_TOOL_NAME);
    }

    #[test]
    fn test_brief_tool_schema() {
        let tool = BriefTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("message").is_some());
        assert!(schema.properties.get("attachments").is_some());
        assert!(schema.properties.get("status").is_some());
    }

    #[tokio::test]
    async fn test_brief_execute_requires_message() {
        let tool = BriefTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_brief_execute_normal() {
        let tool = BriefTool::new();
        let input = serde_json::json!({
            "message": "Hello, task complete!",
            "status": "normal"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.content.contains("Hello, task complete!"));
        assert!(r.content.contains("Message delivered to user"));
    }

    #[tokio::test]
    async fn test_brief_execute_proactive() {
        let tool = BriefTool::new();
        let input = serde_json::json!({
            "message": "Blocker detected: disk full",
            "status": "proactive"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_brief_execute_with_attachments() {
        let tool = BriefTool::new();
        let input = serde_json::json!({
            "message": "Here's the output",
            "attachments": []
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.content.contains(" attachment(s)"));
    }
}
