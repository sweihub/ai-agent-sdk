// Source: ~/claudecode/openclaudecode/src/tools/TaskListTool/TaskListTool.ts
//! TaskList tool - list all tasks in the task list.

use crate::types::*;
use crate::utils::task_list::{get_task_list_id, is_todo_v2_enabled, list_tasks, TaskStatus};

use super::constants::TASK_LIST_TOOL_NAME;
use super::prompt::{get_prompt, DESCRIPTION};

/// Output of the TaskList tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskListOutput {
    pub tasks: Vec<TaskSummary>,
}

/// Simplified task summary returned by TaskList
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub subject: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub blocked_by: Vec<String>,
}

/// TaskList tool - list all tasks
pub struct TaskListTool;

impl TaskListTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_LIST_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn prompt(&self) -> String {
        get_prompt()
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: None,
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
        _input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, crate::error::AgentError> {
        let task_list_id = get_task_list_id();

        let all_tasks = list_tasks(&task_list_id)
            .await
            .map_err(|e| crate::error::AgentError::Tool(e))?;

        // Filter out internal tasks (matching TypeScript: filter(t => !t.metadata?._internal))
        let all_tasks: Vec<_> = all_tasks
            .into_iter()
            .filter(|t| {
                t.metadata
                    .as_ref()
                    .and_then(|m| m.get("_internal"))
                    .is_none()
            })
            .collect();

        // Build a set of resolved (completed) task IDs for filtering blockedBy
        let resolved_task_ids: std::collections::HashSet<_> = all_tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .map(|t| t.id.clone())
            .collect();

        let tasks: Vec<TaskSummary> = all_tasks
            .into_iter()
            .map(|task| {
                // Filter out resolved task IDs from blockedBy
                let blocked_by: Vec<String> = task
                    .blocked_by
                    .into_iter()
                    .filter(|id| !resolved_task_ids.contains(id))
                    .collect();

                TaskSummary {
                    id: task.id,
                    subject: task.subject,
                    status: task.status.to_string(),
                    owner: task.owner,
                    blocked_by,
                }
            })
            .collect();

        let output = TaskListOutput { tasks };

        let content = serde_json::to_string(&output)
            .unwrap_or_else(|_| "Failed to serialize tasks".to_string());

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "task_list".to_string(),
            content,
            is_error: Some(false),
            was_persisted: None,
        })
    }

    /// Map tool result to a readable text block (matches TypeScript mapToolResultToToolResultBlockParam)
    pub fn format_result(content: &serde_json::Value, tool_use_id: &str) -> String {
        if let Some(tasks) = content.get("tasks").and_then(|v| v.as_array()) {
            if tasks.is_empty() {
                return "No tasks found".to_string();
            }

            let lines: Vec<String> = tasks
                .iter()
                .map(|task| {
                    let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let subject = task.get("subject").and_then(|v| v.as_str()).unwrap_or("");
                    let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("");

                    let owner = task
                        .get("owner")
                        .and_then(|v| v.as_str())
                        .map(|o| format!(" ({o})"))
                        .unwrap_or_default();

                    let blocked = task
                        .get("blockedBy")
                        .and_then(|v| v.as_array())
                        .filter(|arr| !arr.is_empty())
                        .map(|arr| {
                            let ids: Vec<String> = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| format!("#{s}")))
                                .collect();
                            format!(" [blocked by {}]", ids.join(", "))
                        })
                        .unwrap_or_default();

                    format!("#{id} [{status}] {subject}{owner}{blocked}")
                })
                .collect();

            return lines.join("\n");
        }
        format!("Failed to parse task list result for tool {tool_use_id}")
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_list_tool_name() {
        let tool = TaskListTool::new();
        assert_eq!(tool.name(), TASK_LIST_TOOL_NAME);
    }

    #[test]
    fn test_task_list_tool_schema() {
        let tool = TaskListTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert_eq!(schema.required, None);
    }

    #[test]
    fn test_task_list_tool_is_read_only() {
        let tool = TaskListTool::new();
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_task_list_tool_is_concurrency_safe() {
        let tool = TaskListTool::new();
        assert!(tool.is_concurrency_safe());
    }

    #[test]
    fn test_task_list_tool_should_defer() {
        let tool = TaskListTool::new();
        assert!(tool.should_defer());
    }

    #[test]
    fn test_task_list_format_result_empty() {
        let result = serde_json::json!({ "tasks": [] });
        let formatted = TaskListTool::format_result(&result, "test-id");
        assert_eq!(formatted, "No tasks found");
    }

    #[test]
    fn test_task_list_format_result() {
        let result = serde_json::json!({
            "tasks": [
                {
                    "id": "1",
                    "subject": "First task",
                    "status": "pending",
                    "owner": "agent-1",
                    "blockedBy": ["0"]
                },
                {
                    "id": "2",
                    "subject": "Completed task",
                    "status": "completed"
                }
            ]
        });
        let formatted = TaskListTool::format_result(&result, "test-id");
        assert!(formatted.contains("#1 [pending] First task (agent-1) [blocked by #0]"));
        assert!(formatted.contains("#2 [completed] Completed task"));
    }
}
