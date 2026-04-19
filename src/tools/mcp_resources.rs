// Source: ~/claudecode/openclaudecode/src/tools/ListMcpResourcesTool/ListMcpResourcesTool.ts
use crate::error::AgentError;
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
        let _server_filter = input["server"].as_str();

        // MCP server integration would go here
        // For now, return a not-available message
        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: "No MCP servers configured. Configure MCP servers to list their resources.".to_string(),
            is_error: None,
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
}
