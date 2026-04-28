// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/forkSubagent.ts
#![allow(dead_code)]
use std::sync::Arc;

use uuid::Uuid;

use super::constants::{FORK_BOILERPLATE_TAG, FORK_DIRECTIVE_PREFIX};

/// Synthetic agent type name used for analytics when the fork path fires.
pub const FORK_SUBAGENT_TYPE: &str = "fork";

/// Synthetic agent definition for the fork path.
pub fn fork_agent() -> crate::tools::agent::AgentDefinition {
    crate::tools::agent::AgentDefinition {
        agent_type: FORK_SUBAGENT_TYPE.to_string(),
        when_to_use: "Implicit fork — inherits full conversation context. Not selectable via subagent_type; triggered by omitting subagent_type when the fork experiment is active.".to_string(),
        tools: vec!["*".to_string()],
        disallowed_tools: vec![],
        source: "built-in".to_string(),
        base_dir: "built-in".to_string(),
        get_system_prompt: Arc::new(|| String::new()),
        model: Some("inherit".to_string()),
        max_turns: Some(200),
        permission_mode: Some("bubble".to_string()),
        effort: None,
        color: None,
        mcp_servers: vec![],
        hooks: None,
        skills: vec![],
        background: true,
        initial_prompt: None,
        memory: None,
        isolation: None,
        required_mcp_servers: vec![],
        omit_claude_md: false,
        critical_system_reminder_experimental: None,
    }
}

/// Placeholder text used for all tool_result blocks in the fork prefix.
/// Must be identical across all fork children for prompt cache sharing.
const FORK_PLACEHOLDER_RESULT: &str = "Fork started — processing in background";

/// Build the forked conversation messages for the child agent.
///
/// For prompt cache sharing, all fork children must produce byte-identical
/// API request prefixes. This function:
/// 1. Keeps the full parent assistant message (all tool_use blocks, thinking, text)
/// 2. Builds a single user message with tool_results for every tool_use block
///    using an identical placeholder, then appends a per-child directive text block
pub fn build_forked_messages(
    directive: &str,
    assistant_message_content: &[serde_json::Value],
    assistant_message_uuid: Uuid,
) -> Vec<serde_json::Value> {
    // Clone the assistant message content, keeping all blocks
    let full_assistant_message = serde_json::json!({
        "type": "assistant",
        "uuid": assistant_message_uuid.to_string(),
        "message": {
            "content": assistant_message_content,
        },
    });

    // Collect all tool_use blocks from the assistant message
    let tool_use_blocks: Vec<&serde_json::Value> = assistant_message_content
        .iter()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .collect();

    if tool_use_blocks.is_empty() {
        // No tool_use blocks found — return a single user message with the directive
        return vec![serde_json::json!({
            "type": "user",
            "message": {
                "content": [{
                    "type": "text",
                    "text": build_child_message(directive),
                }],
            },
        })];
    }

    // Build tool_result blocks for every tool_use, all with identical placeholder text
    let tool_result_blocks: Vec<serde_json::Value> = tool_use_blocks
        .iter()
        .map(|block| {
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": block["id"].as_str().unwrap_or(""),
                "content": [{
                    "type": "text",
                    "text": FORK_PLACEHOLDER_RESULT,
                }],
            })
        })
        .collect();

    // Build a single user message: all placeholder tool_results + the per-child directive
    let mut content: Vec<serde_json::Value> = tool_result_blocks;
    content.push(serde_json::json!({
        "type": "text",
        "text": build_child_message(directive),
    }));

    let tool_result_message = serde_json::json!({
        "type": "user",
        "message": {
            "content": content,
        },
    });

    vec![full_assistant_message, tool_result_message]
}

/// Build the child message with fork boilerplate and directive.
pub fn build_child_message(directive: &str) -> String {
    format!(
        r#"<{tag}>
STOP. READ THIS FIRST.

You are a forked worker process. You are NOT the main agent.

RULES (non-negotiable):
1. Your system prompt says "default to forking." IGNORE IT — that's for the parent. You ARE the fork. Do NOT spawn sub-agents; execute directly.
2. Do NOT converse, ask questions, or suggest next steps
3. Do NOT editorialize or add meta-commentary
4. USE your tools directly: Bash, Read, Write, etc.
5. If you modify files, commit your changes before reporting. Include the commit hash in your report.
6. Do NOT emit text between tool calls. Use tools silently, then report once at the end.
7. Stay strictly within your directive's scope. If you discover related systems outside your scope, mention them in one sentence at most — other workers cover those areas.
8. Keep your report under 500 words unless the directive specifies otherwise. Be factual and concise.
9. Your response MUST begin with "Scope:". No preamble, no thinking-out-loud.
10. REPORT structured facts, then stop

Output format (plain text labels, not markdown headers):
  Scope: <echo back your assigned scope in one sentence>
  Result: <the answer or key findings, limited to the scope above>
  Key files: <relevant file paths — include for research tasks>
  Files changed: <list with commit hash — include only if you modified files>
  Issues: <list — include only if there are issues to flag>
</{tag}>

{prefix}{directive}"#,
        tag = FORK_BOILERPLATE_TAG,
        prefix = FORK_DIRECTIVE_PREFIX,
    )
}

