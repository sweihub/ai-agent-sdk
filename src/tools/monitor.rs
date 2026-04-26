// Source: ~/claudecode/openclaudecode/src/tools/MonitorTool/MonitorTool.ts
//! Monitor tool - placeholder for unimplemented functionality

use crate::types::*;

/// Monitor tool name
pub const MONITOR_TOOL_NAME: &str = "Monitor";

/// Monitor tool - placeholder for system monitoring functionality
/// TypeScript exports null (feature-gated/not implemented)
pub struct MonitorTool;

impl MonitorTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        MONITOR_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Monitor system resources and performance (not implemented)"
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "Monitor".to_string()
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
            "Monitor tool is not implemented".to_string(),
        ))
    }
}

impl Default for MonitorTool {
    fn default() -> Self {
        Self::new()
    }
}
