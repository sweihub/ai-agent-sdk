// Source: ~/claudecode/openclaudecode/src/types/tools.rs

use serde::{Deserialize, Serialize};

/// Trait for tool structs that provide render/display metadata.
/// All built-in tool structs implement this trait, enabling agent.rs
/// to construct `ToolRenderFns` without hardcoded display logic.
pub trait ToolRender: Send + Sync {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String;
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String>;
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String>;
}

/// Base tool progress data with flexible extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgressData {
    #[serde(rename = "kind", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Progress types for various tools, all sharing ToolProgressData structure.
pub type ShellProgress = ToolProgressData;
pub type BashProgress = ToolProgressData;
pub type PowerShellProgress = ToolProgressData;
pub type McpProgress = ToolProgressData;
pub type SkillToolProgress = ToolProgressData;
pub type TaskOutputProgress = ToolProgressData;
pub type WebSearchProgress = ToolProgressData;
pub type AgentToolProgress = ToolProgressData;
pub type ReplToolProgress = ToolProgressData;
pub type SdkWorkflowProgress = ToolProgressData;
