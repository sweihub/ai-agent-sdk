// Source: ~/claudecode/openclaudecode/src/tools/RemoteTriggerTool/RemoteTriggerTool.ts
use crate::error::AgentError;
use crate::types::*;

pub const REMOTE_TRIGGER_TOOL_NAME: &str = "RemoteTrigger";

pub const DESCRIPTION: &str = "Manage scheduled remote Claude Code agents (triggers) via the claude.ai CCR API";

/// RemoteTrigger tool - manage remote agent triggers via API
pub struct RemoteTriggerTool;

impl RemoteTriggerTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        REMOTE_TRIGGER_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "action": {
                    "type": "string",
                    "enum": ["list", "get", "create", "update", "run"],
                    "description": "The action to perform"
                },
                "trigger_id": {
                    "type": "string",
                    "description": "Required for get, update, and run"
                },
                "body": {
                    "type": "object",
                    "description": "JSON body for create and update"
                }
            }),
            required: Some(vec!["action".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing action parameter".to_string()))?;

        let trigger_id = input["trigger_id"].as_str();
        let body = input.get("body");

        // Build the API URL and method based on action
        let base_url = "https://api.claude.ai/v1/code/triggers";
        let (method, url, request_body) = match action {
            "list" => ("GET", base_url.to_string(), None),
            "get" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("get action requires trigger_id".to_string())
                })?;
                ("GET", format!("{}/{}", base_url, tid), None)
            }
            "create" => {
                let b = body.ok_or_else(|| {
                    AgentError::Tool("create action requires body".to_string())
                })?;
                ("POST", base_url.to_string(), Some(b.clone()))
            }
            "update" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("update action requires trigger_id".to_string())
                })?;
                let b = body.ok_or_else(|| {
                    AgentError::Tool("update action requires body".to_string())
                })?;
                ("POST", format!("{}/{}", base_url, tid), Some(b.clone()))
            }
            "run" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("run action requires trigger_id".to_string())
                })?;
                ("POST", format!("{}/run", base_url), Some(serde_json::json!({})))
            }
            _ => {
                return Err(AgentError::Tool(format!(
                    "Unknown action: {}",
                    action
                )))
            }
        };

        // Note: In a full implementation, this would make the actual HTTP request
        // with OAuth authentication. For now, return a not-implemented response.
        let result = serde_json::json!({
            "status": 501,
            "json": {
                "message": "RemoteTrigger tool requires OAuth authentication with claude.ai. This feature is not available in the current build.",
                "action": action,
                "url": url,
                "method": method
            }
        });

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap_or_default(),
            is_error: None,
            was_persisted: None,
        })
    }
}

impl Default for RemoteTriggerTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_trigger_tool_name() {
        let tool = RemoteTriggerTool::new();
        assert_eq!(tool.name(), REMOTE_TRIGGER_TOOL_NAME);
    }

    #[test]
    fn test_remote_trigger_tool_schema() {
        let tool = RemoteTriggerTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert_eq!(schema.required, Some(vec!["action".to_string()]));
    }
}
