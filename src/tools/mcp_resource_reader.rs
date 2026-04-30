// Source: ~/claudecode/openclaudecode/src/tools/ReadMcpResourceTool/ReadMcpResourceTool.ts
use crate::error::AgentError;
use crate::services::mcp::client::{
    ensure_connected_client, get_mcp_connection, read_mcp_resource,
};
use crate::types::*;

pub const READ_MCP_RESOURCE_TOOL_NAME: &str = "ReadMcpResourceTool";

pub const DESCRIPTION: &str = "Read a specific resource from an MCP server by URI";

/// ReadMcpResourceTool - read MCP resources by URI
pub struct ReadMcpResourceTool;

impl ReadMcpResourceTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        READ_MCP_RESOURCE_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "ReadMcpResourceTool".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["uri"].as_str().map(String::from))
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
                "server": {
                    "type": "string",
                    "description": "The MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "The resource URI to read"
                }
            }),
            required: Some(vec!["server".to_string(), "uri".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let server = input["server"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing server parameter".to_string()))?;

        let uri = input["uri"]
            .as_str()
            .ok_or_else(|| AgentError::Tool("Missing uri parameter".to_string()))?;

        // Look up MCP server connection from global registry
        let conn = get_mcp_connection(server).ok_or_else(|| {
            AgentError::Tool(format!(
                "MCP server '{}' not found in global registry. Configure MCP servers to read their resources.",
                server
            ))
        })?;

        // Ensure the client is connected
        ensure_connected_client(conn.clone())
            .await
            .map_err(|e| AgentError::Tool(format!("MCP server '{}' error: {}", server, e)))?;

        // Read the resource from the MCP server
        let result = read_mcp_resource(&conn, uri)
            .await
            .map_err(|e| AgentError::Tool(format!("Failed to read resource: {}", e)))?;

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap_or_default(),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for ReadMcpResourceTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_mcp_resource_tool_name() {
        let tool = ReadMcpResourceTool::new();
        assert_eq!(tool.name(), READ_MCP_RESOURCE_TOOL_NAME);
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_read_mcp_resource_missing_server() {
        crate::services::mcp::client::clear_mcp_connections();
        let tool = ReadMcpResourceTool::new();
        let input = serde_json::json!({
            "uri": "test://resource"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_read_mcp_resource_not_found() {
        crate::services::mcp::client::clear_mcp_connections();
        let tool = ReadMcpResourceTool::new();
        let input = serde_json::json!({
            "server": "nonexistent",
            "uri": "test://resource"
        });
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_err());
    }
}
