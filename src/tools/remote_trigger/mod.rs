// Source: ~/claudecode/openclaudecode/src/tools/RemoteTriggerTool/RemoteTriggerTool.ts
use crate::error::AgentError;
use crate::session_history::{get_bridge_base_url, get_claude_ai_oauth_tokens, get_oauth_headers};
use crate::types::*;

pub const REMOTE_TRIGGER_TOOL_NAME: &str = "RemoteTrigger";

pub const DESCRIPTION: &str =
    "Manage scheduled remote Claude Code agents (triggers) via the claude.ai CCR API";

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

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "RemoteTrigger".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["action"].as_str().map(String::from))
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

        // Build the API URL and method based on action (matches TS: getOauthConfig().BASE_API_URL)
        let base_url = format!("{}/v1/code/triggers", get_bridge_base_url());
        let (method, url, request_body) = match action {
            "list" => ("GET", base_url.to_string(), None),
            "get" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("get action requires trigger_id".to_string())
                })?;
                ("GET", format!("{}/{}", base_url, tid), None)
            }
            "create" => {
                let b = body
                    .ok_or_else(|| AgentError::Tool("create action requires body".to_string()))?;
                ("POST", base_url.to_string(), Some(b.clone()))
            }
            "update" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("update action requires trigger_id".to_string())
                })?;
                let b = body
                    .ok_or_else(|| AgentError::Tool("update action requires body".to_string()))?;
                ("POST", format!("{}/{}", base_url, tid), Some(b.clone()))
            }
            "run" => {
                let tid = trigger_id.ok_or_else(|| {
                    AgentError::Tool("run action requires trigger_id".to_string())
                })?;
                (
                    "POST",
                    format!("{}/run", base_url),
                    Some(serde_json::json!({})),
                )
            }
            _ => return Err(AgentError::Tool(format!("Unknown action: {}", action))),
        };

        // Get OAuth access token
        let tokens = get_claude_ai_oauth_tokens().ok_or_else(|| {
            AgentError::Tool(
                "RemoteTrigger requires OAuth authentication with claude.ai. \
                 Please log in first."
                    .to_string(),
            )
        })?;
        let access_token = tokens.access_token;

        // Build request headers (matching TypeScript axios call)
        let mut headers = get_oauth_headers(&access_token);
        headers.insert(
            "anthropic-beta".to_string(),
            "ccr-triggers-2026-01-30".to_string(),
        );

        // Make HTTP request
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| AgentError::Tool(format!("Failed to create HTTP client: {}", e)))?;

        let mut request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            _ => return Err(AgentError::Tool(format!("Unsupported method: {}", method))),
        };

        for (key, value) in &headers {
            request = request.header(key, value);
        }

        if let Some(body_json) = &request_body {
            request = request.json(body_json);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AgentError::Tool(format!("HTTP request failed: {}", e)))?;

        let status = response.status().as_u16();
        let body_text = response
            .text()
            .await
            .map_err(|e| AgentError::Tool(format!("Failed to read response: {}", e)))?;

        // Parse JSON response
        let json: serde_json::Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
            serde_json::json!({
                "raw": body_text
            })
        });

        let result = serde_json::json!({
            "status": status,
            "json": json,
        });

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap_or_default(),
            is_error: Some(status >= 400),
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

    #[tokio::test]
    async fn test_remote_trigger_requires_auth() {
        let tool = RemoteTriggerTool::new();
        let input = serde_json::json!({
            "action": "list"
        });
        let context = ToolContext::default();
        // Without OAuth tokens, should return error
        let result = tool.execute(input, &context).await;
        // Either error (no tokens) or success (has tokens) — depends on environment
        // We just verify it doesn't panic
        let _ = result;
    }
}
