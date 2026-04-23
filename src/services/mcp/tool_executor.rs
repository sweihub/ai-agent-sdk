// Source: ~/claudecode/openclaudecode/src/services/mcp/client.ts (callToolForClient)
//! MCP tool execution — dispatches mcp__ prefixed tool calls to registered MCP servers.
//!
//! In the SDK, MCP clients are managed externally. This module provides:
//! 1. Parsing of `mcp__serverName_toolName` tool names
//! 2. A callback-based dispatch mechanism for MCP tool calls
//! 3. Integration with the tool execution pipeline

use crate::error::AgentError;
use crate::types::{ToolContext, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;

/// Parses an MCP tool name `mcp__serverName_toolName` into (server_name, tool_name).
pub fn parse_mcp_tool_name(full_name: &str) -> Option<(String, String)> {
    let without_prefix = full_name.strip_prefix("mcp__")?;
    let mut parts = without_prefix.splitn(2, '_');
    let server_name = parts.next()?.to_string();
    let tool_name = parts.next()?.to_string();
    Some((server_name, tool_name))
}

/// Result of an MCP tool call.
#[derive(Debug, Clone)]
pub struct McpToolResult {
    pub content: Vec<serde_json::Value>,
    pub is_error: bool,
    pub _meta: Option<serde_json::Value>,
}

/// Callback type for MCP tool execution. Returns Result<ToolResult, AgentError>.
///
/// The SDK user implements this to connect to their MCP client.
/// Takes (server_name, tool_name, arguments_json).
pub type McpCallback = Arc<
    dyn Fn(String, String, serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send + Sync>>
        + Send
        + Sync,
>;

/// Registry of MCP server callbacks.
#[derive(Clone, Default)]
pub struct McpToolRegistry {
    callbacks: Arc<HashMap<String, McpCallback>>,
}

impl McpToolRegistry {
    pub fn new() -> Self {
        Self {
            callbacks: Arc::new(HashMap::new()),
        }
    }

    /// Register an MCP server callback.
    pub fn register<F, Fut>(&mut self, server_name: String, callback: F)
    where
        F: Fn(String, String, serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<ToolResult, AgentError>> + Send + Sync + 'static,
    {
        let wrapped: McpCallback = Arc::new(move |server: String, tool: String, args: serde_json::Value| {
            Box::pin(callback(server, tool, args))
        });
        let mut map = (*self.callbacks).clone();
        map.insert(server_name, wrapped);
        self.callbacks = Arc::new(map);
    }

    /// Check if a server is registered.
    pub fn has_server(&self, server_name: &str) -> bool {
        self.callbacks.contains_key(server_name)
    }

    /// Execute an MCP tool call from a full name like `mcp__serverName_toolName`.
    pub async fn execute(
        &self,
        full_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let (server_name, tool_name) = parse_mcp_tool_name(full_name)
            .ok_or_else(|| AgentError::Tool(format!("Invalid MCP tool name: {}", full_name)))?;

        let callback = self
            .callbacks
            .get(&server_name)
            .ok_or_else(|| AgentError::Tool(format!(
                "MCP server '{}' not registered. Use McpToolRegistry::register() to add MCP servers.",
                server_name
            )))?;

        callback(server_name, tool_name, arguments).await
    }
}

/// Create a tool executor closure for a specific MCP tool name.
///
/// Register with the QueryEngine:
/// ```ignore
/// let mut mcp_registry = McpToolRegistry::new();
/// mcp_registry.register("myServer".to_string(), |server, tool, args| async move {
///     // Connect to your MCP client and dispatch
///     let client = connect_to_mcp(&server).await?;
///     let result = client.call_tool(&tool, args).await?;
///     Ok(ToolResult { content: serde_json::to_string(&result)?, ..Default::default() })
/// });
///
/// // For each known MCP tool:
/// engine.register_tool(
///     "mcp__myServer_listFiles".to_string(),
///     create_named_mcp_executor(mcp_registry.clone(), "mcp__myServer_listFiles"),
/// );
/// ```
pub fn create_named_mcp_executor(
    registry: McpToolRegistry,
    full_name: &str,
) -> impl Fn(serde_json::Value, &ToolContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolResult, AgentError>> + Send>>
     + Clone
     + Send
     + Sync
     + 'static
{
    let name = full_name.to_string();
    move |input: serde_json::Value, _ctx: &ToolContext| {
        let registry = registry.clone();
        let name = name.clone();
        Box::pin(async move { registry.execute(&name, input).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_tool_name() {
        let (server, tool) = parse_mcp_tool_name("mcp__fs_readFile").unwrap();
        assert_eq!(server, "fs");
        assert_eq!(tool, "readFile");
    }

    #[test]
    fn test_parse_mcp_tool_name_no_prefix() {
        assert!(parse_mcp_tool_name("Bash").is_none());
    }

    #[test]
    fn test_parse_mcp_tool_name_no_tool_part() {
        assert!(parse_mcp_tool_name("mcp__server").is_none());
    }

    #[tokio::test]
    async fn test_mcp_registry_call_unregistered() {
        let registry = McpToolRegistry::new();
        let result = registry.execute("mcp__nonexistent_tool", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not registered"));
    }

    #[tokio::test]
    async fn test_mcp_registry_call_registered() {
        let mut registry = McpToolRegistry::new();
        registry.register(
            "test".to_string(),
            |_server, _tool, _args| async {
                Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: "hello from mcp".to_string(),
                    is_error: Some(false),
                    was_persisted: None,
                })
            },
        );
        let result = registry
            .execute("mcp__test_myTool", serde_json::json!({"key": "val"}))
            .await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.content.contains("hello from mcp"));
    }

    #[tokio::test]
    async fn test_create_named_mcp_executor() {
        let mut registry = McpToolRegistry::new();
        registry.register(
            "fs".to_string(),
            |_server, tool, _args| async move {
                Ok(ToolResult {
                    result_type: "text".to_string(),
                    tool_use_id: "".to_string(),
                    content: format!("result of {}", tool),
                    is_error: Some(false),
                    was_persisted: None,
                })
            },
        );
        let executor = create_named_mcp_executor(registry.clone(), "mcp__fs_readFile");
        let ctx = ToolContext::default();
        let result = executor(serde_json::json!({}), &ctx).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("result of readFile"));
    }
}
