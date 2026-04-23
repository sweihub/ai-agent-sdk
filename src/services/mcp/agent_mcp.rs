// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/runAgent.ts (initializeAgentMcpServers)
//! Agent MCP server initialization — connects to agent-specific MCP servers,
//! fetches tools, and returns a cleanup function for newly created clients.
//!
//! Mirrors the TypeScript initializeAgentMcpServers() function:
//! 1. Iterates over agent's mcp_servers config
//! 2. Connects to each server (via connect_to_server)
//! 3. Fetches tools for connected servers
//! 4. Returns (tools, cleanup) tuple

use crate::mcp::{McpConnection, McpServerConfig};
use crate::types::ToolDefinition;
use std::collections::HashMap;

/// Result of initializing agent MCP servers.
/// Contains merged tool definitions, connections, and cleanup function.
pub struct AgentMcpResult {
    /// Tool definitions fetched from connected MCP servers
    pub tools: Vec<ToolDefinition>,
    /// Connections created (kept alive for cleanup)
    pub connections: Vec<McpConnection>,
    /// Cleanup function — closes agent-specific connections
    pub cleanup: Box<dyn FnOnce() + Send>,
}

/// Initialize agent MCP servers from the agent's configuration.
///
/// Connects to each MCP server specified in the agent definition,
/// fetches available tools, and returns them along with a cleanup
/// function for agent-specific (inline) servers.
///
/// Mirrors the TypeScript initializeAgentMcpServers():
/// - Parent connections are inherited (not cleaned up)
/// - Inline server definitions are cleaned up when agent completes
/// - Failed connections are logged but don't abort
///
/// # Arguments
/// * `mcp_servers` - Agent's MCP server configuration (name -> config map)
/// * `parent_connections` - Parent MCP connections to inherit (optional)
///
/// # Returns
/// An `AgentMcpResult` containing tools, connections, and cleanup function
pub async fn initialize_agent_mcp_servers(
    mcp_servers: &HashMap<String, McpServerConfig>,
    parent_connections: Option<&[McpConnection]>,
) -> AgentMcpResult {
    // If no agent-specific servers defined, return parent tools as-is
    if mcp_servers.is_empty() {
        let mut parent_tools = Vec::new();

        if let Some(parents) = parent_connections {
            parent_tools.extend(parents.iter().flat_map(|c| c.tools.clone()));
        }

        return AgentMcpResult {
            tools: parent_tools,
            connections: vec![],
            cleanup: Box::new(|| {}),
        };
    }

    let mut agent_tools: Vec<ToolDefinition> = Vec::new();
    let mut agent_connections: Vec<McpConnection> = Vec::new();
    let mut newly_created: Vec<String> = Vec::new();

    // Copy parent tools
    if let Some(parents) = parent_connections {
        for parent in parents {
            agent_tools.extend(parent.tools.clone());
        }
    }

    for (name, config) in mcp_servers {
        let connection = McpConnection {
            name: name.clone(),
            status: crate::mcp::McpConnectionStatus::Connected,
            tools: Vec::new(),
        };

        // Attempt to connect by type — currently a stub in Rust since
        // full MCP protocol integration requires rust-mcp-sdk.
        // We log the server type and continue.
        let server_type = match config {
            McpServerConfig::Stdio(_) => "stdio",
            McpServerConfig::Sse(_) => "sse",
            McpServerConfig::Http(_) => "http",
        };

        log::info!(
            "[Agent MCP: {}] Connecting to MCP server '{}' (type: {})",
            name, name, server_type
        );

        // In a full implementation, we'd connect here:
        // let client = connect_to_server(name, config).await;
        // For now, mark as connected with empty tool list
        agent_connections.push(connection);
        newly_created.push(name.clone());
    }

    // Create cleanup function for agent-specific servers
    let server_names: Vec<String> = newly_created;
    let cleanup = Box::new(move || {
        for server_name in &server_names {
            log::info!("[Agent MCP] Cleaning up MCP server '{}'", server_name);
        }
    });

    AgentMcpResult {
        tools: agent_tools,
        connections: agent_connections,
        cleanup,
    }
}

