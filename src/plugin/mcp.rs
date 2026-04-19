// Source: /data/home/swei/claudecode/openclaudecode/src/entrypoints/mcp.ts
//! Plugin MCP server implementation - ported from ~/claudecode/openclaudecode/src/utils/plugins/mcpPluginIntegration.ts
//!
//! This module provides MCP server management for plugins, including:
//! - Loading MCP server configs from plugin manifests
//! - Server lifecycle management (start, stop, status)
//! - Support for stdio and SSE transport types

use crate::error::AgentError;
use crate::mcp::McpConnection;
use crate::types::{McpConnectionStatus, McpServerConfig, McpSseConfig, McpStdioConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

/// Transport type for MCP server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginMcpTransport {
    Stdio,
    Sse,
    Http,
    #[serde(other)]
    Unknown,
}

impl Default for PluginMcpTransport {
    fn default() -> Self {
        PluginMcpTransport::Stdio
    }
}

/// Status of a plugin MCP server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginMcpServerStatus {
    /// Server is not started
    Stopped,
    /// Server is starting up
    Starting,
    /// Server is running and connected
    Running,
    /// Server encountered an error
    Error,
    /// Server is disabled
    Disabled,
}

/// Plugin MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMcpServerConfig {
    /// Transport type (stdio, sse, http)
    pub transport_type: Option<PluginMcpTransport>,
    /// Server command (for stdio)
    pub command: Option<String>,
    /// Server arguments (for stdio)
    pub args: Option<Vec<String>>,
    /// Environment variables (for stdio)
    pub env: Option<HashMap<String, String>>,
    /// Server URL (for sse/http)
    pub url: Option<String>,
    /// HTTP headers (for sse/http)
    pub headers: Option<HashMap<String, String>>,
    /// Scope: local, user, project, dynamic, enterprise
    pub scope: Option<String>,
    /// Plugin source identifier
    pub plugin_source: Option<String>,
}

impl PluginMcpServerConfig {
    /// Convert to standard McpServerConfig
    pub fn to_mcp_config(&self) -> Option<McpServerConfig> {
        let transport = self
            .transport_type
            .as_ref()
            .unwrap_or(&PluginMcpTransport::Stdio);

        match transport {
            PluginMcpTransport::Stdio => {
                let command = self.command.as_ref()?;
                Some(McpServerConfig::Stdio(McpStdioConfig {
                    transport_type: Some("stdio".to_string()),
                    command: command.clone(),
                    args: self.args.clone(),
                    env: self.env.clone(),
                }))
            }
            PluginMcpTransport::Sse => {
                let url = self.url.as_ref()?;
                Some(McpServerConfig::Sse(McpSseConfig {
                    transport_type: "sse".to_string(),
                    url: url.clone(),
                    headers: self.headers.clone(),
                }))
            }
            PluginMcpTransport::Http => {
                let url = self.url.as_ref()?;
                Some(McpServerConfig::Http(crate::types::McpHttpConfig {
                    transport_type: "http".to_string(),
                    url: url.clone(),
                    headers: self.headers.clone(),
                }))
            }
            PluginMcpTransport::Unknown => None,
        }
    }
}

/// Plugin MCP server instance
#[derive(Debug)]
pub struct PluginMcpServer {
    /// Server name
    pub name: String,
    /// Server configuration
    pub config: PluginMcpServerConfig,
    /// Current status
    pub status: PluginMcpServerStatus,
    /// Child process handle (for stdio servers)
    child: Option<tokio::process::Child>,
    /// MCP connection if running
    connection: Option<McpConnection>,
    /// Plugin path for resolving relative paths
    plugin_path: String,
    /// Plugin source identifier
    _plugin_source: String,
}

impl PluginMcpServer {
    /// Create a new plugin MCP server
    pub fn new(
        name: String,
        config: PluginMcpServerConfig,
        plugin_path: String,
        plugin_source: String,
    ) -> Self {
        Self {
            name,
            config,
            status: PluginMcpServerStatus::Stopped,
            child: None,
            connection: None,
            plugin_path,
            _plugin_source: plugin_source,
        }
    }

