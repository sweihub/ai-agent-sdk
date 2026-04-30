// Source: ~/claudecode/openclaudecode/src/tasks/ (background task state management)
//! Background task registry for tracking running background tasks.
//!
//! Provides a shared registry that TaskStop and TaskOutput tools use
//! to manage background tasks (bash commands, agents, remote sessions).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Result of a background task
#[derive(Debug, Clone)]
pub enum BackgroundTaskResult {
    /// Task completed successfully
    Completed { stdout: String, stderr: String },
    /// Task failed with exit code
    Failed { stdout: String, stderr: String, exit_code: i32 },
    /// Task was killed/stopped
    Killed,
    /// Task is still running
    Running { partial_stdout: String, partial_stderr: String },
}

/// A tracked background task
#[derive(Debug)]
pub struct BackgroundTaskEntry {
    pub task_id: String,
    pub task_type: String,
    pub description: String,
    pub command: Option<String>,
    pub tool_use_id: Option<String>,
    pub status: BackgroundTaskStatus,
    pub result: Option<BackgroundTaskResult>,
    pub abort_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Clone for BackgroundTaskEntry {
    fn clone(&self) -> Self {
        Self {
            task_id: self.task_id.clone(),
            task_type: self.task_type.clone(),
            description: self.description.clone(),
            command: self.command.clone(),
            tool_use_id: self.tool_use_id.clone(),
            status: self.status.clone(),
            result: self.result.clone(),
            abort_handle: None, // Cannot clone JoinHandle
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl BackgroundTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackgroundTaskStatus::Pending => "pending",
            BackgroundTaskStatus::Running => "running",
            BackgroundTaskStatus::Completed => "completed",
            BackgroundTaskStatus::Failed => "failed",
            BackgroundTaskStatus::Killed => "killed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            BackgroundTaskStatus::Completed
                | BackgroundTaskStatus::Failed
                | BackgroundTaskStatus::Killed
        )
    }
}

/// Global background task registry
static REGISTRY: OnceLock<Mutex<HashMap<String, BackgroundTaskEntry>>> = OnceLock::new();

fn get_registry() -> &'static Mutex<HashMap<String, BackgroundTaskEntry>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a new background task
pub fn register_task(
    task_id: String,
    task_type: String,
    description: String,
    command: Option<String>,
    tool_use_id: Option<String>,
) -> BackgroundTaskEntry {
    let entry = BackgroundTaskEntry {
        task_id: task_id.clone(),
        task_type,
        description,
        command,
        tool_use_id,
        status: BackgroundTaskStatus::Pending,
        result: None,
        abort_handle: None,
    };
    get_registry().lock().unwrap().insert(task_id, entry.clone());
    entry
}

/// Start a task (transition to running)
pub fn start_task(task_id: &str) {
    let mut reg = get_registry().lock().unwrap();
    if let Some(entry) = reg.get_mut(task_id) {
        entry.status = BackgroundTaskStatus::Running;
    }
}

/// Set the abort handle for a task (so TaskStop can cancel it)
pub fn set_abort_handle(task_id: &str, handle: tokio::task::JoinHandle<()>) {
    let mut reg = get_registry().lock().unwrap();
    if let Some(entry) = reg.get_mut(task_id) {
        entry.abort_handle = Some(handle);
    }
}

/// Complete a task with output
pub fn complete_task(task_id: &str, stdout: String, stderr: String) {
    let mut reg = get_registry().lock().unwrap();
    if let Some(entry) = reg.get_mut(task_id) {
        entry.status = BackgroundTaskStatus::Completed;
        entry.result = Some(BackgroundTaskResult::Completed { stdout, stderr });
        entry.abort_handle = None;
    }
}

/// Update partial output for a running task
pub fn update_partial_output(task_id: &str, stdout: String, stderr: String) {
    let mut reg = get_registry().lock().unwrap();
    if let Some(entry) = reg.get_mut(task_id) {
        entry.result = Some(BackgroundTaskResult::Running {
            partial_stdout: stdout,
            partial_stderr: stderr,
        });
    }
}

/// Fail a task with exit code
pub fn fail_task(task_id: &str, stdout: String, stderr: String, exit_code: i32) {
    let mut reg = get_registry().lock().unwrap();
    if let Some(entry) = reg.get_mut(task_id) {
        entry.status = BackgroundTaskStatus::Failed;
        entry.result = Some(BackgroundTaskResult::Failed {
            stdout,
            stderr,
            exit_code,
        });
        entry.abort_handle = None;
    }
}

/// Kill a task by aborting its handle; returns a clone of the entry if successfully killed.
pub fn kill_task(task_id: &str) -> Option<BackgroundTaskEntry> {
    let mut reg = get_registry().lock().unwrap();
    let is_running = reg
        .get(task_id)
        .map(|e| e.status == BackgroundTaskStatus::Running)
        .unwrap_or(false);

    if is_running {
        // Abort the handle if present (must read before moving entry)
        if let Some(entry) = reg.get(task_id) {
            if let Some(handle) = &entry.abort_handle {
                handle.abort();
            }
        }
        // Update status
        if let Some(entry) = reg.get_mut(task_id) {
            entry.status = BackgroundTaskStatus::Killed;
            entry.result = Some(BackgroundTaskResult::Killed);
            entry.abort_handle = None;
        }
        // Return a clone (without the abort_handle)
        reg.get(task_id).cloned()
    } else {
        None
    }
}

/// Read current task state synchronously (no lock held across await).
fn read_task_state(task_id: &str) -> Option<TaskOutputData> {
    let reg = get_registry().lock().unwrap();
    let entry = reg.get(task_id)?;
    let data = format_task_output(task_id, entry);
    Some(data)
}

/// Format a task entry into TaskOutputData.
fn format_task_output(task_id: &str, entry: &BackgroundTaskEntry) -> TaskOutputData {
    match &entry.result {
        Some(BackgroundTaskResult::Completed { stdout, stderr }) => TaskOutputData {
            status: "completed".to_string(),
            task_type: entry.task_type.clone(),
            description: entry.description.clone(),
            content: if !stdout.is_empty() {
                stdout.clone()
            } else {
                stderr.clone()
            },
        },
        Some(BackgroundTaskResult::Failed {
            stdout,
            stderr,
            exit_code,
        }) => TaskOutputData {
            status: "failed".to_string(),
            task_type: entry.task_type.clone(),
            description: entry.description.clone(),
            content: format!(
                "Task failed with exit code {}\n\nStdout:\n{}\n\nStderr:\n{}",
                exit_code,
                if stdout.is_empty() { "(none)" } else { stdout.as_str() },
                if stderr.is_empty() { "(none)" } else { stderr.as_str() }
            ),
        },
        Some(BackgroundTaskResult::Killed) => TaskOutputData {
            status: "killed".to_string(),
            task_type: entry.task_type.clone(),
            description: entry.description.clone(),
            content: "Task was killed".to_string(),
        },
        Some(BackgroundTaskResult::Running {
            partial_stdout,
            partial_stderr,
        }) => TaskOutputData {
            status: "running".to_string(),
            task_type: entry.task_type.clone(),
            description: entry.description.clone(),
            content: format!(
                "Task is still running.\n\nPartial output:\n{}",
                if partial_stdout.is_empty() {
                    partial_stderr.clone()
                } else {
                    partial_stdout.clone()
                }
            ),
        },
        None => {
            let status = entry.status.clone();
            if status == BackgroundTaskStatus::Running {
                TaskOutputData {
                    status: "running".to_string(),
                    task_type: entry.task_type.clone(),
                    description: entry.description.clone(),
                    content: format!("Task '{}' is still running (no partial output available).", task_id),
                }
            } else {
                TaskOutputData {
                    status: status.as_str().to_string(),
                    task_type: entry.task_type.clone(),
                    description: entry.description.clone(),
                    content: format!("Task '{}' is in {} state.", task_id, status.as_str()),
                }
            }
        }
    }
}

/// Get task output for TaskOutput tool
pub async fn get_task_output(
    task_id: &str,
    block: bool,
    timeout_ms: u64,
) -> TaskOutputData {
    // First check: read current state (sync, lock released immediately)
    let initial = read_task_state(task_id);

    match initial {
        None => TaskOutputData {
            status: "not_found".to_string(),
            task_type: "unknown".to_string(),
            description: format!("Task '{}' not found in background task registry.", task_id),
            content: format!(
                "Task output not available for '{}'. In the SDK context, background task output is managed by the caller's task framework. \
                Use the BackgroundTaskRegistry API (register_task, complete_task, fail_task) to track tasks.",
                task_id
            ),
        },
        Some(data) if data.is_terminal_status() => data,
        Some(data) if !block => data,
        Some(data) => {
            // Task is running and blocking requested - poll until complete or timeout
            let timeout = Duration::from_millis(timeout_ms);
            let start = std::time::Instant::now();

            loop {
                if start.elapsed() >= timeout {
                    return TaskOutputData {
                        status: "timeout".to_string(),
                        task_type: data.task_type.clone(),
                        description: data.description.clone(),
                        content: format!(
                            "Timed out waiting for task '{}' after {}ms.",
                            task_id, timeout_ms
                        ),
                    };
                }
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Check again
                if let Some(updated) = read_task_state(task_id) {
                    if updated.is_terminal_status() {
                        return updated;
                    }
                }
            }
        }
    }
}

/// Remove a task from the registry
pub fn remove_task(task_id: &str) -> bool {
    get_registry().lock().unwrap().remove(task_id).is_some()
}

/// List all tasks in the registry
pub fn list_tasks() -> Vec<BackgroundTaskEntry> {
    get_registry().lock().unwrap().values().cloned().collect()
}

/// Clear the registry (for testing)
pub fn reset_registry() {
    let mut reg = get_registry().lock().unwrap();
    reg.clear();
}

pub struct TaskOutputData {
    pub status: String,
    pub task_type: String,
    pub description: String,
    pub content: String,
}

impl TaskOutputData {
    fn is_terminal_status(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed" | "failed" | "killed"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        reset_registry();
    }

