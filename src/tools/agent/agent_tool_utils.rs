// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/agentToolUtils.ts
#![allow(dead_code)]
use std::sync::Arc;

use std::collections::{HashMap, HashSet};

use super::constants::{
    AGENT_TOOL_NAME, ALL_AGENT_DISALLOWED_TOOLS, ASYNC_AGENT_ALLOWED_TOOLS,
    CUSTOM_AGENT_DISALLOWED_TOOLS, FORK_BOILERPLATE_TAG, FORK_DIRECTIVE_PREFIX,
};
use super::load_agents_dir::AgentDefinition;

/// Resolved tools for an agent.
#[derive(Debug, Clone)]
pub struct ResolvedAgentTools {
    pub has_wildcard: bool,
    pub valid_tools: Vec<String>,
    pub invalid_tools: Vec<String>,
    pub resolved_tool_names: Vec<String>,
    pub allowed_agent_types: Option<Vec<String>>,
}

/// Filter tools available to an agent based on built-in status and async mode.
pub fn filter_tools_for_agent(
    available_tools: &[String],
    is_built_in: bool,
    is_async: bool,
) -> Vec<String> {
    available_tools
        .iter()
        .filter(|tool| {
            // Allow MCP tools for all agents
            if tool.starts_with("mcp__") {
                return true;
            }
            // Block globally disallowed tools
            if ALL_AGENT_DISALLOWED_TOOLS.contains(&tool.as_str()) {
                return false;
            }
            // Block custom-agent-specific tools for non-built-in agents
            if !is_built_in && CUSTOM_AGENT_DISALLOWED_TOOLS.contains(&tool.as_str()) {
                return false;
            }
            // Block async-restricted tools for async agents
            if is_async && !ASYNC_AGENT_ALLOWED_TOOLS.contains(&tool.as_str()) {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// Parse a tool spec string to extract the tool name and any permission pattern.
fn parse_tool_spec(spec: &str) -> (String, Option<String>) {
    if let Some(pos) = spec.find('(') {
        let tool_name = spec[..pos].trim().to_string();
        let rule_content = spec[pos..].trim().to_string();
        (tool_name, Some(rule_content))
    } else {
        (spec.trim().to_string(), None)
    }
}

/// Resolves and validates agent tools against available tools.
/// Handles wildcard expansion and validation.
pub fn resolve_agent_tools(
    agent_definition: &AgentDefinition,
    available_tools: &[String],
    is_async: bool,
) -> ResolvedAgentTools {
    // Filter available tools based on agent's built-in status and async mode
    let filtered_available = filter_tools_for_agent(
        available_tools,
        agent_definition.source == "built-in",
        is_async,
    );

    // Create a set of disallowed tool names
    let disallowed_set: HashSet<&str> = agent_definition
        .disallowed_tools
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Filter out disallowed tools
    let allowed_available: Vec<String> = filtered_available
        .into_iter()
        .filter(|t| !disallowed_set.contains(t.as_str()))
        .collect();

    // Check for wildcard
    let has_wildcard = agent_definition.tools.is_empty()
        || agent_definition.tools == vec!["*"]
        || (agent_definition.tools.len() == 1 && agent_definition.tools[0] == "*");

    if has_wildcard {
        return ResolvedAgentTools {
            has_wildcard: true,
            valid_tools: vec![],
            invalid_tools: vec![],
            resolved_tool_names: allowed_available,
            allowed_agent_types: None,
        };
    }

    let available_map: HashMap<&str, &String> =
        allowed_available.iter().map(|t| (t.as_str(), t)).collect();

    let mut valid_tools: Vec<String> = Vec::new();
    let mut invalid_tools: Vec<String> = Vec::new();
    let mut resolved: Vec<String> = Vec::new();
    let mut resolved_set: HashSet<String> = HashSet::new();
    let mut allowed_agent_types: Option<Vec<String>> = None;

    for tool_spec in &agent_definition.tools {
        let (tool_name, rule_content) = parse_tool_spec(tool_spec);

        // Special case: Agent tool carries allowedAgentTypes metadata
        if tool_name == AGENT_TOOL_NAME {
            if let Some(ref rules) = rule_content {
                // Parse comma-separated agent types: "worker, researcher" -> ["worker", "researcher"]
                let types: Vec<String> = rules
                    .trim_matches(|c: char| c == '(' || c == ')')
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                allowed_agent_types = Some(types);
            }
            valid_tools.push(tool_spec.clone());
            continue;
        }

        if available_map.contains_key(tool_name.as_str()) {
            valid_tools.push(tool_spec.clone());
            if resolved_set.insert(tool_name.clone()) {
                resolved.push(tool_name);
            }
        } else {
            invalid_tools.push(tool_spec.clone());
        }
    }

    ResolvedAgentTools {
        has_wildcard: false,
        valid_tools,
        invalid_tools,
        allowed_agent_types,
        resolved_tool_names: resolved,
    }
}

/// Count tool uses in a list of messages (represented as JSON values).
pub fn count_tool_uses(messages: &[serde_json::Value]) -> usize {
    let mut count = 0;
    for msg in messages {
        if msg.get("type").and_then(|t| t.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    for block in arr {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

/// Extract text content from a message's content array.
pub fn extract_text_content(content: &[serde_json::Value], separator: &str) -> String {
    let texts: Vec<String> = content
        .iter()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
        .map(|t| t.to_string())
        .collect();
    texts.join(separator)
}

/// Get the last assistant message from a list of messages.
pub fn get_last_assistant_message(messages: &[serde_json::Value]) -> Option<&serde_json::Value> {
    messages
        .iter()
        .rev()
        .find(|msg| msg.get("type").and_then(|t| t.as_str()) == Some("assistant"))
}

/// Extract a partial result string from an agent's accumulated messages.
/// Used when an async agent is killed to preserve what it accomplished.
pub fn extract_partial_result(messages: &[serde_json::Value]) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
            if let Some(arr) = content.as_array() {
                let text = extract_text_content(arr, "\n");
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Extract a partial result string from a QueryEngine's message history.
/// Used when a subagent is killed to preserve what it accomplished.
/// Matches TypeScript's extractPartialResult but operates on engine Message type.
pub fn extract_partial_result_from_engine(messages: &[crate::types::Message]) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.role != crate::types::MessageRole::Assistant {
            continue;
        }
        if !msg.content.is_empty() {
            return Some(msg.content.clone());
        }
    }
    None
}

/// Get the name of the last tool_use block in a message.
pub fn get_last_tool_use_name(message: &serde_json::Value) -> Option<String> {
    if message.get("type").and_then(|t| t.as_str()) != Some("assistant") {
        return None;
    }
    let content = message.get("message").and_then(|m| m.get("content"))?;
    let arr = content.as_array()?;
    for block in arr.iter().rev() {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            return block
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
        }
    }
    None
}

/// Token usage tracking for an agent run.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_creation_input_tokens: usize,
    pub cache_read_input_tokens: usize,
}

/// Result returned when an agent completes.
#[derive(Debug, Clone)]
pub struct AgentToolResult {
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub content: String,
    pub total_tool_use_count: usize,
    pub total_duration_ms: u64,
    pub total_tokens: usize,
    pub usage: TokenUsage,
}

/// Finalize an agent run and produce a result.
pub fn finalize_agent_tool(
    messages: &[serde_json::Value],
    agent_id: &str,
    agent_type: &str,
    start_time_ms: u64,
) -> Result<AgentToolResult, String> {
    let last_assistant = get_last_assistant_message(messages)
        .ok_or_else(|| "No assistant messages found".to_string())?;

    // Extract text content
    let content = last_assistant
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .map(|arr| extract_text_content(arr, "\n"))
        .unwrap_or_default();

    let total_tool_use_count = count_tool_uses(messages);

    // Extract usage from last assistant message
    let usage = last_assistant
        .get("message")
        .and_then(|m| m.get("usage"))
        .map(|u| TokenUsage {
            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            cache_creation_input_tokens: u
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            cache_read_input_tokens: u
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
        })
        .unwrap_or_default();

    let total_tokens = usage.input_tokens
        + usage.output_tokens
        + usage.cache_creation_input_tokens
        + usage.cache_read_input_tokens;

    Ok(AgentToolResult {
        agent_id: agent_id.to_string(),
        agent_type: Some(agent_type.to_string()),
        content,
        total_tool_use_count,
        total_duration_ms: (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64)
            .saturating_sub(start_time_ms),
        total_tokens,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent_def(tools: Vec<&str>) -> AgentDefinition {
        AgentDefinition {
            agent_type: "test".to_string(),
            when_to_use: "test".to_string(),
            tools: tools.into_iter().map(|s| s.to_string()).collect(),
            source: "built-in".to_string(),
            base_dir: "built-in".to_string(),
            get_system_prompt: Arc::new(|| String::new()),
            model: None,
            disallowed_tools: vec![],
            max_turns: None,
            permission_mode: None,
            effort: None,
            color: None,
            mcp_servers: vec![],
            hooks: None,
            skills: vec![],
            background: false,
            initial_prompt: None,
            memory: None,
            isolation: None,
            required_mcp_servers: vec![],
            omit_claude_md: false,
            critical_system_reminder_experimental: None,
        }
    }

    #[test]
    fn test_resolve_wildcard() {
        let agent = make_agent_def(vec!["*"]);
        let available = vec!["Bash".to_string(), "Read".to_string()];
        let resolved = resolve_agent_tools(&agent, &available, false);
        assert!(resolved.has_wildcard);
        assert_eq!(resolved.resolved_tool_names.len(), 2);
    }

    #[test]
    fn test_resolve_specific_tools() {
        let agent = make_agent_def(vec!["Bash"]);
        let available = vec!["Bash".to_string(), "Read".to_string()];
        let resolved = resolve_agent_tools(&agent, &available, false);
        assert!(!resolved.has_wildcard);
        assert_eq!(resolved.resolved_tool_names, vec!["Bash"]);
    }

    #[test]
    fn test_extract_text_content() {
        let content = vec![
            serde_json::json!({"type": "text", "text": "hello"}),
            serde_json::json!({"type": "tool_use", "name": "Bash"}),
            serde_json::json!({"type": "text", "text": "world"}),
        ];
        assert_eq!(extract_text_content(&content, " "), "hello world");
    }

    #[test]
    fn test_count_tool_uses() {
        let messages = vec![serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "tool_use", "id": "1", "name": "Bash"},
                    {"type": "tool_use", "id": "2", "name": "Read"},
                ]
            }
        })];
        assert_eq!(count_tool_uses(&messages), 2);
    }
}
