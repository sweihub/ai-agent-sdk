// Source: ~/claudecode/openclaudecode/src/tools/TaskCreateTool/TaskCreateTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/TaskGetTool/TaskGetTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/TaskUpdateTool/TaskUpdateTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/TaskListTool/TaskListTool.ts
//! Task management tools (V2).
//!
//! Provides tools for creating, listing, updating, and getting tasks.

use crate::error::AgentError;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

pub const TASK_CREATE_TOOL_NAME: &str = "TaskCreate";
pub const TASK_GET_TOOL_NAME: &str = "TaskGet";
pub const TASK_LIST_TOOL_NAME: &str = "TaskList";
pub const TASK_UPDATE_TOOL_NAME: &str = "TaskUpdate";

/// Global task store
static TASKS: OnceLock<Mutex<HashMap<String, Task>>> = OnceLock::new();
static TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

fn get_tasks_map() -> &'static Mutex<HashMap<String, Task>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn reset_task_store() {
    let mut guard = get_tasks_map().lock().unwrap();
    guard.clear();
    drop(guard);
    TASK_COUNTER.store(1, Ordering::SeqCst);
}

/// Test-only lock that serializes concurrent tests using the task store.
/// Prevents race conditions when multiple tests run in parallel.
#[cfg(test)]
pub fn get_test_lock() -> &'static Mutex<()> {
    use std::sync::Mutex as StdMutex;
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
}

/// Get all non-completed, non-deleted tasks.
pub fn get_unfinished_tasks() -> Vec<Task> {
    let guard = get_tasks_map().lock().unwrap();
    guard
        .values()
        .filter(|t| t.status != "completed" && t.status != "deleted")
        .cloned()
        .collect()
}

/// Get all tasks (including deleted and internal).
pub fn get_all_tasks() -> Vec<Task> {
    let guard = get_tasks_map().lock().unwrap();
    guard.values().cloned().collect()
}

fn next_task_id() -> String {
    let id = TASK_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("task-{}", id)
}

/// A task in the V2 task system
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: String, // pending, in_progress, completed, deleted
    #[serde(rename = "activeForm")]
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub blocks: Vec<String>,     // task IDs this task blocks
    pub blocked_by: Vec<String>, // task IDs that block this task
    #[serde(rename = "_internal")]
    pub internal: Option<bool>,
}

impl Task {
    fn new(id: String, subject: String, description: String, active_form: Option<String>) -> Self {
        Self {
            id,
            subject,
            description,
            status: "pending".to_string(),
            active_form,
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
            internal: None,
        }
    }
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
        "Create a new task in the task list. Tasks can be tracked with status and can block other tasks."
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
                    "description": "Present continuous form shown in spinner when in_progress"
                }
            }),
            required: Some(vec!["subject".to_string(), "description".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let subject = input["subject"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("subject is required".to_string()))?
            .to_string();

        let description = input["description"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("description is required".to_string()))?
            .to_string();

        let active_form = input["activeForm"].as_str().map(|s| s.to_string());

        let id = next_task_id();
        let task = Task::new(
            id.clone(),
            subject.clone(),
            description.clone(),
            active_form.clone(),
        );

        let mut guard = get_tasks_map().lock().unwrap();
        guard.insert(id.clone(), task);
        drop(guard);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "Task created: {}\nSubject: {}\nID: {}",
                id,
                subject.clone(),
                id
            ),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
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
        "List all tasks in the task list. Shows task ID, subject, status, and blocking information."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        _input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let guard = get_tasks_map().lock().unwrap();

        // Filter out internal tasks (matching TS)
        let tasks: Vec<&Task> = guard
            .values()
            .filter(|t| t.internal != Some(true) && t.status != "deleted")
            .collect();

        if tasks.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "No tasks.".to_string(),
                is_error: None,
                was_persisted: None,
            });
        }

        let lines: Vec<String> = tasks
            .iter()
            .map(|t| {
                let blocking_note = if !t.blocks.is_empty() {
                    format!(" (blocks: {})", t.blocks.join(", "))
                } else {
                    String::new()
                };
                let owner_note = if let Some(owner) = &t.owner {
                    format!(" [{}]", owner)
                } else {
                    String::new()
                };
                format!(
                    "{}. {} [{}] - {}{}{}",
                    t.id,
                    t.subject,
                    t.status,
                    t.active_form.as_deref().unwrap_or(""),
                    owner_note,
                    blocking_note
                )
            })
            .collect();

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Tasks:\n{}", lines.join("\n")),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

/// TaskUpdate tool - update a task
pub struct TaskUpdateTool;