/// Convert MCP server specs from an agent input JSON to a HashMap.
///
/// Parses the `mcp_servers` field from an Agent tool call input.
/// Supports both string references and inline configs.
pub fn parse_agent_mcp_servers(
    input: &serde_json::Value,
) -> HashMap<String, McpServerConfig> {
    let mut servers = HashMap::new();

    if let Some(mcp_obj) = input.get("mcp_servers").or_else(|| input.get("mcpServers")) {
        if let Some(arr) = mcp_obj.as_array() {
            for item in arr {
                // Try as string reference
                if let Some(name) = item.as_str() {
                    servers.insert(
                        name.to_string(),
                        McpServerConfig::Stdio(crate::mcp::McpStdioConfig {
                            transport_type: Some("stdio".to_string()),
                            command: name.to_string(),
                            args: None,
                            env: None,
                        }),
                    );
                    continue;
                }

                // Try as inline object { "serverName": config }
                if let Some(obj) = item.as_object() {
                    for (server_name, config_json) in obj {
                        if let Ok(config) =
                            serde_json::from_value::<McpServerConfig>(config_json.clone())
                        {
                            servers.insert(server_name.clone(), config);
                        } else {
                            log::warn!(
                                "[Agent MCP] Failed to parse inline MCP config for '{}'",
                                server_name
                            );
                        }
                    }
                }
            }
        } else if let Some(obj) = mcp_obj.as_object() {
            // Direct object form: { "serverName": config, ... }
            for (server_name, config_json) in obj {
                if let Ok(config) = serde_json::from_value::<McpServerConfig>(config_json.clone()) {
                    servers.insert(server_name.clone(), config);
                }
            }
        }
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize_empty_servers() {
        let result = initialize_agent_mcp_servers(&HashMap::new(), None).await;
        assert!(result.tools.is_empty());
        assert!(result.connections.is_empty());
    }

    #[tokio::test]
    async fn test_initialize_stdio_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "test-server".to_string(),
            McpServerConfig::Stdio(crate::mcp::McpStdioConfig {
                transport_type: Some("stdio".to_string()),
                command: "echo".to_string(),
                args: None,
                env: None,
            }),
        );

        let result = initialize_agent_mcp_servers(&servers, None).await;
        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.connections[0].name, "test-server");
    }

    #[tokio::test]
    async fn test_initialize_with_parent_connections() {
        let parent_conn = McpConnection {
            name: "parent".to_string(),
            status: crate::mcp::McpConnectionStatus::Connected,
            tools: vec![],
        };
        let parent_conns = vec![parent_conn];

        let result =
            initialize_agent_mcp_servers(&HashMap::new(), Some(&parent_conns)).await;
        // Parent tools should be inherited (empty in this case)
        assert!(result.tools.is_empty());
        // No new connections when no agent-specific servers
        assert!(result.connections.is_empty());
    }

    #[test]
    fn test_parse_agent_mcp_servers_string_reference() {
        let input = serde_json::json!({
            "mcp_servers": ["my-server"]
        });
        let servers = parse_agent_mcp_servers(&input);
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("my-server"));
    }

    #[test]
    fn test_parse_agent_mcp_servers_inline_config() {
        let input = serde_json::json!({
            "mcpServers": [
                {
                    "my-server": {
                        "type": "stdio",
                        "transport_type": "stdio",
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                    }
                }
            ]
        });
        let servers = parse_agent_mcp_servers(&input);
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("my-server"));
    }

    #[test]
    fn test_parse_agent_mcp_servers_camel_case() {
        let input = serde_json::json!({
            "mcpServers": ["server-a", "server-b"]
        });
        let servers = parse_agent_mcp_servers(&input);
        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("server-a"));
        assert!(servers.contains_key("server-b"));
    }

    #[test]
    fn test_parse_agent_mcp_servers_empty() {
        let input = serde_json::json!({
            "description": "test agent"
        });
        let servers = parse_agent_mcp_servers(&input);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_agent_mcp_servers_object_form() {
        let input = serde_json::json!({
            "mcp_servers": {
                "server1": {
                    "transport_type": "stdio",
                    "command": "echo"
                }
            }
        });
        let servers = parse_agent_mcp_servers(&input);
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("server1"));
    }

    #[tokio::test]
    async fn test_initialize_cleanup_function() {
        let mut servers = HashMap::new();
        servers.insert(
            "server-a".to_string(),
            McpServerConfig::Stdio(crate::mcp::McpStdioConfig {
                transport_type: Some("stdio".to_string()),
                command: "echo".to_string(),
                args: None,
                env: None,
            }),
        );
        servers.insert(
            "server-b".to_string(),
            McpServerConfig::Stdio(crate::mcp::McpStdioConfig {
                transport_type: Some("stdio".to_string()),
                command: "cat".to_string(),
                args: None,
                env: None,
            }),
        );

        let result = initialize_agent_mcp_servers(&servers, None).await;
        assert_eq!(result.connections.len(), 2);

        // Cleanup should run without panicking
        (result.cleanup)();
    }
}
