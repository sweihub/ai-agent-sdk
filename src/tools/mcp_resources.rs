// Source: ~/claudecode/openclaudecode/src/tools/ListMcpResourcesTool/ListMcpResourcesTool.ts
use crate::error::AgentError;
use crate::services::mcp::client::{
    ensure_connected_client, fetch_resources_for_client, get_all_mcp_connections,
};
use crate::types::*;

pub const LIST_MCP_RESOURCES_TOOL_NAME: &str = "ListMcpResourcesTool";

pub const DESCRIPTION: &str = "List available resources from configured MCP servers";

/// ListMcpResourcesTool - list resources from MCP servers
pub struct ListMcpResourcesTool;

impl ListMcpResourcesTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        LIST_MCP_RESOURCES_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        DESCRIPTION
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "ListMcpResourcesTool".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["server"].as_str().map(String::from))
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
                    "description": "Optional server name to filter resources by"
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
        let server_filter = input["server"].as_str();

        // Get MCP connections from global registry
        let connections = get_all_mcp_connections();

        if connections.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: "No MCP servers configured. Configure MCP servers to list their resources."
                    .to_string(),
                is_error: Some(false),
                was_persisted: None,
            });
        }

        let mut all_resources: Vec<serde_json::Value> = vec![];
        let mut errors: Vec<String> = vec![];

        for (name, conn) in &connections {
            // Filter by server name if specified
            if let Some(filter) = server_filter {
                if name.to_lowercase() != filter.to_lowercase() {
                    continue;
                }
            }

            // Ensure the client is connected
            match ensure_connected_client(conn.clone()).await {
                Ok(_) => {
                    let resources = fetch_resources_for_client(&conn).await;
                    for resource in resources {
                        all_resources.push(serde_json::json!({
                            "uri": resource.uri,
                            "name": resource.name,
                            "description": resource.description,
                            "mimeType": resource.mime_type,
                            "server": resource.server,
                        }));
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", name, e));
                }
            }
        }

        if server_filter.is_some() && all_resources.is_empty() && errors.is_empty() {
            return Ok(ToolResult {
                result_type: "text".to_string(),
                tool_use_id: "".to_string(),
                content: format!(
                    "MCP server '{}' not found or not connected among {} configured servers.",
                    server_filter.unwrap(),
                    connections.len()
                ),
                is_error: Some(false),
                was_persisted: None,
            });
        }

        let result = serde_json::json!({
            "resources": all_resources,
            "errors": errors,
            "total": all_resources.len(),
        });

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap_or_default(),
            is_error: Some(false),
            was_persisted: None,
        })
    }
}

impl Default for ListMcpResourcesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_mcp_resources_tool_name() {
        let tool = ListMcpResourcesTool::new();
        assert_eq!(tool.name(), LIST_MCP_RESOURCES_TOOL_NAME);
    }

    #[tokio::test]
    async fn test_list_mcp_resources_no_servers() {
        crate::services::mcp::client::clear_mcp_connections();
        let tool = ListMcpResourcesTool::new();
        let input = serde_json::json!({});
        let context = ToolContext::default();
        let result = tool.execute(input, &context).await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("No MCP servers configured"));
    }
}
