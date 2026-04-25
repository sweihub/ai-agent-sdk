// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/runAgent.ts
#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs;

use super::agent_tool_utils::{
    AgentToolResult, extract_partial_result, finalize_agent_tool, resolve_agent_tools,
};
use super::load_agents_dir::AgentDefinition;

/// Context for tool execution passed to the agent.
pub struct ToolContext {
    pub available_tools: Vec<String>,
    pub mcp_clients: Vec<String>,
    pub commands: Vec<(String, String)>, // (name, description)
    pub agent_definitions: Vec<AgentDefinition>,
    pub main_loop_model: String,
    pub custom_system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub tool_use_id: Option<String>,
}

/// Agent configuration overrides.
pub struct AgentOverrides {
    pub user_context: Option<HashMap<String, String>>,
    pub system_context: Option<HashMap<String, String>>,
    pub system_prompt: Option<String>,
    pub agent_id: Option<String>,
}

/// Result from running an agent.
pub struct RunAgentResult {
    pub messages: Vec<serde_json::Value>,
    pub result: AgentToolResult,
}

/// Resolve the model for the agent.
pub fn resolve_agent_model(
    agent_model: Option<&str>,
    main_loop_model: &str,
    override_model: Option<&str>,
) -> String {
    if let Some(m) = override_model {
        return m.to_string();
    }
    if let Some(m) = agent_model {
        if m == "inherit" {
            return main_loop_model.to_string();
        }
        return m.to_string();
    }
    main_loop_model.to_string()
}

