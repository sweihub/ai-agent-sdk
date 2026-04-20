// Source: ~/claudecode/openclaudecode/src/utils/tasks.ts
//! Task management utilities (TodoV2 task list system).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Task statuses
pub const TASK_STATUSES: [&str; 3] = ["pending", "in_progress", "completed"];

/// Task status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
        }
    }
}

/// Task representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub status: TaskStatus,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Check if TodoV2 is enabled
pub fn is_todo_v2_enabled() -> bool {
    // In non-interactive mode (e.g. SDK users who want Task tools over TodoWrite),
    // tasks can be force-enabled via environment variable
    let env_enabled = std::env::var("AI_CODE_ENABLE_TASKS")
        .map(|v| v == "1" || v == "true" || v == "yes")
        .unwrap_or(false);

    if env_enabled {
        return true;
    }

    // Default: enabled in interactive sessions
    // For now, always return true (the agent runs in interactive mode)
    true
}

/// Get the task list ID (directory identifier for this session's tasks)
pub fn get_task_list_id() -> String {
    // Use session ID or a unique identifier for the current session
    std::env::var("AI_CODE_SESSION_ID").ok().unwrap_or_else(|| {
        // Fallback: use a UUID-like identifier
        uuid::Uuid::new_v4().to_string()
    })
}

/// Get the tasks directory path
fn get_tasks_dir(task_list_id: &str) -> PathBuf {
    let config_dir = dirs::home_dir()
        .map(|d| d.join(".ai").join("tasks"))
        .unwrap_or_else(|| PathBuf::from("/tmp/.ai/tasks"));

    config_dir.join(task_list_id)
}

/// In-memory task store for single-session use
static TASK_STORE: OnceLock<Mutex<TaskStore>> = OnceLock::new();

struct TaskStore {
    tasks: HashMap<String, Task>,
    high_water_mark: u64,
}

impl TaskStore {
    fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            high_water_mark: 0,
        }
    }
}

fn get_store() -> &'static Mutex<TaskStore> {
    TASK_STORE.get_or_init(|| Mutex::new(TaskStore::new()))
}

/// Generate the next task ID
fn next_task_id() -> String {
    let mut store = get_store().lock().unwrap();
    store.high_water_mark += 1;
    store.high_water_mark.to_string()
}

/// Create a new task
pub async fn create_task(_task_list_id: &str, task: Task) -> Result<String, String> {
    let id = next_task_id();
    let mut new_task = task.clone();
    new_task.id = id.clone();

    let mut store = get_store().lock().unwrap();
    store.tasks.insert(id.clone(), new_task);
    Ok(id)
}

/// Get a task by ID
pub async fn get_task(_task_list_id: &str, task_id: &str) -> Result<Option<Task>, String> {
    let store = get_store().lock().unwrap();
    Ok(store.tasks.get(task_id).cloned())
}

/// List all tasks
pub async fn list_tasks(_task_list_id: &str) -> Result<Vec<Task>, String> {
    let store = get_store().lock().unwrap();
    Ok(store.tasks.values().cloned().collect())
}

/// Get all non-completed tasks from the in-memory store.
pub fn get_unfinished_tasks() -> Vec<Task> {
    let store = get_store().lock().unwrap();
    store
        .tasks
        .values()
        .filter(|t| t.status != TaskStatus::Completed)
        .cloned()
        .collect()
}

/// Update a task
pub async fn update_task(
    _task_list_id: &str,
    task_id: &str,
    updates: TaskUpdate,
) -> Result<(), String> {
    let mut store = get_store().lock().unwrap();
    if let Some(task) = store.tasks.get_mut(task_id) {
        if let Some(subject) = updates.subject {
            task.subject = subject;
        }
        if let Some(description) = updates.description {
            task.description = description;
        }
        if let Some(status) = updates.status {
            task.status = status;
        }
        if let Some(owner) = updates.owner {
            task.owner = Some(owner);
        }
        if let Some(active_form) = updates.active_form {
            task.active_form = Some(active_form);
        }
        if let Some(blocks) = updates.blocks {
            task.blocks = blocks;
        }
        if let Some(blocked_by) = updates.blocked_by {
            task.blocked_by = blocked_by;
        }
        Ok(())
    } else {
        Err(format!("Task {} not found", task_id))
    }
}

/// Delete a task
pub async fn delete_task(_task_list_id: &str, task_id: &str) -> Result<(), String> {
    let mut store = get_store().lock().unwrap();
    if store.tasks.remove(task_id).is_some() {
        Ok(())
    } else {
        Err(format!("Task {} not found", task_id))
    }
}

/// Task update fields
pub struct TaskUpdate {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
    pub active_form: Option<String>,
    pub blocks: Option<Vec<String>>,
    pub blocked_by: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_todo_v2_enabled() {
        assert!(is_todo_v2_enabled());
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Pending.to_string(), "pending");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
    }

    #[tokio::test]
    async fn test_create_and_get_task() {
        let task_list_id = get_task_list_id();
        let task = Task {
            id: String::new(),
            subject: "Test task".to_string(),
            description: "Test description".to_string(),
            active_form: None,
            owner: None,
            status: TaskStatus::Pending,
            blocks: vec![],
            blocked_by: vec![],
            metadata: None,
        };
        let id = create_task(&task_list_id, task).await.unwrap();
        assert_eq!(id, "1");

        let retrieved = get_task(&task_list_id, &id).await.unwrap().unwrap();
        assert_eq!(retrieved.subject, "Test task");
        assert_eq!(retrieved.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let task_list_id = get_task_list_id();
        let tasks = list_tasks(&task_list_id).await.unwrap();
        assert!(!tasks.is_empty());
    }

    #[tokio::test]
    async fn test_delete_task() {
        let task_list_id = get_task_list_id();
        let task = Task {
            id: String::new(),
            subject: "To delete".to_string(),
            description: "Will be deleted".to_string(),
            active_form: None,
            owner: None,
            status: TaskStatus::Pending,
            blocks: vec![],
            blocked_by: vec![],
            metadata: None,
        };
        let id = create_task(&task_list_id, task).await.unwrap();
        delete_task(&task_list_id, &id).await.unwrap();
        let retrieved = get_task(&task_list_id, &id).await.unwrap();
        assert!(retrieved.is_none());
    }
}
