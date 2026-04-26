// Source: ~/claudecode/openclaudecode/src/tools/EnterPlanModeTool/EnterPlanModeTool.ts
// Source: ~/claudecode/openclaudecode/src/tools/ExitPlanModeTool/ExitPlanModeV2Tool.ts
//! Plan mode tools - enter/exit structured planning workflow.
//!
//! Provides tools for switching between implementation and planning modes.

use crate::error::AgentError;
use crate::types::*;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub const ENTER_PLAN_MODE_TOOL_NAME: &str = "EnterPlanMode";
pub const EXIT_PLAN_MODE_TOOL_NAME: &str = "ExitPlanModeV2";

/// Global plan mode state
static IN_PLAN_MODE: OnceLock<AtomicBool> = OnceLock::new();

fn is_in_plan_mode() -> bool {
    IN_PLAN_MODE
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst)
}

fn set_plan_mode(val: bool) {
    IN_PLAN_MODE
        .get_or_init(|| AtomicBool::new(false))
        .store(val, Ordering::SeqCst);
}

/// Plan storage
static CURRENT_PLAN: OnceLock<std::sync::Mutex<String>> = OnceLock::new();

fn get_plan() -> String {
    CURRENT_PLAN
        .get_or_init(|| std::sync::Mutex::new(String::new()))
        .lock()
        .unwrap()
        .clone()
}

fn set_plan(plan: String) {
    *CURRENT_PLAN
        .get_or_init(|| std::sync::Mutex::new(String::new()))
        .lock()
        .unwrap() = plan;
}

/// EnterPlanMode tool - enter structured planning mode
pub struct EnterPlanModeTool;

impl EnterPlanModeTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ENTER_PLAN_MODE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Enter structured planning mode. Switches from implementation to planning workflow where you can explore the codebase and design an implementation approach."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "EnterPlanMode".to_string()
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
                "allowedPrompts": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Prompt-based permissions needed to implement the plan. These are shell command patterns that will be allowed during plan execution."
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
        let allowed = input["allowedPrompts"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        set_plan_mode(true);

        let response = if allowed.is_empty() {
            "Switched to plan mode. You can now explore the codebase and design an implementation plan. \
            When ready, use ExitPlanMode to present the plan for user approval."
                .to_string()
        } else {
            format!(
                "Switched to plan mode with permissions: {}.\n\
                You can now explore the codebase and design an implementation plan.\n\
                The following shell command patterns will be allowed during plan execution:\n\
                - {}\n\
                When ready, use ExitPlanMode to present the plan for user approval.",
                allowed.len(),
                allowed.join("\n- ")
            )
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "enter_plan_mode".to_string(),
            content: response,
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for EnterPlanModeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// ExitPlanModeV2 tool - exit planning mode and present plan for approval
pub struct ExitPlanModeTool;

impl ExitPlanModeTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        EXIT_PLAN_MODE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Exit plan mode and present the plan for user approval. Call this when you have finished designing the implementation approach."
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "ExitPlanMode".to_string()
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
            properties: serde_json::json!({}),
            required: None,
        }
    }

    pub async fn execute(
        &self,
        _input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        if !is_in_plan_mode() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "Error: Not currently in plan mode. Use EnterPlanMode first.".to_string(),
                is_error: Some(true),
                was_persisted: None,
            });
        }

        set_plan_mode(false);

        let plan = get_plan();
        let response = if plan.is_empty() {
            "Exiting plan mode. No plan has been created yet.\n\
            You should first explore the codebase and design an implementation approach\n\
            before exiting plan mode."
                .to_string()
        } else {
            format!(
                "Plan submitted for user approval.\n\
                The plan will be presented to the user for review and approval.\n\
                Once approved, you can proceed with implementation.\n\n\
                Plan summary:\n{}",
                plan
            )
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "exit_plan_mode".to_string(),
            content: response,
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for ExitPlanModeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset the global plan mode state and plan storage for test isolation.
pub fn reset_plan_for_testing() {
    // Reset IN_PLAN_MODE flag
    IN_PLAN_MODE
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    // Clear CURRENT_PLAN to empty string
    if let Some(plan_mutex) = CURRENT_PLAN.get() {
        let mut plan = plan_mutex.lock().unwrap();
        plan.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_plan_mode_name() {
        let tool = EnterPlanModeTool::new();
        assert_eq!(tool.name(), ENTER_PLAN_MODE_TOOL_NAME);
    }

    #[test]
    fn test_exit_plan_mode_name() {
        let tool = ExitPlanModeTool::new();
        assert_eq!(tool.name(), EXIT_PLAN_MODE_TOOL_NAME);
    }

    #[test]
    fn test_enter_plan_mode_schema() {
        let tool = EnterPlanModeTool::new();
        let schema = tool.input_schema();
        assert!(schema.properties.get("allowedPrompts").is_some());
    }

    #[tokio::test]
    async fn test_enter_plan_mode_sets_flag() {
        let tool = EnterPlanModeTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        assert!(is_in_plan_mode());
    }

    #[tokio::test]
    async fn test_exit_plan_mode_clears_flag() {
        set_plan_mode(true);
        let tool = ExitPlanModeTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        assert!(!is_in_plan_mode());
    }

    #[tokio::test]
    async fn test_exit_plan_mode_not_in_mode() {
        set_plan_mode(false);
        let tool = ExitPlanModeTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("Not currently in plan mode"));
    }

    #[tokio::test]
    async fn test_enter_plan_mode_with_permissions() {
        let tool = EnterPlanModeTool::new();
        let input = serde_json::json!({
            "allowedPrompts": ["npm run build", "git commit"]
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("permissions"));
        assert!(content.contains("npm run build"));
    }
}
