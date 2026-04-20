// Inspector — final review of unfinished tasks at the end of a query loop.
//
// Roles and Jobs:
//
// **Role:** The Inspector acts as a silent gatekeeper between the agent's "done" and the
// real world. It catches the gap between what the LLM thinks is complete and what
// actually remains unfinished — ensuring the LLM sees its own blind spots before
// returning control to the user.
//
// **Jobs:**
//
// 1. **Scavenge** — Query every task store (TodoWrite, Task V2 both in-process and disk-backed)
//    for items whose status is not `"completed"` or `"deleted"`.
//
// 2. **Deduplicate** — The same task may exist in multiple stores with different IDs.
//    Merge entries by subject/ID to avoid reporting the same work twice.
//
// 3. **Summarize** — Format the incomplete items into a concise, actionable list grouped
//    by tool system (TODOs vs Tasks), including status, owner, and ACTIVE_FORM where available.
//
// 4. **Nudge** — Return a single system message that the LLM can read and act on.
//    If nothing is unfinished, return `None` so the query loop exits cleanly
//    without injecting an empty or redundant message into the conversation.
//
// **When it runs:** At every "graceful exit" point in the query loop
// (`streaming_result.tool_calls.is_empty()`), *before* returning `ExitReason::Completed`.
// If any unfinished items are found, the message is injected as a System message
// and the loop continues (consuming one turn) so the LLM can address them.

/// Information about an unfinished todo item
#[derive(Debug, Clone)]
pub struct TodoItemInfo {
    pub content: String,
    pub status: String,
    pub active_form: Option<String>,
}

/// Information about an unfinished task
#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: String,
    pub subject: String,
    pub status: String,
    pub owner: Option<String>,
}

/// Inspect all task stores for unfinished items and return a nudge message if any exist.
///
/// The entry point called from `query_engine.rs` at the end of a query turn.
/// Returns `Some(message)` if there is work left to do, `None` if everything is complete.
pub fn check() -> Option<String> {
    let incomplete_todos = collect_unfinished_todos();
    let incomplete_tasks = collect_unfinished_tasks();

    if incomplete_todos.is_empty() && incomplete_tasks.is_empty() {
        return None;
    }

    Some(format_nudge(&incomplete_todos, &incomplete_tasks))
}

fn collect_unfinished_todos() -> Vec<TodoItemInfo> {
    let session_key = "default_session";
    crate::tools::todo::get_unfinished_todos(session_key)
        .into_iter()
        .map(|t| TodoItemInfo {
            content: t.content,
            status: t.status,
            active_form: t.active_form,
        })
        .collect()
}

fn collect_unfinished_tasks() -> Vec<TaskInfo> {
    let mut tasks = crate::tools::tasks::get_unfinished_tasks()
        .into_iter()
        .map(|t| TaskInfo {
            id: t.id,
            subject: t.subject,
            status: t.status,
            owner: t.owner,
        })
        .collect::<Vec<_>>();

    // Also check the V2 task_list store (different key space, numeric IDs)
    for t in crate::utils::task_list::get_unfinished_tasks() {
        // Avoid duplicates by ID
        if !tasks.iter().any(|ti| ti.id == t.id) {
            tasks.push(TaskInfo {
                id: t.id,
                subject: t.subject,
                status: t.status.to_string(),
                owner: t.owner,
            });
        }
    }

    tasks
}

/// Format a nudge message listing unfinished tasks.
pub fn format_nudge(
    incomplete_todos: &[TodoItemInfo],
    incomplete_tasks: &[TaskInfo],
) -> String {
    let mut lines = Vec::new();

    lines.push("You have unfinished items that may not be complete. Review and address them:".to_string());

    if !incomplete_todos.is_empty() {
        lines.push(String::new());
        lines.push("**TODOs:**".to_string());
        for todo in incomplete_todos {
            let status_tag = format!("[{}]", todo.status);
            let active = todo
                .active_form
                .as_deref()
                .map(|a| format!(" ({})", a))
                .unwrap_or_default();
            lines.push(format!("  - {} {}{}", status_tag, todo.content, active));
        }
    }

    if !incomplete_tasks.is_empty() {
        lines.push(String::new());
        lines.push("**Tasks:**".to_string());
        for task in incomplete_tasks {
            let owner = task
                .owner
                .as_deref()
                .map(|o| format!(" ({})", o))
                .unwrap_or_default();
            lines.push(format!("  - {} [{}]{}{}", task.id, task.status, task.subject, owner));
        }
    }

    lines.push(String::new());
    lines.push(
        "Please continue working on these unfinished items before considering the task complete."
            .to_string(),
    );

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_empty_nudge() {
        let msg = format_nudge(&[], &[]);
        assert!(msg.contains("unfinished items"));
        assert!(msg.contains("continue working"));
    }

    #[test]
    fn test_format_nudge_with_todos() {
        let todos = vec![
            TodoItemInfo {
                content: "Implement feature X".to_string(),
                status: "in_progress".to_string(),
                active_form: Some("Implementing feature X".to_string()),
            },
            TodoItemInfo {
                content: "Write tests".to_string(),
                status: "pending".to_string(),
                active_form: None,
            },
        ];
        let msg = format_nudge(&todos, &[]);
        assert!(msg.contains("in_progress"));
        assert!(msg.contains("Implement feature X"));
        assert!(msg.contains("pending"));
        assert!(msg.contains("Write tests"));
    }

    #[test]
    fn test_format_nudge_with_tasks() {
        let tasks = vec![
            TaskInfo {
                id: "task-1".to_string(),
                subject: "Add error handling".to_string(),
                status: "pending".to_string(),
                owner: Some("agent-1".to_string()),
            },
        ];
        let msg = format_nudge(&[], &tasks);
        assert!(msg.contains("task-1"));
        assert!(msg.contains("[pending]"));
        assert!(msg.contains("Add error handling"));
        assert!(msg.contains("(agent-1)"));
    }

    #[test]
    fn test_format_nudge_combined() {
        let todos = vec![TodoItemInfo {
            content: "Fix bug".to_string(),
            status: "in_progress".to_string(),
            active_form: Some("Fixing bug".to_string()),
        }];
        let tasks = vec![TaskInfo {
            id: "task-2".to_string(),
            subject: "Update docs".to_string(),
            status: "pending".to_string(),
            owner: None,
        }];
        let msg = format_nudge(&todos, &tasks);
        assert!(msg.contains("**TODOs:**"));
        assert!(msg.contains("**Tasks:**"));
        assert!(msg.contains("Fix bug"));
        assert!(msg.contains("task-2"));
    }

    #[test]
    fn test_check_no_stores() {
        // With no todos/tasks stored, should return None
        // Note: this test may be affected by other tests that modify global state
        let todos = collect_unfinished_todos();
        let tasks = collect_unfinished_tasks();
        // Just verify the function doesn't panic
        let _ = check();
    }
}
