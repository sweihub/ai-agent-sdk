// Source: ~/claudecode/openclaudecode/src/tools/SleepTool/prompt.ts
use crate::error::AgentError;
use crate::types::*;

pub const SLEEP_TOOL_NAME: &str = "Sleep";

pub const DESCRIPTION: &str = "Wait for a specified duration";

/// Sleep tool - wait for a duration without holding a shell process
pub struct SleepTool;

impl SleepTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        SLEEP_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "Sleep".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["duration"].as_f64().map(|d| format!("{:.1}s", d)))
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
                "duration": {
                    "type": "number",
                    "description": "Duration to sleep in seconds (default: 60)"
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
        let duration_secs = input["duration"].as_f64().unwrap_or(60.0);
        let duration_ms = (duration_secs * 1000.0) as u64;

        // Sleep for the specified duration
        tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!("Slept for {:.1} seconds", duration_secs),
            is_error: None,
            was_persisted: None,
        })
    }
}

impl Default for SleepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_tool_name() {
        let tool = SleepTool::new();
        assert_eq!(tool.name(), SLEEP_TOOL_NAME);
    }

    #[test]
    fn test_sleep_tool_schema() {
        let tool = SleepTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties["duration"].is_object());
    }
}