    /// Start the MCP server
    pub async fn start(&mut self) -> Result<(), AgentError> {
        if self.status == PluginMcpServerStatus::Running {
            return Ok(());
        }

        self.status = PluginMcpServerStatus::Starting;

        let mcp_config = self.config.to_mcp_config().ok_or_else(|| {
            AgentError::Mcp(format!("Invalid MCP config for server {}", self.name))
        })?;

        // Resolve environment variables including plugin-specific ones
        let resolved_config = self.resolve_environment(&mcp_config);

        match resolved_config {
            McpServerConfig::Stdio(stdio_config) => {
                self.start_stdio(stdio_config).await?;
            }
            McpServerConfig::Sse(sse_config) => {
                self.start_sse(sse_config).await?;
            }
            McpServerConfig::Http(http_config) => {
                self.start_http(http_config).await?;
            }
        }

        self.status = PluginMcpServerStatus::Running;
        Ok(())
    }

    /// Start stdio-based MCP server
    async fn start_stdio(&mut self, config: McpStdioConfig) -> Result<(), AgentError> {
        let mut env_vars: HashMap<String, String> = std::env::vars().collect();

        // Add plugin-specific environment variables
        env_vars.insert("AI_PLUGIN_ROOT".to_string(), self.plugin_path.clone());

        // Add custom env vars
        if let Some(custom_env) = &config.env {
            for (key, value) in custom_env {
                env_vars.insert(key.clone(), value.clone());
            }
        }

        let command = config.command.clone();
        let args = config.args.unwrap_or_default();

        let mut child = Command::new(&command)
            .args(&args)
            .envs(&env_vars)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AgentError::Mcp(format!("Failed to spawn MCP server '{}': {}", command, e))
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Mcp("Failed to take stdout from MCP server".to_string()))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::Mcp("Failed to take stdin from MCP server".to_string()))?;

        let mut stdout_reader = BufReader::new(stdout).lines();

        // Send initialize request
        let initialize_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": format!("agent-sdk-plugin-{}", self.name),
                    "version": "1.0.0"
                }
            }
        });

        stdin
            .write_all(format!("{initialize_request}\n").as_bytes())
            .await
            .map_err(|e| {
                AgentError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
        stdin.flush().await.map_err(|e| {
            AgentError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Read initialize response
        let _ = stdout_reader.next_line().await;

        // Send tools/list request
        let list_tools_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        stdin
            .write_all(format!("{list_tools_request}\n").as_bytes())
            .await
            .map_err(|e| {
                AgentError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
        stdin.flush().await.map_err(|e| {
            AgentError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Read tools/list response
        let mut tools = vec![];
        if let Ok(Some(response)) = stdout_reader.next_line().await {
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&response) {
                if let Some(result) = resp.get("result") {
                    if let Some(tools_array) = result.get("tools").and_then(|t| t.as_array()) {
                        for tool_val in tools_array {
                            if let Ok(mcp_tool) =
                                serde_json::from_value::<crate::types::McpTool>(tool_val.clone())
                            {
                                let tool_def = create_mcp_tool_definition(&self.name, &mcp_tool);
                                tools.push(tool_def);
                            }
                        }
                    }
                }
            }
        }

        // Drop stdin to signal EOF, but keep process running
        drop(stdin);

        self.child = Some(child);
        self.connection = Some(McpConnection {
            name: self.name.clone(),
            status: McpConnectionStatus::Connected,
            tools,
        });

        Ok(())
    }

    /// Start SSE-based MCP server
    async fn start_sse(&mut self, _config: McpSseConfig) -> Result<(), AgentError> {
        // SSE support would require the SSE client implementation
        // For now, mark as running with placeholder connection
        self.connection = Some(McpConnection {
            name: self.name.clone(),
            status: McpConnectionStatus::Connected,
            tools: vec![],
        });
        Ok(())
    }

    /// Start HTTP-based MCP server
    async fn start_http(&mut self, _config: crate::types::McpHttpConfig) -> Result<(), AgentError> {
        // HTTP support would require the HTTP client implementation
        // For now, mark as running with placeholder connection
        self.connection = Some(McpConnection {
            name: self.name.clone(),
            status: McpConnectionStatus::Connected,
            tools: vec![],
        });
        Ok(())
    }

    /// Stop the MCP server
    pub async fn stop(&mut self) -> Result<(), AgentError> {
        if self.status == PluginMcpServerStatus::Stopped {
            return Ok(());
        }

        // Drop the connection
        if let Some(mut conn) = self.connection.take() {
            conn.close().await;
        }

        // Kill the child process if any
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }

        self.status = PluginMcpServerStatus::Stopped;
        Ok(())
    }

    /// Check if the server is running
    pub fn is_running(&self) -> bool {
        self.status == PluginMcpServerStatus::Running
    }

    /// Get the server status
    pub fn get_status(&self) -> &PluginMcpServerStatus {
        &self.status
    }

    /// Get the MCP connection
    pub fn get_connection(&self) -> Option<&McpConnection> {
        self.connection.as_ref()
    }

    /// Resolve environment variables in config
    fn resolve_environment(&self, config: &McpServerConfig) -> McpServerConfig {
        match config {
            McpServerConfig::Stdio(stdio_config) => {
                let mut resolved_env = std::env::vars().collect::<HashMap<_, _>>();

                // Add plugin-specific env vars
                resolved_env.insert("AI_PLUGIN_ROOT".to_string(), self.plugin_path.clone());

                if let Some(custom_env) = &stdio_config.env {
                    for (key, value) in custom_env {
                        let resolved = self.substitute_variables(value);
                        resolved_env.insert(key.clone(), resolved);
                    }
                }

                McpServerConfig::Stdio(McpStdioConfig {
                    transport_type: stdio_config.transport_type.clone(),
                    command: self.substitute_variables(&stdio_config.command),
                    args: stdio_config
                        .args
                        .as_ref()
                        .map(|args| args.iter().map(|a| self.substitute_variables(a)).collect()),
                    env: Some(resolved_env),
                })
            }
            McpServerConfig::Sse(sse_config) => {
                let resolved_url = self.substitute_variables(&sse_config.url);
                let resolved_headers = sse_config.headers.as_ref().map(|headers| {
                    headers
                        .iter()
                        .map(|(k, v)| (k.clone(), self.substitute_variables(v)))
                        .collect()
                });

                McpServerConfig::Sse(McpSseConfig {
                    transport_type: sse_config.transport_type.clone(),
                    url: resolved_url,
                    headers: resolved_headers,
                })
            }
            McpServerConfig::Http(http_config) => {
                let resolved_url = self.substitute_variables(&http_config.url);
                let resolved_headers = http_config.headers.as_ref().map(|headers| {
                    headers
                        .iter()
                        .map(|(k, v)| (k.clone(), self.substitute_variables(v)))
                        .collect()
                });

                McpServerConfig::Http(crate::types::McpHttpConfig {
                    transport_type: http_config.transport_type.clone(),
                    url: resolved_url,
                    headers: resolved_headers,
                })
            }
        }
    }

    /// Substitute variables in a string
    fn substitute_variables(&self, value: &str) -> String {
        let mut result = value.to_string();

        // Substitute AI_PLUGIN_ROOT
        result = result.replace("${AI_PLUGIN_ROOT}", &self.plugin_path);
        result = result.replace("$AI_PLUGIN_ROOT", &self.plugin_path);

        // Substitute environment variables
        for (key, val) in std::env::vars() {
            let pattern = format!("${{{}}}", key);
            let pattern_dollar = format!("${}", key);
            result = result.replace(&pattern, &val);
            result = result.replace(&pattern_dollar, &val);
        }

        result
    }
}

/// Create a ToolDefinition from an MCP tool
fn create_mcp_tool_definition(
    server_name: &str,
    mcp_tool: &crate::types::McpTool,
) -> crate::types::ToolDefinition {
    let tool_name = format!("mcp__{}__{}", server_name, mcp_tool.name);

    let input_schema = mcp_tool.input_schema.clone().unwrap_or_else(|| {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    });

    crate::types::ToolDefinition {
        name: tool_name,
        description: mcp_tool
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool: {}", mcp_tool.name)),
        input_schema: crate::types::ToolInputSchema {
            schema_type: input_schema
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("object")
                .to_string(),
            properties: input_schema
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::json!({})),
            required: input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                }),
        },
        annotations: None,
        should_defer: None,
        always_load: None,
        is_mcp: None,
        search_hint: None,
        aliases: None,
    }
}

/// Plugin MCP server manager
pub struct PluginMcpServerManager {
    /// Active servers
    servers: RwLock<HashMap<String, Arc<RwLock<PluginMcpServer>>>>,
}

impl Default for PluginMcpServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginMcpServerManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
        }
    }

    /// Add a server to the manager
    pub async fn add_server(&self, server: PluginMcpServer) {
        let name = server.name.clone();
        let server = Arc::new(RwLock::new(server));
        self.servers.write().await.insert(name, server);
    }

    /// Get a server by name
    pub async fn get_server(&self, name: &str) -> Option<Arc<RwLock<PluginMcpServer>>> {
        self.servers.read().await.get(name).cloned()
    }

    /// Remove a server by name
    pub async fn remove_server(&self, name: &str) {
        if let Some(server) = self.servers.write().await.remove(name) {
            let mut server = server.write().await;
            let _ = server.stop().await;
        }
    }

    /// Start a server by name
    pub async fn start_server(&self, name: &str) -> Result<(), AgentError> {
        if let Some(server) = self.servers.read().await.get(name) {
            let mut server = server.write().await;
            server.start().await
        } else {
            Err(AgentError::Mcp(format!("Server '{}' not found", name)))
        }
    }

    /// Stop a server by name
    pub async fn stop_server(&self, name: &str) -> Result<(), AgentError> {
        if let Some(server) = self.servers.read().await.get(name) {
            let mut server = server.write().await;
            server.stop().await
        } else {
            Err(AgentError::Mcp(format!("Server '{}' not found", name)))
        }
    }

    /// Start all servers
    pub async fn start_all(&self) -> Vec<(String, Result<(), AgentError>)> {
        let mut results = Vec::new();
        let servers = self.servers.read().await;

        for (name, server) in servers.iter() {
            let mut server = server.write().await;
            results.push((name.clone(), server.start().await));
        }

        results
    }

    /// Stop all servers
    pub async fn stop_all(&self) {
        let mut servers = self.servers.write().await;

        for (_, server) in servers.iter() {
            let mut server = server.write().await;
            let _ = server.stop().await;
        }

        servers.clear();
    }

    /// Get all server names
    pub async fn list_servers(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    /// Get status of all servers
    pub async fn get_all_status(&self) -> HashMap<String, PluginMcpServerStatus> {
        let servers = self.servers.read().await;
        let mut result = HashMap::new();

        for (name, server) in servers.iter() {
            let status = server.read().await.status.clone();
            result.insert(name.clone(), status);
        }

        result
    }
}

