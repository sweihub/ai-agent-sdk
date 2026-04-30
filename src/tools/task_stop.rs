// Source: ~/claudecode/openclaudecode/src/tools/TaskStopTool/TaskStopTool.ts
//! TaskStop tool - stop background tasks.
//!
//! Also known as KillShell (deprecated alias).
//! Integrates with the background task registry for real task stopping.

use crate::error::AgentError;
use crate::tools::background_task_registry;
use crate::types::*;

pub mod prompt;
pub use prompt::TASK_STOP_TOOL_NAME;

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

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "TaskStop".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["task_id"].as_str().map(String::from))
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

        let task_id =
            id.ok_or_else(|| AgentError::Tool("Missing required parameter: task_id".to_string()))?;

        // Try to kill the task in the background task registry
        let killed_entry = background_task_registry::kill_task(task_id);

        if let Some(entry) = killed_entry {
            let result = serde_json::json!({
                "message": format!("Successfully stopped task: {}", task_id),
                "task_id": task_id,
                "task_type": entry.task_type,
                "command": entry.command,
                "status": "killed"
            });

            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: serde_json::to_string_pretty(&result).unwrap_or_default(),
                is_error: Some(false),
                was_persisted: None,
            });
        }

        // Task not killed - check if it exists but is not running
        let tasks = background_task_registry::list_tasks();
        if let Some(entry) = tasks.iter().find(|t| t.task_id == task_id) {
            let result = serde_json::json!({
                "message": format!("Task {} is not running (status: {})", task_id, entry.status.as_str()),
                "task_id": task_id,
                "task_type": entry.task_type,
                "status": entry.status.as_str()
            });

            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: serde_json::to_string_pretty(&result).unwrap_or_default(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Task not found anywhere
        Err(AgentError::Tool(format!(
            "No task found with ID: {}. Background tasks are tracked when spawned with run_in_background=true.",
            task_id
        )))
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
    use crate::tools::background_task_registry as bg;

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

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_stop_not_found() {
        bg::reset_registry();
        let tool = TaskStopTool::new();
        let input = serde_json::json!({
            "task_id": "nonexistent-task"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent-task"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_stop_with_shell_id_compat() {
        bg::reset_registry();
        let tool = TaskStopTool::new();
        let input = serde_json::json!({
            "shell_id": "nonexistent-shell"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_task_stop_lifecycle() {
        bg::reset_registry();
        // Register and start a task
        bg::register_task(
            "lifecycle-test".to_string(),
            "local_bash".to_string(),
            "Test task".to_string(),
            Some("echo hello".to_string()),
            None,
        );
        bg::start_task("lifecycle-test");

        // Stop it
        let tool = TaskStopTool::new();
        let input = serde_json::json!({ "task_id": "lifecycle-test" });
        let result = tool.execute(input.clone(), &ToolContext::default()).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("killed"));

        // Try to stop again - should report not running
        let result = tool.execute(input, &ToolContext::default()).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("not running"));
    }
}
