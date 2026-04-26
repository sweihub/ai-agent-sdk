// Source: ~/claudecode/openclaudecode/src/tools/TodoWriteTool/TodoWriteTool.ts
//! TodoWrite tool - session todo list.
//!
//! Provides tool for managing session todo items with actual persistence.

use crate::error::AgentError;
use crate::tools::agent::constants::VERIFICATION_AGENT_TYPE;
use crate::types::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";

/// Global todo store - keyed by session/agent ID
static TODOS: OnceLock<Mutex<HashMap<String, Vec<TodoItem>>>> = OnceLock::new();

fn get_todos_map() -> &'static Mutex<HashMap<String, Vec<TodoItem>>> {
    TODOS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get all todos for a session, filtered to non-completed items.
pub fn get_unfinished_todos(session_key: &str) -> Vec<TodoItem> {
    let mut guard = get_todos_map().lock().unwrap();
    guard
        .get(session_key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.status != "completed")
        .collect()
}

/// Get all todos for a session (full list).
pub fn get_all_todos(session_key: &str) -> Vec<TodoItem> {
    let mut guard = get_todos_map().lock().unwrap();
    guard.get(session_key).cloned().unwrap_or_default()
}

/// A single todo item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String, // pending, in_progress, completed
    #[serde(rename = "ACTIVE_FORM")]
    pub active_form: Option<String>,
}

/// Todo list for a session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoList {
    pub old_todos: Vec<TodoItem>,
    pub new_todos: Vec<TodoItem>,
    pub verification_nudge_needed: Option<bool>,
}

/// TodoWrite tool - manage session todo list
pub struct TodoWriteTool;

impl TodoWriteTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TODO_WRITE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Update the todo list for this session. Provide the complete updated list of todos."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "TodoWrite".to_string()
    }

    pub fn get_tool_use_summary(&self, _input: Option<&serde_json::Value>) -> Option<String> {
        None
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
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "What needs to be done" },
                            "status": {
                                "type": "string",
                                "enum": ["in_progress", "pending", "completed"],
                                "description": "Current status of the task"
                            },
                            "ACTIVE_FORM": { "type": "string", "description": "Present continuous form for display" }
                        },
                        "required": ["content", "status"]
                    },
                    "description": "The updated todo list"
                }
            }),
            required: Some(vec!["todos".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let todos = input["todos"]
            .as_array()
            .ok_or_else(|| AgentError::Tool("todos is required".to_string()))?;

        let new_items: Vec<TodoItem> = todos
            .iter()
            .filter_map(|t| {
                let content = t.get("content")?.as_str()?.to_string();
                let status = t.get("status")?.as_str()?.to_string();
                let active_form = t
                    .get("ACTIVE_FORM")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Some(TodoItem {
                    content,
                    status,
                    active_form,
                })
            })
            .collect();

        // Use a default session key (in full impl, this comes from context.agentId or session ID)
        let todo_key = "default_session".to_string();

        let mut guard = get_todos_map().lock().unwrap();
        let old_todos = guard.get(&todo_key).cloned().unwrap_or_default();

        // If all done, clear the list (matching TS: allDone ? [] : todos)
        let all_done = new_items.iter().all(|t| t.status == "completed");
        let stored_todos = if all_done { vec![] } else { new_items.clone() };

        guard.insert(todo_key.clone(), stored_todos);
        drop(guard);

        // Verification nudge: 3+ items, none mention "verif"
        let verification_nudge_needed = all_done
            && new_items.len() >= 3
            && !new_items
                .iter()
                .any(|t| t.content.to_lowercase().contains("verif"));

        let base = "Todos have been modified successfully. \
            Ensure that you continue to use the todo list to track your progress. \
            Please proceed with the current tasks if applicable";

        let nudge = if verification_nudge_needed {
            format!(
                "\n\nNOTE: You just closed out {}+ tasks and none of them was a verification step. \
                Before writing your final summary, spawn the verification agent (subagent_type=\"{}\"). \
                You cannot self-assign PARTIAL by listing caveats in your summary — only the verifier issues a verdict.",
                new_items.len(),
                VERIFICATION_AGENT_TYPE
            )
        } else {
            String::new()
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "todo_write".to_string(),
            content: format!("{}{}", base, nudge),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for TodoWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset the global todo store for test isolation.
pub fn reset_todos_for_testing() {
    let mut guard = get_todos_map().lock().unwrap();
    guard.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tests::common::clear_all_test_state;

    #[test]
    fn test_todo_write_tool_name() {
        clear_all_test_state();
        let tool = TodoWriteTool::new();
        assert_eq!(tool.name(), TODO_WRITE_TOOL_NAME);
    }

    #[test]
    fn test_todo_write_schema() {
        clear_all_test_state();
        let tool = TodoWriteTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("todos").is_some());
    }

    #[tokio::test]
    async fn test_todo_write_creates_items() {
        clear_all_test_state();
        let tool = TodoWriteTool::new();
        let input = serde_json::json!({
            "todos": [
                { "content": "Task 1", "status": "pending" },
                { "content": "Task 2", "status": "in_progress" }
            ]
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("modified successfully"));
    }

    #[tokio::test]
    async fn test_todo_write_clears_when_all_done() {
        clear_all_test_state();
        let tool = TodoWriteTool::new();
        // First, add some todos
        let input = serde_json::json!({
            "todos": [
                { "content": "Task A", "status": "completed" },
                { "content": "Task B", "status": "completed" },
                { "content": "Task C", "status": "completed" },
                { "content": "Task D", "status": "completed" }
            ]
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("modified successfully"));
    }

    #[tokio::test]
    async fn test_todo_write_verification_nudge() {
        clear_all_test_state();
        let tool = TodoWriteTool::new();
        // 3+ items, none mention "verif", all completed
        let input = serde_json::json!({
            "todos": [
                { "content": "Implement feature", "status": "completed" },
                { "content": "Write tests", "status": "completed" },
                { "content": "Update docs", "status": "completed" }
            ]
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("verification step"));
        assert!(content.contains(VERIFICATION_AGENT_TYPE));
    }
}