/// Load MCP server configs from a JSON file in the plugin directory
pub async fn load_mcp_servers_from_file(
    plugin_path: &str,
    filename: &str,
) -> Result<HashMap<String, PluginMcpServerConfig>, AgentError> {
    let path = Path::new(plugin_path).join(filename);

    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
        AgentError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read MCP config from {}: {}", path.display(), e),
        ))
    })?;

    let parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AgentError::Mcp(format!("Failed to parse MCP config: {}", e)))?;

    // Support both { mcpServers: {...} } and direct {...} formats
    let mcp_servers = if let Some(servers) = parsed.get("mcpServers") {
        servers.clone()
    } else {
        parsed
    };

    let mut configs = HashMap::new();

    if let Some(obj) = mcp_servers.as_object() {
        for (name, config_val) in obj {
            let config = parse_mcp_server_config(config_val);
            if config.is_some() {
                configs.insert(name.clone(), config.unwrap());
            }
        }
    }

    Ok(configs)
}

/// Parse a single MCP server config from JSON value
fn parse_mcp_server_config(value: &serde_json::Value) -> Option<PluginMcpServerConfig> {
    let obj = value.as_object()?;

    // Determine transport type
    let transport_type = obj
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| match t {
            "stdio" => PluginMcpTransport::Stdio,
            "sse" => PluginMcpTransport::Sse,
            "http" => PluginMcpTransport::Http,
            _ => PluginMcpTransport::Unknown,
        })
        .unwrap_or(PluginMcpTransport::Stdio);

    // Extract stdio fields
    let command = obj
        .get("command")
        .and_then(|v| v.as_str())
        .map(String::from);
    let args = obj.get("args").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect()
    });

    let env = obj.get("env").and_then(|v| v.as_object()).map(|obj| {
        obj.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    });

    // Extract SSE/HTTP fields
    let url = obj.get("url").and_then(|v| v.as_str()).map(String::from);
    let headers = obj.get("headers").and_then(|v| v.as_object()).map(|obj| {
        obj.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    });

    Some(PluginMcpServerConfig {
        transport_type: Some(transport_type),
        command,
        args,
        env,
        url,
        headers,
        scope: None,
        plugin_source: None,
    })
}

