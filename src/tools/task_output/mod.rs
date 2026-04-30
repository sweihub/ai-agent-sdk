// Source: ~/claudecode/openclaudecode/src/tools/TaskOutputTool/TaskOutputTool.tsx
//! TaskOutput tool — retrieve output from background tasks.
//!
//! Supports both blocking (wait for completion) and non-blocking modes.
//! Integrates with the background task registry for real task output retrieval.

use crate::error::AgentError;
use crate::tools::background_task_registry;
use crate::types::*;

pub mod constants;
pub use constants::TASK_OUTPUT_TOOL_NAME;

/// TaskOutput tool — retrieve output from a completed or running background task.
pub struct TaskOutputTool;

impl TaskOutputTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_OUTPUT_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Retrieve output from a running or completed background task (bash command, agent, etc.). Supports blocking wait for completion with configurable timeout."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "TaskOutput".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["task_id"].as_str().map(String::from))
    }

    pub fn render_tool_result_message(
        &self,
        content: &serde_json::Value,
    ) -> Option<String> {
        let text = content["content"].as_str()?;
        let lines = text.lines().count();
        Some(format!("{} lines", lines))
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "task_id": {
                    "type": "string",
                    "description": "The task ID to get output from"
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for completion. Default: true"
                },
                "timeout": {
                    "type": "number",
                    "description": "Max wait time in ms. Default: 30000, max: 600000"
                }
            }),
            required: Some(vec!["task_id".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let task_id = input["task_id"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing required parameter: task_id".to_string()))?;

        let block = input["block"]
            .as_bool()
            .unwrap_or(true);
        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(30_000)
            .min(600_000);

        // Retrieve output from the background task registry
        let output = background_task_registry::get_task_output(task_id, block, timeout_ms).await;

        let result = serde_json::json!({
            "retrieval_status": output.status,
            "task": {
                "task_id": task_id,
                "task_type": output.task_type,
                "status": output.status,
                "description": output.description,
                "output": output.content
            }
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

impl Default for TaskOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::background_task_registry as bg;

    #[test]
    fn test_task_output_tool_name() {
        let tool = TaskOutputTool::new();
        assert_eq!(tool.name(), TASK_OUTPUT_TOOL_NAME);
    }

    #[test]
    fn test_task_output_tool_schema() {
        let tool = TaskOutputTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.get("task_id").is_some());
        assert!(schema.properties.get("block").is_some());
        assert!(schema.properties.get("timeout").is_some());
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_output_requires_task_id() {
        bg::reset_registry();
        let tool = TaskOutputTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_output_not_found() {
        bg::reset_registry();
        let tool = TaskOutputTool::new();
        let input = serde_json::json!({
            "task_id": "nonexistent-task",
            "block": false
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("not_found"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_output_completed_task() {
        bg::reset_registry();
        // Register and complete a task
        bg::register_task(
            "completed-task".to_string(),
            "local_bash".to_string(),
            "ls".to_string(),
            Some("ls".to_string()),
            None,
        );
        bg::complete_task("completed-task", "file.txt\n".to_string(), "".to_string());

        let tool = TaskOutputTool::new();
        let input = serde_json::json!({
            "task_id": "completed-task",
            "block": false
        });
        let result = tool.execute(input, &ToolContext::default()).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("completed"));
        assert!(content.contains("file.txt"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_output_blocking_mode() {
        bg::reset_registry();
        let tool = TaskOutputTool::new();
        let input = serde_json::json!({
            "task_id": "blocking-task-456",
            "block": true,
            "timeout": 100
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        // Should timeout since task not found
        let content = result.unwrap().content;
        assert!(content.contains("not_found"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_output_timeout_cap() {
        bg::reset_registry();
        let tool = TaskOutputTool::new();
        let input = serde_json::json!({
            "task_id": "timeout-task",
            "timeout": 999_999
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
    }
}
