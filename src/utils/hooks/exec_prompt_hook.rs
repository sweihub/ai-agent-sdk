// Source: ~/claudecode/openclaudecode/src/utils/hooks/execPromptHook.ts
#![allow(dead_code)]

use std::sync::Arc;
use uuid::Uuid;

use crate::types::Message;
use crate::utils::hooks::hook_helpers::{HookResponse, add_arguments_to_prompt, hook_response_schema};

/// Result of a hook execution
pub enum HookResult {
    Success {
        hook_name: String,
        hook_event: String,
        tool_use_id: String,
    },
    Blocking {
        blocking_error: String,
        command: String,
        prevent_continuation: bool,
        stop_reason: String,
    },
    Cancelled,
    NonBlockingError {
        hook_name: String,
        hook_event: String,
        tool_use_id: String,
        stderr: String,
        stdout: String,
        exit_code: i32,
    },
}

/// Represents a prompt hook configuration
pub struct PromptHook {
    /// The prompt to send to the model
    pub prompt: String,
    /// Optional timeout in seconds
    pub timeout: Option<u64>,
    /// Optional model override
    pub model: Option<String>,
}

/// Execute a prompt-based hook using an LLM
pub async fn exec_prompt_hook(
    hook: &PromptHook,
    hook_name: &str,
    hook_event: &str,
    json_input: &str,
    _signal: tokio::sync::watch::Receiver<bool>,
    tool_use_context: Arc<crate::utils::hooks::can_use_tool::ToolUseContext>,
    messages: Option<&[Message]>,
    tool_use_id: Option<String>,
) -> HookResult {
    // Use provided tool_use_id or generate a new one
    let effective_tool_use_id = tool_use_id.unwrap_or_else(|| format!("hook-{}", Uuid::new_v4()));

    // Replace $ARGUMENTS with the JSON input
    let processed_prompt = add_arguments_to_prompt(&hook.prompt, json_input);
    log_for_debugging(&format!(
        "Hooks: Processing prompt hook with prompt: {}",
        processed_prompt.chars().take(200).collect::<String>()
    ));

    // Create user message directly
    let user_message = create_user_message(&processed_prompt);

    // Prepend conversation history if provided
    let messages_to_query: Vec<serde_json::Value> = if let Some(msgs) = messages {
        let mut msg_vec: Vec<serde_json::Value> = msgs.iter().map(|m| message_to_json(m)).collect();
        msg_vec.push(message_to_json_user(&user_message));
        msg_vec
    } else {
        vec![message_to_json_user(&user_message)]
    };

    log_for_debugging(&format!(
        "Hooks: Querying model with {} messages",
        messages_to_query.len()
    ));

    // Query the model with a small fast model
    let hook_timeout_ms = hook.timeout.map_or(30_000, |t| t * 1000);

    // Create abort channel
    let (abort_tx, abort_rx) = tokio::sync::watch::channel(false);

    // Setup timeout
    let timeout_handle = tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(hook_timeout_ms)).await;
        let _ = abort_tx.send(true);
    });

    // Build the query
    let model = hook.model.clone().unwrap_or_else(get_small_fast_model);
    let system_prompt = r#"You are evaluating a hook in Claude Code.

