// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/resumeAgent.ts
#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use super::agent_tool_utils::{extract_partial_result, finalize_agent_tool};
use super::load_agents_dir::AgentDefinition;
use super::run_agent::{
    AgentOverrides, RunAgentParams, ToolContext, filter_incomplete_tool_calls, run_agent,
};

/// Result from resuming an agent.
pub struct ResumeAgentResult {
    pub agent_id: String,
    pub description: String,
    pub output_file: String,
}

/// Transcript data loaded from disk for a resumed agent.
struct AgentTranscript {
    messages: Vec<serde_json::Value>,
    content_replacements: Option<serde_json::Value>,
}

/// Agent metadata loaded from disk.
struct AgentMetadata {
    agent_type: Option<String>,
    worktree_path: Option<String>,
    description: Option<String>,
}

/// Resume a previously running background agent.
pub async fn resume_agent_background(
    agent_id: &str,
    prompt: &str,
    tool_context: ToolContext,
) -> Result<ResumeAgentResult, String> {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Load transcript and metadata
    let (transcript, meta) = load_transcript_and_metadata(agent_id).await?;

    // Filter messages for resume
    let resumed_messages = filter_whitespace_only_assistant_messages(
        filter_orphaned_thinking_only_messages(filter_unresolved_tool_uses(&transcript.messages)),
    );

    // Check if worktree still exists
    let resumed_worktree_path = meta.worktree_path.as_ref().and_then(|p| {
        let path = PathBuf::from(p);
        if path.is_dir() {
            // Bump mtime so stale-worktree cleanup doesn't delete a just-resumed worktree
            let marker = path.join(".claude_resume_marker");
            tokio::task::block_in_place(|| std::fs::write(&marker, "").ok());
            Some(p.clone())
        } else {
            log::debug!(
                "Resumed worktree {} no longer exists; falling back to parent cwd",
                p
            );
            None
        }
    });

    // Determine which agent definition to use
    let selected_agent = select_agent_for_resume(&meta, &tool_context);

    let ui_description = meta
        .description
        .clone()
        .unwrap_or_else(|| "(resumed)".to_string());

    // Resolve model
    let resolved_model = selected_agent
        .model
        .clone()
        .unwrap_or_else(|| tool_context.main_loop_model.clone());

    // Build run params
    let mut prompt_messages = resumed_messages;
    // Add the resume prompt as a user message
    prompt_messages.push(serde_json::json!({
        "type": "user",
        "message": {
            "content": [{"type": "text", "text": prompt}]
        }
    }));

    let run_params = RunAgentParams {
        agent_definition: selected_agent.clone(),
        prompt_messages,
        tool_context,
        is_async: true,
        override_params: None,
        model: None,
        max_turns: None,
        fork_context_messages: None,
        allowed_tools: None,
        worktree_path: resumed_worktree_path.clone(),
        description: meta.description.clone(),
    };

    // Run the agent (in a full implementation, this would be an async generator)
    let _result = run_agent(run_params).await?;

    let output_file = get_task_output_path(agent_id);

    Ok(ResumeAgentResult {
        agent_id: agent_id.to_string(),
        description: ui_description,
        output_file,
    })
}

/// Load transcript and metadata for an agent.
async fn load_transcript_and_metadata(
    agent_id: &str,
) -> Result<(AgentTranscript, AgentMetadata), String> {
    let transcript_path = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(".claude")
        .join("subagents")
        .join(agent_id)
        .join("transcript.json");

    let metadata_path = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(".claude")
        .join("subagents")
        .join(agent_id)
        .join("metadata.json");

    // Load transcript
    let transcript_content = fs::read_to_string(&transcript_path)
        .await
        .map_err(|e| format!("Failed to read transcript: {}", e))?;
    let messages: Vec<serde_json::Value> = serde_json::from_str(&transcript_content)
        .map_err(|e| format!("Failed to parse transcript: {}", e))?;

    let content_replacements = None; // Simplified

    // Load metadata
    let meta = if fs::metadata(&metadata_path).await.is_ok() {
        let meta_content = fs::read_to_string(&metadata_path)
            .await
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let meta_json: serde_json::Value = serde_json::from_str(&meta_content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;
        AgentMetadata {
            agent_type: meta_json
                .get("agentType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            worktree_path: meta_json
                .get("worktreePath")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            description: meta_json
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    } else {
        AgentMetadata {
            agent_type: None,
            worktree_path: None,
            description: None,
        }
    };

    Ok((
        AgentTranscript {
            messages,
            content_replacements,
        },
        meta,
    ))
}

/// Filter out whitespace-only assistant messages.
fn filter_whitespace_only_assistant_messages(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .filter(|msg| {
            if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                return true;
            }
            // Check if message has non-whitespace content
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    for block in arr {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            if !text.trim().is_empty() {
                                return true;
                            }
                        }
                        if block.get("type").and_then(|t| t.as_str()) != Some("text") {
                            return true; // Non-text blocks mean it's not whitespace-only
                        }
                    }
                    return false; // All text blocks were whitespace
                }
            }
            true // No content means not whitespace-only
        })
        .collect()
}