/// Load MCP servers from plugin manifest mcpServers field
pub async fn load_plugin_mcp_servers(
    plugin_path: &str,
    mcp_servers_spec: &serde_json::Value,
) -> Result<HashMap<String, PluginMcpServerConfig>, AgentError> {
    let mut servers = HashMap::new();

    match mcp_servers_spec {
        // Single string - path to JSON file or MCPB file
        serde_json::Value::String(path) => {
            if path.ends_with(".mcpb") {
                // MCPB files would need special handling - download and extract
                // For now, skip MCPB files
                eprintln!("MCPB file loading not yet implemented: {}", path);
            } else {
                // Path to JSON file
                let loaded = load_mcp_servers_from_file(plugin_path, path).await?;
                servers.extend(loaded);
            }
        }
        // Array of paths or inline configs
        serde_json::Value::Array(arr) => {
            for spec in arr {
                match spec {
                    serde_json::Value::String(path) => {
                        if path.ends_with(".mcpb") {
                            eprintln!("MCPB file loading not yet implemented: {}", path);
                        } else {
                            let loaded = load_mcp_servers_from_file(plugin_path, path).await?;
                            servers.extend(loaded);
                        }
                    }
                    _ => {
                        // Inline config
                        if let Some(config) = parse_mcp_server_config(spec) {
                            // Generate a name if not provided
                            let name = format!("inline_{}", servers.len());
                            servers.insert(name, config);
                        }
                    }
                }
            }
        }
        // Inline object config
        serde_json::Value::Object(_) => {
            if let Some(config) = parse_mcp_server_config(mcp_servers_spec) {
                let name = format!("inline_{}", servers.len());
                servers.insert(name, config);
            }
        }
        _ => {}
    }

    Ok(servers)
}