/// Filter out assistant messages with incomplete tool calls (tool uses without results).
pub fn filter_incomplete_tool_calls(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    // Build a set of tool use IDs that have results
    let mut tool_use_ids_with_results = std::collections::HashSet::new();

    for message in messages {
        if message.get("type").and_then(|t| t.as_str()) == Some("user") {
            if let Some(content) = message.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    for block in arr {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                            if let Some(id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                                tool_use_ids_with_results.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Filter out assistant messages that contain tool uses without results
    messages
        .iter()
        .filter(|message| {
            if message.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                return true;
            }
            if let Some(content) = message.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    let has_incomplete = arr.iter().any(|block| {
                        block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                            && block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .is_some_and(|id| !tool_use_ids_with_results.contains(id))
                    });
                    return !has_incomplete;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Parameters for running an agent.
pub struct RunAgentParams {
    pub agent_definition: AgentDefinition,
    pub prompt_messages: Vec<serde_json::Value>,
    pub tool_context: ToolContext,
    pub is_async: bool,
    pub override_params: Option<AgentOverrides>,
    pub model: Option<String>,
    pub max_turns: Option<usize>,
    pub fork_context_messages: Option<Vec<serde_json::Value>>,
    pub allowed_tools: Option<Vec<String>>,
    pub worktree_path: Option<String>,
    pub description: Option<String>,
}

/// Run an agent synchronously and return the result.
/// In a full implementation, this would use an async generator pattern to yield messages.
/// Here we provide a simplified sync interface that runs the agent and returns the result.
pub async fn run_agent(params: RunAgentParams) -> Result<RunAgentResult, String> {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let resolved_model = resolve_agent_model(
        params.agent_definition.model.as_deref(),
        &params.tool_context.main_loop_model,
        params.model.as_deref(),
    );

    let agent_id = params
        .override_params
        .as_ref()
        .and_then(|o| o.agent_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Handle message forking for context sharing
    let context_messages = params
        .fork_context_messages
        .as_ref()
        .map(|msgs| filter_incomplete_tool_calls(msgs))
        .unwrap_or_default();

    let mut initial_messages: Vec<serde_json::Value> = context_messages;
    initial_messages.extend(params.prompt_messages);

    // Resolve tools
    let resolved = resolve_agent_tools(
        &params.agent_definition,
        &params.tool_context.available_tools,
        params.is_async,
    );

    // Build agent-specific options
    let _agent_system_prompt = params
        .override_params
        .as_ref()
        .and_then(|o| o.system_prompt.clone())
        .unwrap_or_else(|| params.agent_definition.system_prompt());

    // In a full implementation, the query() loop would:
    // 1. Send messages + system prompt to the API
    // 2. Parse the response
    // 3. Execute tool calls
    // 4. Build new messages with tool results
    // 5. Repeat until max_turns or stop sequence
    //
    // Here we provide a simplified implementation that returns the initial messages
    // and a placeholder result. A full port would integrate with the actual API client.

    log::debug!(
        "Running agent '{}' (type: {}, model: {}, async: {})",
        agent_id,
        params.agent_definition.agent_type,
        resolved_model,
        params.is_async
    );

    // Record metadata
    let _ = write_agent_metadata(
        &agent_id,
        &params.agent_definition,
        &params.worktree_path,
        &params.description,
    )
    .await;

    // Build the result
    let result = AgentToolResult {
        agent_id: agent_id.clone(),
        agent_type: Some(params.agent_definition.agent_type.clone()),
        content: "Agent completed".to_string(),
        total_tool_use_count: 0,
        total_duration_ms: 0,
        total_tokens: 0,
        usage: super::agent_tool_utils::TokenUsage::default(),
    };

    Ok(RunAgentResult {
        messages: initial_messages,
        result,
    })
}

/// Write agent metadata for persistence.
async fn write_agent_metadata(
    agent_id: &str,
    agent_definition: &AgentDefinition,
    worktree_path: &Option<String>,
    description: &Option<String>,
) -> std::io::Result<()> {
    let metadata_dir = std::env::current_dir()?
        .join(".claude")
        .join("subagents")
        .join(agent_id);
    fs::create_dir_all(&metadata_dir).await?;

    let meta = serde_json::json!({
        "agentType": agent_definition.agent_type,
        "worktreePath": worktree_path,
        "description": description,
    });

    fs::write(
        metadata_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta)?,
    )
    .await
}

/// Clean up resources after agent completion.
pub fn cleanup_agent(agent_id: &str) {
    // Release any resources associated with this agent
    // In a full implementation, this would:
    // - Clear MCP server connections specific to this agent
    // - Clear session hooks
    // - Release file state cache memory
    // - Kill any background tasks spawned by this agent
    // - Release todo entries
    log::debug!("Cleaning up agent: {}", agent_id);
}

/// Extract a summary from agent messages (for async agent notifications).
pub fn extract_agent_summary(messages: &[serde_json::Value]) -> String {
    // Get the last assistant message with text content
    for msg in messages.iter().rev() {
        if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
            if let Some(arr) = content.as_array() {
                let text = super::agent_tool_utils::extract_text_content(arr, "\n");
                if !text.is_empty() {
                    // Truncate to reasonable notification length
                    if text.len() > 500 {
                        return format!("{}...", &text[..497]);
                    }
                    return text;
                }
            }
        }
    }

    // Fall back to partial result extraction
    extract_partial_result(messages).unwrap_or_else(|| "Agent completed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent_def() -> AgentDefinition {
        AgentDefinition {
            agent_type: "test".to_string(),
            when_to_use: "test".to_string(),
            tools: vec!["*".to_string()],
            disallowed_tools: vec![],
            source: "built-in".to_string(),
            base_dir: "built-in".to_string(),
            get_system_prompt: Arc::new(|| String::new()),
            model: None,
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
    fn test_resolve_agent_model_override() {
        assert_eq!(
            resolve_agent_model(Some("haiku"), "sonnet", Some("opus")),
            "opus"
        );
    }

    #[test]
    fn test_resolve_agent_model_inherit() {
        assert_eq!(
            resolve_agent_model(Some("inherit"), "sonnet", None),
            "sonnet"
        );
    }

    #[test]
    fn test_filter_incomplete_tool_calls_keeps_complete() {
        let messages = vec![
            serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [{"type": "tool_use", "id": "1", "name": "Bash"}]
                }
            }),
            serde_json::json!({
                "type": "user",
                "message": {
                    "content": [{"type": "tool_result", "tool_use_id": "1", "content": "done"}]
                }
            }),
        ];
        let filtered = filter_incomplete_tool_calls(&messages);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_incomplete_tool_calls_removes_incomplete() {
        let messages = vec![serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "tool_use", "id": "1", "name": "Bash"}]
            }
        })];
        let filtered = filter_incomplete_tool_calls(&messages);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_extract_agent_summary_from_messages() {
        let messages = vec![serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": "Task completed successfully"}]
            }
        })];
        let summary = extract_agent_summary(&messages);
        assert_eq!(summary, "Task completed successfully");
    }
}