/// Filter out orphaned thinking-only messages (thinking without tool use or text).
fn filter_orphaned_thinking_only_messages(
    messages: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .filter(|msg| {
            if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                return true;
            }
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    // Keep if has toolUse or text blocks
                    return arr.iter().any(|block| {
                        block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                            || block.get("type").and_then(|t| t.as_str()) == Some("text")
                    });
                }
            }
            true
        })
        .collect()
}

/// Filter out unresolved tool uses (tool_use without matching tool_result).
fn filter_unresolved_tool_uses(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let messages = messages.to_vec();

    // Build set of tool_use IDs that have results
    let mut tool_use_ids_with_results = std::collections::HashSet::new();
    for msg in &messages {
        if msg.get("type").and_then(|t| t.as_str()) == Some("user") {
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
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

    // Remove assistant messages that have tool_uses without results
    messages
        .into_iter()
        .filter(|msg| {
            if msg.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                return true;
            }
            if let Some(content) = msg.get("message").and_then(|m| m.get("content")) {
                if let Some(arr) = content.as_array() {
                    // Check if all tool_uses have results
                    let tool_uses: Vec<_> = arr
                        .iter()
                        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                        .collect();
                    if tool_uses.is_empty() {
                        return true; // No tool uses, keep the message
                    }
                    return tool_uses.iter().all(|block| {
                        block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .is_some_and(|id| tool_use_ids_with_results.contains(id))
                    });
                }
            }
            true
        })
        .collect()
}

/// Select the appropriate agent definition for resume.
fn select_agent_for_resume(meta: &AgentMetadata, tool_context: &ToolContext) -> AgentDefinition {
    if let Some(ref agent_type) = meta.agent_type {
        // Look up the agent definition
        if let Some(found) = tool_context
            .agent_definitions
            .iter()
            .find(|a| &a.agent_type == agent_type)
        {
            return found.clone();
        }
    }

    // Fall back to general-purpose agent
    super::built_in_agents::general_purpose_agent()
}

/// Get the output file path for a task.
fn get_task_output_path(agent_id: &str) -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("tasks")
        .join(format!("{}.output", agent_id))
        .to_string_lossy()
        .to_string()
}

/// General purpose agent for fallback during resume.
mod general_purpose_fallback {
    use std::sync::Arc;

    use super::AgentDefinition;

    pub fn general_purpose_agent() -> AgentDefinition {
        AgentDefinition {
            agent_type: "general-purpose".to_string(),
            when_to_use: "General-purpose agent for multi-step tasks".to_string(),
            tools: vec!["*".to_string()],
            disallowed_tools: vec![],
            source: "built-in".to_string(),
            base_dir: "built-in".to_string(),
            get_system_prompt: Arc::new(|| "You are a helpful assistant.".to_string()),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_whitespace_only_messages() {
        let messages = vec![
            serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [{"type": "text", "text": "   \n  "}]
                }
            }),
            serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [{"type": "text", "text": "hello"}]
                }
            }),
        ];
        let filtered = filter_whitespace_only_assistant_messages(messages);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_orphaned_thinking_messages() {
        let messages = vec![
            serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [] // empty content
                }
            }),
            serde_json::json!({
                "type": "assistant",
                "message": {
                    "content": [{"type": "tool_use", "id": "1", "name": "Bash"}]
                }
            }),
        ];
        let filtered = filter_orphaned_thinking_only_messages(messages);
        assert_eq!(filtered.len(), 1); // Keep the one with tool_use
    }
}
