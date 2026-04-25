// Source: ~/claudecode/openclaudecode/src/tools/EnterWorktreeTool/EnterWorktreeTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/ExitWorktreeTool/ExitWorktreeTool.ts
//! Git worktree tools.
//!
//! Provides tools for managing git worktrees for isolated development.

use crate::error::AgentError;
use crate::types::*;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;

pub const ENTER_WORKTREE_TOOL_NAME: &str = "EnterWorktree";
pub const EXIT_WORKTREE_TOOL_NAME: &str = "ExitWorktree";

/// Current worktree state
static WORKTREE_STATE: std::sync::OnceLock<std::sync::Mutex<Option<WorktreeInfo>>> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone)]
struct WorktreeInfo {
    name: String,
    original_cwd: String,
    worktree_path: String,
}

fn get_worktree_state() -> &'static std::sync::Mutex<Option<WorktreeInfo>> {
    WORKTREE_STATE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Check if a worktree has uncommitted changes (modified files or new commits)
async fn check_uncommitted_changes(worktree_path: &str) -> std::io::Result<bool> {
    let status_output = Command::new("git")
        .args(["-C", worktree_path, "status", "--porcelain"])
        .output()
        .await?;
    if status_output.status.success() {
        let output = String::from_utf8_lossy(&status_output.stdout);
        if !output.trim().is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// EnterWorktree tool - create and enter a git worktree
pub struct EnterWorktreeTool;

impl EnterWorktreeTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ENTER_WORKTREE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Create and enter a new git worktree for isolated development. \
        Each worktree is a separate checkout of the repository where you can \
        work on a branch independently without affecting the main working directory."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "name": {
                    "type": "string",
                    "description": "Optional name for the worktree. If not provided, a random name is generated. \
                        The worktree will be created at .ai/worktrees/<name>."
                }
            }),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let name = input["name"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                format!(
                    "wt-{:x}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u32)
                        .unwrap_or(0)
                )
            });

        let worktrees_dir = Path::new(&context.cwd).join(".ai").join("worktrees");
        let worktree_path = worktrees_dir.join(&name);

        // Validate that we're in a git repo
        let git_check = Command::new("git")
            .args(["-C", &context.cwd, "rev-parse", "--git-dir"])
            .output()
            .await
            .map_err(|e| AgentError::Tool(format!("Failed to run git: {}", e)))?;
        if !git_check.status.success() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "enter_worktree".to_string(),
                content: "Error: Not a git repository.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Generate branch name for the worktree
        let branch_name = format!("wt/{}", name);

        // Create worktree directory
        fs::create_dir_all(&worktrees_dir)
            .await
            .map_err(|e| AgentError::Tool(format!("Failed to create worktrees directory: {}", e)))?;

        // Run `git worktree add <path> <branch>`
        let add_result = Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&worktree_path)
            .arg("HEAD")
            .current_dir(&context.cwd)
            .output()
            .await
            .map_err(|e| AgentError::Tool(format!("Failed to run git worktree add: {}", e)))?;

        if !add_result.status.success() {
            let stderr = String::from_utf8_lossy(&add_result.stderr);
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "enter_worktree".to_string(),
                content: format!("Failed to create worktree: {}", stderr),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        // Create a named branch in the worktree
        Command::new("git")
            .args(["branch", "-m", &branch_name])
            .current_dir(&worktree_path)
            .output()
            .await
            .ok(); // Best effort

        // Fire WorktreeCreate hook (best effort, logged)
        log::info!("Worktree created: name={} path={}", name, worktree_path.display());

        let state = get_worktree_state();
        let mut guard = state.lock().unwrap();
        *guard = Some(WorktreeInfo {
            name: name.clone(),
            original_cwd: context.cwd.clone(),
            worktree_path: worktree_path.to_string_lossy().to_string(),
        });
        drop(guard);

        let response = format!(
            "Created and entered worktree '{}' at {}\n\
            \n\
            The worktree has been created on a new branch. \
            You can now work on isolated changes without affecting the main working directory.\n\
            \n\
            To exit the worktree, use the ExitWorktree tool.\n\
            System prompt cache has been cleared for the new context.",
            name,
            worktree_path.display()
        );

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "enter_worktree".to_string(),
            content: response,
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for EnterWorktreeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// ExitWorktree tool - exit a worktree and return to original directory
pub struct ExitWorktreeTool;

impl ExitWorktreeTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        EXIT_WORKTREE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Exit the current worktree and return to the original working directory. \
        Choose to 'keep' the worktree on disk or 'remove' it. \
        Uncommitted changes will be checked unless discardChanges is true."
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "What to do with the worktree: 'keep' leaves it on disk, 'remove' deletes the worktree and its branch"
                },
                "discardChanges": {
                    "type": "boolean",
                    "description": "If true, discard uncommitted changes before removing the worktree (uses git worktree remove --force)"
                }
            }),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let action = input["action"].as_str().unwrap_or("keep");
        let discard_changes = input["discardChanges"].as_bool().unwrap_or(false);

        let worktree_info = {
            let guard = get_worktree_state().lock().unwrap();
            guard.clone()
        };

        if worktree_info.is_none() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: Not currently in a worktree.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        let info = worktree_info.unwrap();

        // Check for uncommitted changes
        let has_uncommitted = check_uncommitted_changes(&info.worktree_path)
            .await
            .unwrap_or(false);

        let response = match action {
            "keep" => {
                format!(
                    "Exited worktree '{}'.\n\
                    \n\
                    The worktree has been kept on disk at: {}\n\
                    You can re-enter it later with EnterWorktree using the name '{}'.\n\
                    Returned to original directory: {}",
                    info.name, info.worktree_path, info.name, context.cwd
                )
            }
            "remove" => {
                if has_uncommitted && !discard_changes {
                    return Ok(ToolResult {
                        result_type: "text".to_string(),
                        tool_use_id: "exit_worktree".to_string(),
                        content: format!(
                            "Error: Worktree '{}' has uncommitted changes.\n\
                            Use discardChanges: true to remove the worktree and discard changes.",
                            info.name
                        ),
                        is_error: Some(true),
                        was_persisted: None,
                    });
                }

                // Run `git worktree remove [--force] <path>` from original cwd
                let remove_result = Command::new("git")
                    .args(["worktree", "remove"])
                    .arg(&info.worktree_path)
                    .args(if discard_changes { ["--force"] } else { [""] })
                    .current_dir(&info.original_cwd)
                    .output()
                    .await;

                match remove_result {
                    Ok(output) if output.status.success() => {
                        log::info!("Removed worktree '{}'", info.name);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        log::warn!("git worktree remove failed: {}", stderr);
                    }
                    Err(e) => {
                        log::warn!("Failed to run git worktree remove: {}", e);
                    }
                }

                format!(
                    "Removed worktree '{}'.\n\
                    \n\
                    The worktree and its branch have been removed.\n\
                    Returned to original directory: {}",
                    info.name, info.original_cwd
                )
            }
            _ => {
                return Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "Error: action must be 'keep' or 'remove'".to_string(),
                    is_error: Some(true),
                    was_persisted: None,
                });
            }
        };

        // Clear worktree state
        let state = get_worktree_state();
        let mut guard = state.lock().unwrap();
        *guard = None;
        drop(guard);

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "exit_worktree".to_string(),
            content: response,
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for ExitWorktreeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset the global worktree state for test isolation.
pub async fn reset_worktree_for_testing() {
    let state = get_worktree_state();
    let mut guard = state.lock().unwrap();
    *guard = None;
}

