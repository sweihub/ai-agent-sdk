// Source: ~/claudecode/openclaudecode/src/tools/TaskCreateTool/TaskCreateTool.ts
//! TaskCreate tool - create a new task in the task list.

use crate::types::*;
use crate::utils::task_list::{
    create_task, delete_task, get_task_list_id, is_todo_v2_enabled, Task, TaskStatus,
};

use super::constants::TASK_CREATE_TOOL_NAME;
use super::prompt::{get_prompt, DESCRIPTION};

/// Output of the TaskCreate tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskCreateOutput {
    pub task: CreatedTask,
}

/// Created task info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreatedTask {
    pub id: String,
    pub subject: String,
}

/// Result from a task created hook
pub struct TaskCreatedHookResult {
    pub blocking_error: Option<String>,
}

/// Execute task created hooks (stub implementation matching TypeScript behavior).
/// Returns an async iterator of hook results.
pub async fn execute_task_created_hooks(
    _task_id: &str,
    _subject: &str,
    _description: &str,
    _agent_name: Option<String>,
    _team_name: Option<String>,
) -> Vec<TaskCreatedHookResult> {
    // In the TypeScript version, this executes registered hooks that can
    // perform side effects when a task is created. If any hook returns
    // a blocking error, the task creation should be rolled back.
    //
    // For now, return empty results (no hooks registered).
    Vec::new()
}

/// Get formatted error message from a hook blocking error
pub fn get_task_created_hook_message(error: &str) -> String {
    format!("Task creation hook failed: {error}")
}

/// TaskCreate tool - create a new task
pub struct TaskCreateTool;

impl TaskCreateTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_CREATE_TOOL_NAME
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
            properties: serde_json::json!({
                "subject": {
                    "type": "string",
                    "description": "A brief title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "What needs to be done"
                },
                "activeForm": {
                    "type": "string",
                    "description": "Present continuous form shown in spinner when in_progress (e.g., \"Running tests\")"
                },
                "metadata": {
                    "type": "object",
                    "description": "Arbitrary metadata to attach to the task",
                    "additionalProperties": true
                }
            }),
            required: Some(vec!["subject".to_string(), "description".to_string()]),
        }
    }

    pub fn is_enabled(&self) -> bool {
        is_todo_v2_enabled()
    }

    pub fn is_concurrency_safe(&self) -> bool {
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
        let subject = input["subject"]
            .as_str()
            .ok_or_else(|| crate::error::AgentError::Tool("Missing subject parameter".to_string()))?
            .to_string();

        let description = input["description"]
            .as_str()
            .ok_or_else(|| {
                crate::error::AgentError::Tool("Missing description parameter".to_string())
            })?
            .to_string();

        let active_form = input["activeForm"].as_str().map(String::from);

        let metadata = input["metadata"]
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());

        let task_list_id = get_task_list_id();

        let task = Task {
            id: String::new(),
            subject: subject.clone(),
            description: description.clone(),
            active_form,
            status: TaskStatus::Pending,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            metadata,
        };

        let task_id = create_task(&task_list_id, task)
            .await
            .map_err(|e| crate::error::AgentError::Tool(e))?;

        // Execute task created hooks
        let hook_results = execute_task_created_hooks(
            &task_id,
            &subject,
            &description,
            None, // agent name
            None, // team name
        )
        .await;

        // Check for blocking errors from hooks
        let blocking_errors: Vec<String> = hook_results
            .into_iter()
            .filter_map(|r| r.blocking_error)
            .map(|e| get_task_created_hook_message(&e))
            .collect();

        if !blocking_errors.is_empty() {
            // Roll back: delete the task since hooks failed
            let _ = delete_task(&task_list_id, &task_id).await;
            return Err(crate::error::AgentError::Tool(blocking_errors.join("\n")));
        }

        // Auto-expand task list when creating tasks
        // (In TypeScript, this sets app state expandedView to 'tasks')
        // We skip this in Rust as it's a UI concern handled by the TUI layer.

        let output = TaskCreateOutput {
            task: CreatedTask {
                id: task_id,
                subject,
            },
        };

        let content = serde_json::to_string(&output)
            .unwrap_or_else(|_| "Failed to serialize task".to_string());

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "task_create".to_string(),
            content,
            is_error: Some(false),
            was_persisted: None,
        })
    }

    /// Map tool result to a readable text block (matches TypeScript mapToolResultToToolResultBlockParam)
    pub fn format_result(content: &serde_json::Value, tool_use_id: &str) -> String {
        if let Some(task) = content.get("task") {
            if let Some(task_obj) = task.as_object() {
                let id = task_obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let subject = task_obj
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return format!("Task #{id} created successfully: {subject}");
            }
        }
        format!("Failed to parse task create result for tool {tool_use_id}")
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_create_tool_name() {
        let tool = TaskCreateTool::new();
        assert_eq!(tool.name(), TASK_CREATE_TOOL_NAME);
    }

    #[test]
    fn test_task_create_tool_schema() {
        let tool = TaskCreateTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("subject").is_some());
        assert!(schema.properties.get("description").is_some());
        assert!(schema.properties.get("activeForm").is_some());
        assert!(schema.properties.get("metadata").is_some());
        assert_eq!(
            schema.required,
            Some(vec!["subject".to_string(), "description".to_string()])
        );
    }

    #[test]
    fn test_task_create_tool_is_concurrency_safe() {
        let tool = TaskCreateTool::new();
        assert!(tool.is_concurrency_safe());
    }

    #[test]
    fn test_task_create_tool_should_defer() {
        let tool = TaskCreateTool::new();
        assert!(tool.should_defer());
    }

    #[test]
    fn test_task_create_format_result() {
        let result = serde_json::json!({
            "task": {
                "id": "1",
                "subject": "New task"
            }
        });
        let formatted = TaskCreateTool::format_result(&result, "test-id");
        assert_eq!(formatted, "Task #1 created successfully: New task");
    }
}