Your response must be a JSON object matching one of the following schemas:
1. If the condition is met, return: {"ok": true}
2. If the condition is not met, return: {"ok": false, "reason": "Reason for why it is not met}"#;

    // Make the API call
    let response =
        query_model_without_streaming(&messages_to_query, system_prompt, &model, &tool_use_context)
            .await;

    timeout_handle.abort();

    // Check if aborted
    if *abort_rx.borrow() {
        return HookResult::Cancelled;
    }

    match response {
        Ok(content) => {
            // Update response length for spinner display (not applicable in Rust)
            let full_response = content.trim();
            log_for_debugging(&format!("Hooks: Model response: {}", full_response));

            // Parse JSON response
            let json = match serde_json::from_str::<serde_json::Value>(full_response) {
                Ok(j) => j,
                Err(_) => {
                    log_for_debugging(&format!(
                        "Hooks: error parsing response as JSON: {}",
                        full_response
                    ));
                    return HookResult::NonBlockingError {
                        hook_name: hook_name.to_string(),
                        hook_event: hook_event.to_string(),
                        tool_use_id: effective_tool_use_id,
                        stderr: "JSON validation failed".to_string(),
                        stdout: full_response.to_string(),
                        exit_code: 1,
                    };
                }
            };

            // Validate against hook response schema
            let parsed = serde_json::from_value::<HookResponse>(json.clone());
            match parsed {
                Ok(hook_resp) => {
                    // Failed to meet condition
                    if !hook_resp.ok {
                        let reason = hook_resp.reason.unwrap_or_default();
                        log_for_debugging(&format!(
                            "Hooks: Prompt hook condition was not met: {}",
                            reason
                        ));
                        return HookResult::Blocking {
                            blocking_error: format!(
                                "Prompt hook condition was not met: {}",
                                reason
                            ),
                            command: hook.prompt.clone(),
                            prevent_continuation: true,
                            stop_reason: reason,
                        };
                    }

                    // Condition was met
                    log_for_debugging("Hooks: Prompt hook condition was met");
                    return HookResult::Success {
                        hook_name: hook_name.to_string(),
                        hook_event: hook_event.to_string(),
                        tool_use_id: effective_tool_use_id,
                    };
                }
                Err(err) => {
                    log_for_debugging(&format!(
                        "Hooks: model response does not conform to expected schema: {}",
                        err
                    ));
                    return HookResult::NonBlockingError {
                        hook_name: hook_name.to_string(),
                        hook_event: hook_event.to_string(),
                        tool_use_id: effective_tool_use_id,
                        stderr: format!("Schema validation failed: {}", err),
                        stdout: full_response.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        Err(e) => {
            log_for_debugging(&format!("Hooks: Prompt hook error: {}", e));
            return HookResult::NonBlockingError {
                hook_name: hook_name.to_string(),
                hook_event: hook_event.to_string(),
                tool_use_id: effective_tool_use_id,
                stderr: format!("Error executing prompt hook: {}", e),
                stdout: String::new(),
                exit_code: 1,
            };
        }
    }
}

/// Create a user message with the given content
fn create_user_message(content: &str) -> Message {
    Message {
        role: crate::types::api_types::MessageRole::User,
        content: content.to_string(),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
    }
}

/// Convert Message to JSON value
fn message_to_json(msg: &Message) -> serde_json::Value {
    serde_json::json!({
        "role": msg.role.as_str(),
        "content": &msg.content
    })
}

/// Convert user message struct to JSON value (forces role to "user")
fn message_to_json_user(msg: &Message) -> serde_json::Value {
    serde_json::json!({
        "role": "user",
        "content": &msg.content
    })
}

/// Get the small fast model (simplified)
fn get_small_fast_model() -> String {
    "claude-3-haiku-20240307".to_string()
}

/// Query model without streaming — makes a real non-streaming API call
async fn query_model_without_streaming(
    messages: &[serde_json::Value],
    system_prompt: &str,
    model: &str,
    _tool_use_context: &crate::utils::hooks::can_use_tool::ToolUseContext,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Get API credentials
    let base_url = std::env::var("AI_API_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let api_key = std::env::var("AI_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .map_err(|e| format!("No API key found: {}", e))?;

    let url = format!("{}/v1/messages", base_url);
    let is_anthropic = base_url.contains("anthropic.com");

    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": [{"type": "text", "text": system_prompt}],
        "messages": messages,
        "temperature": 0.0,
        "output": {
            "type": "json_schema",
            "name": "hook_response",
            "schema": hook_response_schema(),
            "strict": true
        }
    });

    let client = reqwest::Client::new();
    let mut req_builder = client.post(&url).json(&request_body)
        .header("Content-Type", "application/json");

    if is_anthropic {
        req_builder = req_builder
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01");
    } else {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = req_builder.send().await?;
    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(format!("API error {}: {}", status, body).into());
    }

    // Extract text content from response
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let text = extract_text(&parsed);
    if text.is_empty() {
        return Err("Empty response from model".into());
    }

    Ok(text)
}

/// Extract text content from an API response (supports both Anthropic and OpenAI formats)
fn extract_text(response: &serde_json::Value) -> String {
    // OpenAI format: choices[].message.content
    if let Some(content) = response.get("choices").and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str()) {
        return content.to_string();
    }
    // Anthropic format: content[].text
    if let Some(blocks) = response.get("content").and_then(|c| c.as_array()) {
        let mut texts = Vec::new();
        for block in blocks {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                texts.push(text.to_string());
            }
        }
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    String::new()
}

/// Log for debugging
fn log_for_debugging(msg: &str) {
    log::debug!("{}", msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_anthropic() {
        let response = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello from Anthropic"},
                {"type": "text", "text": "Second block"}
            ]
        });
        assert_eq!(extract_text(&response), "Hello from Anthropic\nSecond block");
    }

    #[test]
    fn test_extract_text_anthropic_single_block() {
        let response = serde_json::json!({
            "content": [
                {"type": "text", "text": "Single block"}
            ]
        });
        assert_eq!(extract_text(&response), "Single block");
    }

    #[test]
    fn test_extract_text_openai() {
        let response = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": "Hello from OpenAI"
                    }
                }
            ]
        });
        assert_eq!(extract_text(&response), "Hello from OpenAI");
    }

    #[test]
    fn test_extract_text_empty() {
        let response = serde_json::json!({});
        assert_eq!(extract_text(&response), "");
    }

    #[test]
    fn test_extract_text_no_text_blocks() {
        let response = serde_json::json!({
            "content": [
                {"type": "tool_use", "name": "some_tool", "input": {}}
            ]
        });
        assert_eq!(extract_text(&response), "");
    }

    #[test]
    fn test_message_to_json_user() {
        let msg = Message {
            role: crate::types::api_types::MessageRole::User,
            content: "test content".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
        };
        let json = message_to_json(&msg);
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "test content");
    }

    #[test]
    fn test_message_to_json_assistant() {
        let msg = Message {
            role: crate::types::api_types::MessageRole::Assistant,
            content: "assistant reply".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
        };
        let json = message_to_json(&msg);
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "assistant reply");
    }

    #[test]
    fn test_message_to_json_user_forces_user_role() {
        let msg = Message {
            role: crate::types::api_types::MessageRole::Assistant,
            content: "should be user".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: None,
            is_error: None,
            is_meta: None,
        is_api_error_message: None,
        error_details: None,
        uuid: None,
        };
        let json = message_to_json_user(&msg);
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "should be user");
    }

    #[test]
    fn test_role_to_str() {
        assert_eq!(crate::types::api_types::MessageRole::User.as_str(), "user");
        assert_eq!(crate::types::api_types::MessageRole::Assistant.as_str(), "assistant");
        assert_eq!(crate::types::api_types::MessageRole::Tool.as_str(), "tool");
        assert_eq!(crate::types::api_types::MessageRole::System.as_str(), "system");
    }
}