impl TaskUpdateTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_UPDATE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Update an existing task's status, subject, description, or other fields."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to update"
                },
                "subject": {
                    "type": "string",
                    "description": "New subject for the task"
                },
                "description": {
                    "type": "string",
                    "description": "New description for the task"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "deleted"],
                    "description": "New status for the task"
                },
                "activeForm": {
                    "type": "string",
                    "description": "New active form"
                },
                "owner": {
                    "type": "string",
                    "description": "New owner for the task"
                },
                "blocks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that this task blocks"
                },
                "blockedBy": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that block this task"
                }
            }),
            required: Some(vec!["taskId".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let task_id = input["taskId"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("taskId is required".to_string()))?;

        let mut guard = get_tasks_map().lock().unwrap();
        let task = guard
            .get_mut(task_id)
            .ok_or_else(|| AgentError::Tool(format!("Task '{}' not found", task_id)))?;

        let mut changes: Vec<String> = Vec::new();

        let old_status = task.status.clone();

        if let Some(subject) = input["subject"].as_str() {
            task.subject = subject.to_string();
            changes.push("subject".to_string());
        }
        if let Some(description) = input["description"].as_str() {
            task.description = description.to_string();
            changes.push("description".to_string());
        }
        if let Some(status) = input["status"].as_str() {
            task.status = status.to_string();
            changes.push(format!("status: {} -> {}", old_status, status));
        }
        if let Some(active_form) = input["activeForm"].as_str() {
            task.active_form = Some(active_form.to_string());
            changes.push("activeForm".to_string());
        }
        if let Some(owner) = input["owner"].as_str() {
            task.owner = Some(owner.to_string());
            changes.push(format!("owner -> {}", owner));
        }
        if let Some(blocks) = input["blocks"].as_array() {
            task.blocks = blocks
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            changes.push("blocks".to_string());
        }
        if let Some(blocked_by) = input["blockedBy"].as_array() {
            task.blocked_by = blocked_by
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            changes.push("blockedBy".to_string());
        }

        drop(guard);

        let changes_str = if changes.is_empty() {
            "no changes".to_string()
        } else {
            changes.join(", ")
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Task {} updated: {}", task_id, changes_str),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TaskUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

/// TaskGet tool - get a specific task
pub struct TaskGetTool;

impl TaskGetTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TASK_GET_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Get details of a specific task by ID."
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

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let task_id = input["taskId"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("taskId is required".to_string()))?;

        let guard = get_tasks_map().lock().unwrap();
        let task = guard
            .get(task_id)
            .ok_or_else(|| AgentError::Tool(format!("Task '{}' not found", task_id)))?;

        let content = serde_json::to_string_pretty(&serde_json::json!({
            "id": task.id,
            "subject": task.subject,
            "description": task.description,
            "status": task.status,
            "activeForm": task.active_form,
            "owner": task.owner,
            "blocks": task.blocks,
            "blockedBy": task.blocked_by
        }))
        .unwrap_or_default();

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content,
            is_error: Some(false),
            was_persisted: None,
        })
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

    fn test_setup() -> std::sync::MutexGuard<'static, ()> {
        let _lock = get_test_lock().lock().unwrap();
        reset_task_store();
        _lock
    }

    #[tokio::test]
    async fn test_task_create_and_get() {
        let _lock = test_setup();

        let create = TaskCreateTool::new();
        let result = create
            .execute(
                serde_json::json!({
                    "subject": "Test Task",
                    "description": "A test task",
                    "activeForm": "Testing"
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());

        // Extract the task ID from the create result (format: "ID: task-N")
        let content = result.unwrap().content;
        let task_id = content
            .lines()
            .find(|l| l.starts_with("ID: "))
            .unwrap()
            .strip_prefix("ID: ")
            .unwrap()
            .trim()
            .to_string();

        let get = TaskGetTool::new();
        let get_result = get
            .execute(
                serde_json::json!({ "taskId": task_id }),
                &ToolContext::default(),
            )
            .await;
        assert!(get_result.is_ok());
        let content = get_result.unwrap().content;
        assert!(content.contains("Test Task"));
    }

    #[tokio::test]
    async fn test_task_list() {
        let _lock = test_setup();

        let create = TaskCreateTool::new();
        create
            .execute(
                serde_json::json!({ "subject": "Task A", "description": "Desc A" }),
                &ToolContext::default(),
            )
            .await
            .unwrap();

        let list = TaskListTool::new();
        let result = list
            .execute(serde_json::json!({}), &ToolContext::default())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("Task A"));
    }

    #[tokio::test]
    async fn test_task_update_status() {
        let _lock = test_setup();

        let update = TaskUpdateTool::new();
        let result = update
            .execute(
                serde_json::json!({
                    "taskId": "task-1",
                    "status": "in_progress"
                }),
                &ToolContext::default(),
            )
            .await;
        // task-1 doesn't exist yet after reset, so create it first
        let create = TaskCreateTool::new();
        let create_result = create
            .execute(
                serde_json::json!({
                    "subject": "Update Me",
                    "description": "To be updated"
                }),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        let task_id = create_result
            .content
            .lines()
            .find(|l| l.starts_with("ID: "))
            .unwrap()
            .strip_prefix("ID: ")
            .unwrap()
            .trim()
            .to_string();

        let result = update
            .execute(
                serde_json::json!({
                    "taskId": task_id,
                    "status": "in_progress"
                }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());

        let get = TaskGetTool::new();
        let get_result = get
            .execute(
                serde_json::json!({ "taskId": task_id }),
                &ToolContext::default(),
            )
            .await;
        assert!(get_result.unwrap().content.contains("in_progress"));
    }
}
