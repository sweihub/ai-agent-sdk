// Source: ~/claudecode/openclaudecode/src/tools/ToolSearchTool/ToolSearchTool.ts
use crate::error::AgentError;
use crate::tools::config_tools::TOOL_SEARCH_TOOL_NAME;
use crate::tools::deferred_tools::{
    ToolSearchQuery, extract_discovered_tool_names, get_deferred_tool_names, is_deferred_tool,
    parse_tool_search_query, search_tools_with_keywords,
};
use crate::types::*;

/// ToolSearchTool result output
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSearchOutput {
    pub matches: Vec<String>,
    pub query: String,
    pub total_deferred_tools: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_mcp_servers: Option<Vec<String>>,
}

/// ToolSearchTool - discovers deferred tools via search
pub struct ToolSearchTool;

impl ToolSearchTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        TOOL_SEARCH_TOOL_NAME
    }

    pub fn description(&self) -> &str {
        "Fetches full schema definitions for deferred tools so they can be called. \
         Deferred tools appear by name in <available-deferred-tools> messages. \
         Until fetched, only the name is known — there is no parameter schema, so the tool cannot be invoked. \
         This tool takes a query, matches it against the deferred tool list, and returns the matched tools' \
         complete JSONSchema definitions inside a <functions> block. \
         Query forms: \
         - \"select:Read,Edit,Grep\" — fetch these exact tools by name \
         - \"notebook jupyter\" — keyword search, up to max_results best matches \
         - \"+slack send\" — require \"slack\" in the name, rank by remaining terms"
    }

    pub fn user_facing_name(&self, _input: Option<&serde_json::Value>) -> String {
        "ToolSearch".to_string()
    }

    pub fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        input.and_then(|inp| inp["query"].as_str().map(String::from))
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
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results to return (default: 5)"
                }
            }),
            required: Some(vec!["query".to_string()]),
        }
    }

    pub async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, AgentError> {
        let query = input["query"].as_str().unwrap_or("");
        let max_results = input["max_results"].as_u64().unwrap_or(5) as usize;

        // Get all base tools to identify deferred subset
        let all_tools = crate::tools::get_all_base_tools();
        let deferred_tools: Vec<&ToolDefinition> =
            all_tools.iter().filter(|t| is_deferred_tool(t)).collect();

        let total_deferred = deferred_tools.len();

        // Parse the query
        let parsed_query = parse_tool_search_query(query);

        let matches = match &parsed_query {
            ToolSearchQuery::Select(requested) => {
                // Direct tool selection
                let mut found = Vec::new();
                let mut missing = Vec::new();

                for tool_name in requested {
                    // Check deferred tools first, then all tools
                    if let Some(tool) = deferred_tools.iter().find(|t| t.name == *tool_name) {
                        if !found.contains(&tool.name) {
                            found.push(tool.name.clone());
                        }
                    } else if let Some(tool) = all_tools.iter().find(|t| t.name == *tool_name) {
                        // Tool is already loaded (not deferred) — still return it so model can proceed
                        if !found.contains(&tool.name) {
                            found.push(tool.name.clone());
                        }
                    } else {
                        missing.push(tool_name.clone());
                    }
                }

                if found.is_empty() {
                    log::debug!(
                        "ToolSearchTool: select failed — none found: {}",
                        missing.join(", ")
                    );
                } else if !missing.is_empty() {
                    log::debug!(
                        "ToolSearchTool: partial select — found: {}, missing: {}",
                        found.join(", "),
                        missing.join(", ")
                    );
                } else {
                    log::debug!("ToolSearchTool: selected {}", found.join(", "));
                }
                found
            }
            ToolSearchQuery::Keyword(q) => {
                let results = search_tools_with_keywords(q, &deferred_tools, max_results);
                log::debug!(
                    "ToolSearchTool: keyword search for \"{}\", found {} matches",
                    q,
                    results.len()
                );
                results
            }
            ToolSearchQuery::KeywordWithRequired { .. } => {
                let results = search_tools_with_keywords(query, &deferred_tools, max_results);
                log::debug!(
                    "ToolSearchTool: keyword search with required terms for \"{}\", found {} matches",
                    query,
                    results.len()
                );
                results
            }
        };

        // Build result
        // When matches exist, we return tool_reference blocks for API expansion.
        // When no matches, we return plain text.
        let output = ToolSearchOutput {
            matches: matches.clone(),
            query: query.to_string(),
            total_deferred_tools: total_deferred,
            pending_mcp_servers: None, // No MCP in Rust SDK yet
        };

        // Serialize to the structured content format
        let content_value = if matches.is_empty() {
            let deferred_names: Vec<&str> =
                deferred_tools.iter().map(|t| t.name.as_str()).collect();
            let names_str = deferred_names.join(", ");
            serde_json::json!({
                "type": "text",
                "text": format!("No matching deferred tools found for query: \"{}\". Available deferred tools: {}", query, names_str)
            })
        } else {
            // Return tool_reference blocks for API expansion
            serde_json::json!(
                matches
                    .iter()
                    .map(|name| {
                        serde_json::json!({
                            "type": "tool_reference",
                            "tool_name": name
                        })
                    })
                    .collect::<Vec<_>>()
            )
        };

        Ok(ToolResult {
            result_type: "text".to_string(),
            tool_use_id: "".to_string(),
            content: serde_json::to_string(&content_value).unwrap_or_default(),
            is_error: Some(false),
            was_persisted: None,
        })
    }

    /// Build a tool_result with tool_reference blocks for the API
    pub fn build_tool_reference_result(matches: &[String], tool_use_id: &str) -> serde_json::Value {
        if matches.is_empty() {
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": "No matching deferred tools found."
            })
        } else {
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": matches.iter().map(|name| {
                    serde_json::json!({
                        "type": "tool_reference",
                        "tool_name": name
                    })
                }).collect::<Vec<_>>()
            })
        }
    }
}

impl Default for ToolSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_search_tool_name() {
        let tool = ToolSearchTool::new();
        assert_eq!(tool.name(), TOOL_SEARCH_TOOL_NAME);
    }

    #[test]
    fn test_tool_search_tool_schema() {
        let tool = ToolSearchTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.is_some());
        assert!(
            schema
                .required
                .as_ref()
                .unwrap()
                .contains(&"query".to_string())
        );
    }

    #[test]
    fn test_build_tool_reference_result() {
        let result = ToolSearchTool::build_tool_reference_result(
            &["WebSearch".to_string(), "WebFetch".to_string()],
            "tool_123",
        );
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "tool_123");
        assert!(result["content"].is_array());
        assert_eq!(result["content"].as_array().unwrap().len(), 2);
        assert_eq!(result["content"][0]["type"], "tool_reference");
        assert_eq!(result["content"][0]["tool_name"], "WebSearch");
    }

    #[test]
    fn test_build_tool_reference_result_empty() {
        let result = ToolSearchTool::build_tool_reference_result(&[], "tool_123");
        assert_eq!(result["type"], "tool_result");
        assert!(result["content"].is_string());
    }

    #[test]
    fn test_extract_discovered_tool_names() {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "content": [
                    {"type": "tool_reference", "tool_name": "WebSearch"},
                    {"type": "tool_reference", "tool_name": "WebFetch"}
                ]
            }]
        })];
        let discovered = extract_discovered_tool_names(&messages);
        assert!(discovered.contains("WebSearch"));
        assert!(discovered.contains("WebFetch"));
    }
}