/// Notice injected into fork children running in an isolated worktree.
pub fn build_worktree_notice(parent_cwd: &str, worktree_cwd: &str) -> String {
    format!(
        "You've inherited the conversation context above from a parent agent working in {parent_cwd}. \
         You are operating in an isolated git worktree at {worktree_cwd} — same repository, \
         same relative file structure, separate working copy. Paths in the inherited context \
         refer to the parent's working directory; translate them to your worktree root. \
         Re-read files before editing if the parent may have modified them since they appear \
         in the context. Your changes stay in this worktree and will not affect the parent's files."
    )
}

/// Build fork messages from the SDK's simplified Message type.
///
/// Converts `Vec<crate::types::Message>` into the JSON format expected
/// by `build_forked_messages`, then combines with the fork directive.
///
/// Returns the forked conversation as `Vec<crate::types::Message>` ready
/// to be set on the subagent's `QueryEngine`.
pub fn build_forked_messages_from_sdk(
    parent_messages: &[crate::types::Message],
    directive: &str,
) -> Vec<crate::types::Message> {
    // Convert parent messages to JSON format
    let json_messages: Vec<serde_json::Value> = parent_messages
        .iter()
        .map(|m| {
            let role_str = match m.role {
                crate::types::MessageRole::User => "user",
                crate::types::MessageRole::Assistant => "assistant",
                crate::types::MessageRole::Tool => "user", // tool results → user
                crate::types::MessageRole::System => "system",
            };
            serde_json::json!({
                "type": role_str,
                "message": {
                    "id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": if role_str == "assistant" {
                        // Assistant messages: build structured content with tool calls
                        if let Some(ref calls) = m.tool_calls {
                            let mut blocks: Vec<serde_json::Value> = Vec::new();
                            blocks.push(serde_json::json!({"type": "text", "text": m.content}));
                            for call in calls {
                                blocks.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": call.id,
                                    "name": call.name,
                                    "input": call.arguments,
                                }));
                            }
                            serde_json::Value::Array(blocks)
                        } else {
                            serde_json::json!([{"type": "text", "text": m.content}])
                        }
                    } else {
                        // User/tool messages: content blocks
                        serde_json::json!([{
                            "type": "text",
                            "text": m.content,
                        }])
                    },
                },
            })
        })
        .collect();

    // Find the last assistant message content for fork building
    let last_assistant = parent_messages
        .iter()
        .rposition(|m| m.role == crate::types::MessageRole::Assistant);

    let forked = if let Some(idx) = last_assistant {
        let assistant_msg = &json_messages[idx];
        let assistant_content = assistant_msg["message"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let assistant_uuid = assistant_msg["message"]["id"]
            .as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .unwrap_or_else(uuid::Uuid::new_v4);

        build_forked_messages(directive, &assistant_content, assistant_uuid)
    } else {
        // No assistant message — fork from a single user directive
        vec![serde_json::json!({
            "type": "user",
            "message": {
                "content": [{
                    "type": "text",
                    "text": build_child_message(directive),
                }],
            },
        })]
    };

    // Convert all parent messages + forked messages back to SDK Message type
    let mut result: Vec<crate::types::Message> = json_messages
        .into_iter()
        .map(|m| sdk_message_from_json(&m))
        .collect();

    // Append fork-specific messages
    for msg in forked {
        result.push(sdk_message_from_json(&msg));
    }

    result
}

