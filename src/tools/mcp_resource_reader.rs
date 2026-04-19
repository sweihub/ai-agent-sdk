// Source: ~/claudecode/openclaudecode/src/tools/ReadMcpResourceTool/ReadMcpResourceTool.ts
use crate::error::AgentError;
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

        // MCP server integration would go here
        // For now, return a not-available message
        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: format!(
                "MCP server '{}' not found or not connected. Cannot read resource: {}",
                server, uri
            ),
            is_error: None,
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
}
