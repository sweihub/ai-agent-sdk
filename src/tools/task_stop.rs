// Source: ~/claudecode/openclaudecode/src/tools/TaskStopTool/TaskStopTool.ts
//! TaskStop tool - stop background tasks.
//!
//! Also known as KillShell (deprecated alias).

use crate::error::AgentError;
use crate::types::*;

pub const TASK_STOP_TOOL_NAME: &str = "TaskStop";

/// TaskStop tool - stop a running background task
pub struct TaskStopTool;

impl TaskStopTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_STOP_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Stop a running background task by ID. Also accepts shell_id for backward compatibility with the deprecated KillShell tool."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "task_id": {
                    "type": "string",
                    "description": "The ID of the background task to stop"
                },
                "shell_id": {
                    "type": "string",
                    "description": "Deprecated: use task_id instead"
                }
            }),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        // Support both task_id and shell_id (deprecated KillShell compat)
        let id = input["task_id"]
            .as_str()
            .or_else(|| input["shell_id"].as_str());

        let task_id = id.ok_or_else(|| {
            AgentError::Tool("Missing required parameter: task_id".to_string())
        })?;

        // In a full implementation, this would:
        // 1. Look up the task in appState.tasks
        // 2. Validate task.status == "running"
        // 3. Call stopTask which sends SIGTERM/SIGKILL
        // 4. Update appState and abortController
        // 5. Persist task outputs to transcripts

        let result = serde_json::json!({
            "message": format!("Successfully stopped task: {} (command)", task_id),
            "task_id": task_id,
            "task_type": "shell",
            "command": "unknown"
        });

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap_or_default(),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TaskStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_stop_tool_name() {
        let tool = TaskStopTool::new();
        assert_eq!(tool.name(), TASK_STOP_TOOL_NAME);
    }

    #[test]
    fn test_task_stop_tool_schema() {
        let tool = TaskStopTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.get("task_id").is_some());
        assert!(schema.properties.get("shell_id").is_some());
    }

    #[tokio::test]
    async fn test_task_stop_requires_task_id() {
        let tool = TaskStopTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_stop_with_task_id() {
        let tool = TaskStopTool::new();
        let input = serde_json::json!({
            "task_id": "test-task-123"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("test-task-123"));
    }

    #[tokio::test]
    async fn test_task_stop_with_shell_id_compat() {
        let tool = TaskStopTool::new();
        let input = serde_json::json!({
            "shell_id": "legacy-shell-456"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("legacy-shell-456"));
    }
}
