// Source: ~/claudecode/openclaudecode/src/tools/TaskGetTool/TaskGetTool.ts
//! TaskGet tool - retrieve a task by ID from the task list.

use crate::types::*;
use crate::utils::task_list::{get_task, get_task_list_id, is_todo_v2_enabled, Task, TaskStatus};

use super::constants::TASK_GET_TOOL_NAME;
use super::prompt::{DESCRIPTION, PROMPT};

/// Output of the TaskGet tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskGetOutput {
    pub task: Option<TaskInfo>,
}

/// Simplified task info returned by TaskGet
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: String,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
}

/// TaskGet tool - retrieve a task by ID
pub struct TaskGetTool;

impl TaskGetTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_GET_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn prompt(&self) -> &str {
        PROMPT
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to retrieve"
                }
            }),
            required: Some(vec!["taskId".to_string()]),
        }
    }

    pub fn is_enabled(&self) -> bool {
        is_todo_v2_enabled()
    }

    pub fn is_concurrency_safe(&self) -> bool {
        true
    }

    pub fn is_read_only(&self) -> bool {
        true
    }

    pub fn should_defer(&self) -> bool {
        true
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let task_id = input["taskId"].as_str().ok_or_else(|| {
            crate::error::AgentError::Tool("Missing taskId parameter".to_string())
        })?;

        let task_list_id = get_task_list_id();
        let task = get_task(&task_list_id, task_id)
            .await
            .map_err(|e| crate::error::AgentError::Tool(e))?;

        let output = TaskGetOutput {
            task: task.map(|t| TaskInfo {
                id: t.id,
                subject: t.subject,
                description: t.description,
                status: t.status.to_string(),
                blocks: t.blocks,
                blocked_by: t.blocked_by,
            }),
        };

        let content = serde_json::to_string(&output)
            .unwrap_or_else(|_| "Failed to serialize task".to_string());

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "task_get".to_string(),
            content,
            is_error: Some(false),
            was_persisted: None,
        })
    }

    /// Map tool result to a readable text block (matches TypeScript mapToolResultToToolResultBlockParam)
    pub fn format_result(content: &serde_json::Value, tool_use_id: &str) -> String {
        if let Some(task) = content.get("task") {
            if task.is_null() {
                return "Task not found".to_string();
            }
            if let Some(task_obj) = task.as_object() {
                let id = task_obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let subject = task_obj
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = task_obj
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let description = task_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let mut lines = vec![
                    format!("Task #{id}: {subject}"),
                    format!("Status: {status}"),
                    format!("Description: {description}"),
                ];

                if let Some(blocked_by) = task_obj.get("blockedBy").and_then(|v| v.as_array()) {
                    if !blocked_by.is_empty() {
                        let ids: Vec<String> = blocked_by
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| format!("#{s}")))
                            .collect();
                        lines.push(format!("Blocked by: {}", ids.join(", ")));
                    }
                }

                if let Some(blocks) = task_obj.get("blocks").and_then(|v| v.as_array()) {
                    if !blocks.is_empty() {
                        let ids: Vec<String> = blocks
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| format!("#{s}")))
                            .collect();
                        lines.push(format!("Blocks: {}", ids.join(", ")));
                    }
                }

                return lines.join("\n");
            }
        }
        format!("Failed to parse task result for tool {tool_use_id}")
    }
}

impl Default for TaskGetTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_get_tool_name() {
        let tool = TaskGetTool::new();
        assert_eq!(tool.name(), TASK_GET_TOOL_NAME);
    }

    #[test]
    fn test_task_get_tool_schema() {
        let tool = TaskGetTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("taskId").is_some());
        assert_eq!(schema.required, Some(vec!["taskId".to_string()]));
    }

    #[test]
    fn test_task_get_tool_is_read_only() {
        let tool = TaskGetTool::new();
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_task_get_tool_is_concurrency_safe() {
        let tool = TaskGetTool::new();
        assert!(tool.is_concurrency_safe());
    }

    #[test]
    fn test_task_get_tool_should_defer() {
        let tool = TaskGetTool::new();
        assert!(tool.should_defer());
    }

    #[test]
    fn test_task_get_format_result_null() {
        let result = serde_json::json!({ "task": null });
        let formatted = TaskGetTool::format_result(&result, "test-id");
        assert_eq!(formatted, "Task not found");
    }

    #[test]
    fn test_task_get_format_result() {
        let result = serde_json::json!({
            "task": {
                "id": "1",
                "subject": "Test task",
                "description": "Do something",
                "status": "pending",
                "blocks": ["2", "3"],
                "blockedBy": ["0"]
            }
        });
        let formatted = TaskGetTool::format_result(&result, "test-id");
        assert!(formatted.contains("Task #1: Test task"));
        assert!(formatted.contains("Status: pending"));
        assert!(formatted.contains("Blocked by: #0"));
        assert!(formatted.contains("Blocks: #2, #3"));
    }
}
