// Source: /data/home/swei/claudecode/openclaudecode/src/tools/SendUserFileTool/prompt.ts
//! SendUserFile tool - returns null (feature not implemented)

use crate::types::*;

/// SendUserFile tool name (from TypeScript prompt.ts)
pub const SEND_USER_FILE_TOOL_NAME: &str = "send_user_file";

/// SendUserFile tool - placeholder for sending user files
/// Feature-gated (KAIROS) in TypeScript
pub struct SendUserFileTool;

impl SendUserFileTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        SEND_USER_FILE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Send a file from the user to the agent (not implemented)"
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "SendUserFile".to_string()
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
    ) -> Result<ToolResult, crate::error::AgentError> {
        Err(crate::error::AgentError::ToolNotImplemented(
            "SendUserFile tool is not implemented".to_string(),
        ))
    }
}

impl Default for SendUserFileTool {
    fn default() -> Self {
        Self::new()
    }
}