/// Add plugin scope to MCP server configs (prefix server names)
pub fn add_plugin_scope_to_servers(
    servers: HashMap<String, PluginMcpServerConfig>,
    plugin_name: &str,
    plugin_source: &str,
) -> HashMap<String, PluginMcpServerConfig> {
    servers
        .into_iter()
        .map(|(name, mut config)| {
            let scoped_name = format!("plugin:{}:{}", plugin_name, name);
            config.plugin_source = Some(plugin_source.to_string());
            (scoped_name, config)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_parsing() {
        let json = serde_json::json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "some-server"]
        });

        let config = parse_mcp_server_config(&json).unwrap();
        assert_eq!(config.transport_type, Some(PluginMcpTransport::Stdio));
        assert_eq!(config.command, Some("npx".to_string()));
    }

    #[test]
    fn test_sse_config_parsing() {
        let json = serde_json::json!({
            "type": "sse",
            "url": "http://localhost:3000/sse"
        });

        let config = parse_mcp_server_config(&json).unwrap();
        assert_eq!(config.transport_type, Some(PluginMcpTransport::Sse));
        assert_eq!(config.url, Some("http://localhost:3000/sse".to_string()));
    }

    #[test]
    fn test_server_status() {
        let server = PluginMcpServer::new(
            "test".to_string(),
            PluginMcpServerConfig {
                transport_type: Some(PluginMcpTransport::Stdio),
                command: Some("echo".to_string()),
                args: None,
                env: None,
                url: None,
                headers: None,
                scope: None,
                plugin_source: None,
            },
            "/tmp/plugin".to_string(),
            "test-plugin".to_string(),
        );

        assert_eq!(server.get_status(), &PluginMcpServerStatus::Stopped);
        assert!(!server.is_running());
    }

    #[test]
    fn test_manager() {
        let manager = PluginMcpServerManager::new();

        let server = PluginMcpServer::new(
            "test".to_string(),
            PluginMcpServerConfig {
                transport_type: Some(PluginMcpTransport::Stdio),
                command: Some("echo".to_string()),
                args: None,
                env: None,
                url: None,
                headers: None,
                scope: None,
                plugin_source: None,
            },
            "/tmp/plugin".to_string(),
            "test-plugin".to_string(),
        );

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            manager.add_server(server).await;
            let servers = manager.list_servers().await;
            assert_eq!(servers.len(), 1);
            assert!(servers.contains(&"test".to_string()));
        });
    }
}