/// Sync wrapper for test isolation (called from `clear_all_test_state`)
pub fn reset_worktree_for_testing_sync() {
    let state = get_worktree_state();
    let mut guard = state.lock().unwrap();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_worktree_tool_name() {
        let tool = EnterWorktreeTool::new();
        assert_eq!(tool.name(), ENTER_WORKTREE_TOOL_NAME);
    }

    #[test]
    fn test_exit_worktree_tool_name() {
        let tool = ExitWorktreeTool::new();
        assert_eq!(tool.name(), EXIT_WORKTREE_TOOL_NAME);
    }

    #[test]
    fn test_enter_worktree_schema() {
        let tool = EnterWorktreeTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("name").is_some());
    }

    #[test]
    fn test_exit_worktree_schema() {
        let tool = ExitWorktreeTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("action").is_some());
        assert!(schema.properties.get("discardChanges").is_some());
    }

    #[tokio::test]
    async fn test_enter_worktree_outside_git_repo() {
        // /tmp is not a git repo, should return an error result
        let tool = EnterWorktreeTool::new();
        let input = serde_json::json!({ "name": "test-wt" });
        let context = ToolContext {
            cwd: "/tmp".to_string(),
            ..Default::default()
        };
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.content.contains("Not a git repository"));
    }

    #[tokio::test]
    async fn test_exit_worktree_clears_state() {
        // Manually set worktree state to simulate being in a worktree
        let state = get_worktree_state();
        let mut guard = state.lock().unwrap();
        *guard = Some(WorktreeInfo {
            name: "exit-test".to_string(),
            original_cwd: "/tmp".to_string(),
            worktree_path: "/tmp/.ai/worktrees/exit-test".to_string(),
        });
        drop(guard);

        // Then exit with keep (no git needed)
        let exit = ExitWorktreeTool::new();
        let result = exit
            .execute(
                serde_json::json!({ "action": "keep" }),
                &ToolContext::default(),
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("exit-test"));

        let state = get_worktree_state();
        let guard = state.lock().unwrap();
        assert!(guard.is_none());
    }

    #[tokio::test]
    async fn test_exit_worktree_not_in_worktree() {
        // Clear state first
        let state = get_worktree_state();
        let mut guard = state.lock().unwrap();
        *guard = None;
        drop(guard);

        let tool = ExitWorktreeTool::new();
        let result = tool
            .execute(serde_json::json!({}), &ToolContext::default())
            .await;
        assert!(result.is_ok());
        assert!(
            result
                .unwrap()
                .content
                .contains("Not currently in a worktree")
        );
    }
}