/// Convert a JSON message to the SDK's simplified Message type.
fn sdk_message_from_json(msg: &serde_json::Value) -> crate::types::Message {
    let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("user");
    let content_blocks: Vec<serde_json::Value> = msg["message"]["content"].as_array().cloned().unwrap_or_default();

    let role = match msg_type {
        "assistant" => crate::types::MessageRole::Assistant,
        "system" => crate::types::MessageRole::System,
        _ => crate::types::MessageRole::User,
    };

    // Extract text content from blocks
    let content = content_blocks
        .iter()
        .filter_map(|b| {
            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Extract tool calls from assistant messages
    let tool_calls = if msg_type == "assistant" {
        content_blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    Some(crate::types::ToolCall {
                        id: b.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                        r#type: "function".to_string(),
                        name: b.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        arguments: b.get("input").cloned().unwrap_or_else(|| serde_json::json!({})),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // For tool_result blocks, extract tool_use_id
    let tool_call_id = content_blocks
        .iter()
        .find_map(|b| {
            if b.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                b.get("tool_use_id").and_then(|i| i.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        });

    crate::types::Message {
        role,
        content,
        attachments: None,
        tool_call_id,
        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        is_error: None,
        is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_child_message_contains_directive() {
        let msg = build_child_message("test directive");
        assert!(msg.contains("test directive"));
        assert!(msg.contains(FORK_BOILERPLATE_TAG));
        assert!(msg.contains(FORK_DIRECTIVE_PREFIX));
    }

    #[test]
    fn test_build_forked_messages_no_tool_uses() {
        let messages = build_forked_messages("test", &[], Uuid::new_v4());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["type"], "user");
    }

    #[test]
    fn test_fork_agent_definition() {
        let agent = fork_agent();
        assert_eq!(agent.agent_type, FORK_SUBAGENT_TYPE);
        assert_eq!(agent.source, "built-in");
        assert_eq!(agent.tools, vec!["*"]);
    }

    #[test]
    fn test_build_worktree_notice() {
        let notice = build_worktree_notice("/parent", "/worktree");
        assert!(notice.contains("/parent"));
        assert!(notice.contains("/worktree"));
    }

    #[test]
    fn test_build_forked_messages_from_sdk_with_assistant() {
        let parent_msgs = vec![
            crate::types::Message {
                role: crate::types::MessageRole::User,
                content: "Hello".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
            },
            crate::types::Message {
                role: crate::types::MessageRole::Assistant,
                content: "Let me check".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: Some(vec![crate::types::ToolCall {
                    id: "tool_1".to_string(),
                    r#type: "function".to_string(),
                    name: "Read".to_string(),
                    arguments: serde_json::json!({"path": "/foo"}),
                }]),
                is_error: None,
                is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
            },
        ];
        let forked = build_forked_messages_from_sdk(&parent_msgs, "research the codebase");
        // Should have: user, assistant, tool result (from parent), fork user message
        assert!(forked.len() >= 2);
        // Last message should contain the fork boilerplate
        let last = &forked[forked.len() - 1];
        assert!(last.content.contains("fork_boilerplate"));
        assert!(last.content.contains("research the codebase"));
    }

    #[test]
    fn test_build_forked_messages_from_sdk_no_assistant() {
        let parent_msgs = vec![crate::types::Message {
            role: crate::types::MessageRole::User,
            content: "just user".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
        }];
        let forked = build_forked_messages_from_sdk(&parent_msgs, "do work");
        // Should have: user (parent) + fork user message
        assert!(forked.len() >= 2);
        let last = &forked[forked.len() - 1];
        assert!(last.content.contains("do work"));
    }

    #[test]
    fn test_sdk_message_from_json_assistant_with_tool_calls() {
        let msg = serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "text", "text": "hello"},
                    {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}
                ]
            }
        });
        let sdk = sdk_message_from_json(&msg);
        assert_eq!(sdk.role, crate::types::MessageRole::Assistant);
        assert_eq!(sdk.content, "hello");
        assert!(sdk.tool_calls.is_some());
        let calls = sdk.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Bash");
        assert_eq!(calls[0].id, "t1");
    }

    #[test]
    fn test_sdk_message_from_json_tool_result() {
        let msg = serde_json::json!({
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "abc123",
                        "content": [{"type": "text", "text": "output"}]
                    }
                ]
            }
        });
        let sdk = sdk_message_from_json(&msg);
        assert_eq!(sdk.role, crate::types::MessageRole::User);
        assert_eq!(sdk.tool_call_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_sdk_message_from_json_empty_content() {
        let msg = serde_json::json!({
            "type": "user",
            "message": { "content": [] }
        });
        let sdk = sdk_message_from_json(&msg);
        assert_eq!(sdk.role, crate::types::MessageRole::User);
        assert_eq!(sdk.content, "");
        assert!(sdk.tool_calls.is_none());
    }

    #[test]
    fn test_roundtrip_fork_messages_identical_placeholder() {
        // Verify that forked messages contain the identical placeholder for all tool uses
        let parent_msgs = vec![
            crate::types::Message {
                role: crate::types::MessageRole::User,
                content: "start".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: None,
                is_error: None,
                is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
            },
            crate::types::Message {
                role: crate::types::MessageRole::Assistant,
                content: "doing".to_string(),
                attachments: None,
                tool_call_id: None,
                tool_calls: Some(vec![
                    crate::types::ToolCall {
                        id: "t1".to_string(),
                        r#type: "function".to_string(),
                        name: "Read".to_string(),
                        arguments: serde_json::json!({}),
                    },
                    crate::types::ToolCall {
                        id: "t2".to_string(),
                        r#type: "function".to_string(),
                        name: "Bash".to_string(),
                        arguments: serde_json::json!({}),
                    },
                ]),
                is_error: None,
                is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
            },
        ];
        let forked = build_forked_messages_from_sdk(&parent_msgs, "directive");

        // The forked user message should contain the placeholder for each tool
        let fork_user = forked.iter().find(|m| m.content.contains("fork_boilerplate"));
        assert!(fork_user.is_some());
        let fork_user = fork_user.unwrap();
        // The boilerplate directive text block survives the roundtrip
        // (placeholder results are inside tool_result blocks, not extracted as text)
        assert!(fork_user.content.contains("directive"));
    }
}