    #[serial_test::serial]
    #[test]
    fn test_register_and_start_task() {
        setup();
        register_task(
            "test-1".to_string(),
            "local_bash".to_string(),
            "echo hello".to_string(),
            Some("echo hello".to_string()),
            None,
        );
        start_task("test-1");

        let tasks = list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, BackgroundTaskStatus::Running);
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_complete_task_output() {
        setup();
        register_task(
            "test-2".to_string(),
            "local_bash".to_string(),
            "ls".to_string(),
            Some("ls".to_string()),
            None,
        );
        complete_task("test-2", "file.txt\n".to_string(), "".to_string());

        let output = get_task_output("test-2", false, 1000).await;
        assert_eq!(output.status, "completed");
        assert!(output.content.contains("file.txt"));
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_fail_task_output() {
        setup();
        register_task(
            "test-3".to_string(),
            "local_bash".to_string(),
            "false".to_string(),
            Some("false".to_string()),
            None,
        );
        fail_task("test-3", "".to_string(), "error\n".to_string(), 1);

        let output = get_task_output("test-3", false, 1000).await;
        assert_eq!(output.status, "failed");
        assert!(output.content.contains("exit code 1"));
    }

    #[serial_test::serial]
    #[serial_test::serial]
    #[tokio::test]
    async fn test_not_found_task() {
        setup();
        let output = get_task_output("nonexistent", false, 1000).await;
        assert_eq!(output.status, "not_found");
    }

    #[serial_test::serial]
    #[serial_test::serial]
    #[test]
    fn test_kill_task() {
        setup();
        register_task(
            "test-4".to_string(),
            "local_bash".to_string(),
            "sleep 100".to_string(),
            Some("sleep 100".to_string()),
            None,
        );
        start_task("test-4");

        let killed = kill_task("test-4");
        assert!(killed.is_some());
        assert_eq!(killed.unwrap().status, BackgroundTaskStatus::Killed);

        // Cannot kill a killed task
        assert!(kill_task("test-4").is_none());
    }
}
