// Source: ~/claudecode/openclaudecode/src/utils/hooks/apiQueryHookHelper.ts
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::Arc;
use uuid::Uuid;

use crate::types::{Message, MessageRole};

/// System prompt type - a vector of strings
pub type SystemPrompt = Vec<String>;

/// Context for REPL hooks (both post-sampling and stop hooks)
#[derive(Clone)]
pub struct ReplHookContext {
    /// Full message history including assistant responses
    pub messages: Vec<Message>,
    /// System prompt
    pub system_prompt: SystemPrompt,
    /// User context key-value pairs
    pub user_context: std::collections::HashMap<String, String>,
    /// System context key-value pairs
    pub system_context: std::collections::HashMap<String, String>,
    /// Tool use context
    pub tool_use_context: Arc<crate::utils::hooks::can_use_tool::ToolUseContext>,
    /// Query source identifier
    pub query_source: Option<String>,
    /// Optional: message count for API queries
    pub query_message_count: Option<usize>,
}

/// Configuration for an API query hook
pub struct ApiQueryHookConfig<TResult> {
    /// Query source name
    pub name: String,
    /// Whether this hook should run
    pub should_run: Box<
        dyn Fn(&ReplHookContext) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send>>
            + Send
            + Sync,
    >,
    /// Build the complete message list to send to the API
    pub build_messages: Box<dyn Fn(&ReplHookContext) -> Vec<Message> + Send + Sync>,
    /// Optional: override system prompt (defaults to context.system_prompt)
    pub system_prompt: Option<SystemPrompt>,
    /// Optional: whether to use tools from context (defaults to true)
    pub use_tools: Option<bool>,
    /// Parse the response content into a result
    pub parse_response: Box<dyn Fn(&str, &ReplHookContext) -> TResult + Send + Sync>,
    /// Log the result
    pub log_result: Box<dyn Fn(ApiQueryResult<TResult>, &ReplHookContext) + Send + Sync>,
    /// Get the model to use (lazy loaded)
    pub get_model: Box<dyn Fn(&ReplHookContext) -> String + Send + Sync>,
}

/// Result of an API query hook execution
pub enum ApiQueryResult<TResult> {
    Success {
        query_name: String,
        result: TResult,
        message_id: String,
        model: String,
        uuid: String,
    },
    Error {
        query_name: String,
        error: Box<dyn std::error::Error + Send + Sync>,
        uuid: String,
    },
}

/// Create an API query hook from the given configuration.
/// Returns an async function that executes the hook when called.
pub fn create_api_query_hook<TResult: 'static>(
    config: ApiQueryHookConfig<TResult>,
) -> Box<dyn Fn(ReplHookContext) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>
{
    let config = Arc::new(config);
    Box::new(move |context: ReplHookContext| {
        let config = config.clone();
        Box::pin(async move {
            let should_run = (config.should_run)(&context).await;
            if !should_run {
                return;
            }

            let uuid = Uuid::new_v4().to_string();

            // Build messages using the config's build_messages function
            let messages = (config.build_messages)(&context);
            // Note: we can't mutate context directly in Rust; the caller
            // would need to handle query_message_count tracking externally

            // Use config's system prompt if provided, otherwise use context's
            let system_prompt = config
                .system_prompt
                .clone()
                .unwrap_or_else(|| context.system_prompt.clone());

            // Use config's tools preference (defaults to true = use context tools)
            // In Rust, tool access would be through the tool_use_context

            // Get model (lazy loaded)
            let model = (config.get_model)(&context);

            // Make API call - this would use the actual query function
            // The TS version calls queryModelWithoutStreaming
            let response_result =
                query_model_without_streaming_impl(&messages, &system_prompt, &model, &context)
                    .await;

            match response_result {
                Ok(response) => {
                    // Extract text content from response JSON
                    let content = extract_text_content(&response.content).trim().to_string();

                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (config.parse_response)(&content, &context)
                    }));

                    match result {
                        Ok(parsed_result) => {
                            (config.log_result)(
                                ApiQueryResult::Success {
                                    query_name: config.name.clone(),
                                    result: parsed_result,
                                    message_id: response.message_id,
                                    model,
                                    uuid,
                                },
                                &context,
                            );
                        }
                        Err(err) => {
                            let error = if let Some(s) = err.downcast_ref::<String>() {
                                Box::new(std::io::Error::new(std::io::ErrorKind::Other, s.clone()))
                            } else if let Some(s) = err.downcast_ref::<&str>() {
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    s.to_string(),
                                ))
                            } else {
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "Unknown panic in parse_response",
                                ))
                            };
                            (config.log_result)(
                                ApiQueryResult::Error {
                                    query_name: config.name.clone(),
                                    error,
                                    uuid,
                                },
                                &context,
                            );
                        }
                    }
                }
                Err(error) => {
                    log_error(&format!("API query hook error: {}", error));
                    (config.log_result)(
                        ApiQueryResult::Error {
                            query_name: config.name.clone(),
                            error,
                            uuid,
                        },
                        &context,
                    );
                }
            }
        })
    })
}

/// Internal struct for API response
struct ApiResponse {
    message_id: String,
    content: String,
}

/// Get the API key from environment variables.
/// Checks AI_AUTH_TOKEN, ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN in order.
fn get_api_key() -> Result<String, String> {
    if let Ok(key) = std::env::var("AI_AUTH_TOKEN") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    if let Ok(key) = std::env::var("ANTHROPIC_AUTH_TOKEN") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    Err("No API key found. Set AI_AUTH_TOKEN, ANTHROPIC_API_KEY, or ANTHROPIC_AUTH_TOKEN"
        .to_string())
}

/// Convert a MessageRole to its API-compatible string representation.
fn role_to_api_string(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
        MessageRole::System => "system",
    }
}

/// Make a real non-streaming Anthropic Messages API call.
/// Follows the same pattern as `make_away_api_request` in away_summary.rs.
async fn query_model_without_streaming_impl(
    messages: &[Message],
    system_prompt: &SystemPrompt,
    model: &str,
    _context: &ReplHookContext,
) -> Result<ApiResponse, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = get_api_key().map_err(|e| {
        Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            e,
        ))
    })?;

    let base_url = std::env::var("AI_API_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());
    let url = format!("{}/v1/messages", base_url);

    // Determine if this is Anthropic API or a third-party API
    let is_anthropic = base_url.contains("anthropic.com");

    // Build API messages from Message structs
    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let mut msg_obj = serde_json::json!({
                "role": role_to_api_string(&m.role),
                "content": &m.content
            });

            // Add tool_call_id for tool role messages
            if m.role == MessageRole::Tool {
                if let Some(ref tool_call_id) = m.tool_call_id {
                    msg_obj["tool_use_id"] = serde_json::json!(tool_call_id);
                }
            }

            msg_obj
        })
        .collect();

    // Build system prompt as Anthropic format
    let system_prompt_value = serde_json::json!({
        "type": "text",
        "text": system_prompt.join("\n")
    });

    // Build request body
    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system_prompt_value,
        "messages": api_messages,
        "temperature": 0.0,
    });

    let client = reqwest::Client::new();
    let request_builder = if is_anthropic {
        client
            .post(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("User-Agent", crate::utils::http::get_user_agent())
            .json(&request_body)
    } else {
        client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("User-Agent", crate::utils::http::get_user_agent())
            .json(&request_body)
    };

    let response = request_builder
        .send()
        .await
        .map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("API request failed: {}", e),
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("API error {}: {}", status, error_text),
            ),
        ));
    }

    // Parse JSON response
    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse API response: {}", e),
            ))
        })?;

    // Check for API error in response body
    if let Some(error) = response_json.get("error") {
        let error_msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error");
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("API error: {}", error_msg),
            ),
        ));
    }

    // Extract message ID
    let message_id = response_json
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract raw JSON content for downstream text extraction
    let content = serde_json::to_string(&response_json).unwrap_or_default();

    Ok(ApiResponse {
        message_id,
        content,
    })
}

/// Extract text content from API response JSON.
/// Supports both Anthropic format (content array of blocks with text)
/// and OpenAI-compatible format (choices[0].message.content).
/// Falls back to raw string if JSON parsing fails or format is unrecognized.
fn extract_text_content(response_json: &str) -> String {
    let Ok(response) = serde_json::from_str::<serde_json::Value>(response_json) else {
        return response_json.to_string();
    };

    // OpenAI-compatible: response.choices[0].message.content
    if let Some(content) = response
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        return content.to_string();
    }

    // Anthropic: response.content[N].text blocks
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

    response_json.to_string()
}

/// Log an error (simplified version of logError)
fn log_error(msg: &str) {
    log::error!("{}", msg);
}

/// Create a system prompt from a list of strings
pub fn as_system_prompt(parts: Vec<&str>) -> SystemPrompt {
    parts.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_content_anthropic() {
        let response = r#"{
            "id": "msg_abc123",
            "content": [
                {"type": "text", "text": "Hello from Anthropic"},
                {"type": "text", "text": "Second block"}
            ]
        }"#;
        let result = extract_text_content(response);
        assert_eq!(result, "Hello from Anthropic\nSecond block");
    }

    #[test]
    fn test_extract_text_content_anthropic_single_block() {
        let response = r#"{
            "id": "msg_abc123",
            "content": [
                {"type": "text", "text": "Single block response"}
            ]
        }"#;
        let result = extract_text_content(response);
        assert_eq!(result, "Single block response");
    }

    #[test]
    fn test_extract_text_content_openai() {
        let response = r#"{
            "id": "chatcmpl-123",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello from OpenAI compatible"
                    }
                }
            ]
        }"#;
        let result = extract_text_content(response);
        assert_eq!(result, "Hello from OpenAI compatible");
    }

    #[test]
    fn test_extract_text_content_fallback_invalid_json() {
        let raw = "this is not json at all";
        let result = extract_text_content(raw);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_extract_text_content_fallback_unknown_format() {
        let response = r#"{
            "foo": "bar",
            "data": "no content or choices here"
        }"#;
        let result = extract_text_content(response);
        // Falls back to re-serialized JSON string
        assert!(result.contains("foo"));
        assert!(result.contains("bar"));
    }

    #[test]
    fn test_role_to_api_string() {
        assert_eq!(role_to_api_string(&MessageRole::User), "user");
        assert_eq!(role_to_api_string(&MessageRole::Assistant), "assistant");
        assert_eq!(role_to_api_string(&MessageRole::Tool), "tool");
        assert_eq!(role_to_api_string(&MessageRole::System), "system");
    }

    #[test]
    fn test_as_system_prompt() {
        let prompt = as_system_prompt(vec!["line 1", "line 2", "line 3"]);
        assert_eq!(prompt, vec!["line 1", "line 2", "line 3"]);
    }

    #[tokio::test]
    async fn test_create_api_query_hook_should_run_false() {
        // Verify the hook short-circuits when should_run returns false
        let logged = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let logged_clone = logged.clone();
        let hook = create_api_query_hook(ApiQueryHookConfig {
            name: "test_hook".to_string(),
            should_run: Box::new(|_| Box::pin(async { false })),
            build_messages: Box::new(|_| vec![]),
            system_prompt: None,
            use_tools: None,
            parse_response: Box::new(|_, _| ()),
            log_result: Box::new(move |_, _| {
                logged_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
            get_model: Box::new(|_| "test-model".to_string()),
        });

        // Create a minimal context
        let context = ReplHookContext {
            messages: vec![],
            system_prompt: vec![],
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            tool_use_context: Arc::new(
                crate::utils::hooks::can_use_tool::ToolUseContext {
                    session_id: "test".to_string(),
                    cwd: None,
                    is_non_interactive_session: true,
                    options: None,
                }
            ),
            query_source: None,
            query_message_count: None,
        };

        hook(context).await;
        // should_run returned false, so log_result should NOT have been called
        assert!(
            !logged.load(std::sync::atomic::Ordering::SeqCst),
            "log_result should not be called when should_run is false"
        );
    }

    #[tokio::test]
    async fn test_create_api_query_hook_calls_impl() {
        // Verify the hook calls query_model_without_streaming_impl when should_run is true.
        // Since the impl makes a real HTTP call, it will fail without an API key,
        // but we verify the error path is wired correctly.
        let hook_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();
        let hook = create_api_query_hook(ApiQueryHookConfig {
            name: "wiring_test".to_string(),
            should_run: Box::new(|_| Box::pin(async { true })),
            build_messages: Box::new(|_| vec![Message {
                role: MessageRole::User,
                content: "test".to_string(),
                ..Default::default()
            }]),
            system_prompt: Some(vec!["system prompt".to_string()]),
            use_tools: None,
            parse_response: Box::new(|_, _| ()),
            log_result: Box::new(move |result, _| {
                hook_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                // We expect an Error result because no real API key is set
                match result {
                    ApiQueryResult::Error { error, .. } => {
                        // Expected: no API key or connection error
                        let _ = error.to_string();
                    }
                    ApiQueryResult::Success { .. } => {
                        // If it somehow succeeds, that's fine too
                    }
                }
            }),
            get_model: Box::new(|_| "claude-sonnet-4-5-20250514".to_string()),
        });

        let context = ReplHookContext {
            messages: vec![],
            system_prompt: vec![],
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            tool_use_context: Arc::new(
                crate::utils::hooks::can_use_tool::ToolUseContext {
                    session_id: "test".to_string(),
                    cwd: None,
                    is_non_interactive_session: true,
                    options: None,
                }
            ),
            query_source: None,
            query_message_count: None,
        };

        hook(context).await;
        assert!(
            hook_called.load(std::sync::atomic::Ordering::SeqCst),
            "log_result should have been called"
        );
    }

    #[test]
    fn test_extract_text_content_anthropic_with_tool_use_blocks() {
        // Anthropic response with tool_use block mixed with text
        let response = r#"{
            "id": "msg_xyz",
            "content": [
                {"type": "text", "text": "Let me check that for you."},
                {"type": "tool_use", "id": "tool_1", "name": "Read", "input": {"path": "file.txt"}},
                {"type": "text", "text": "Here is the result."}
            ]
        }"#;
        let result = extract_text_content(response);
        // Should only extract text blocks, skipping tool_use blocks
        assert_eq!(result, "Let me check that for you.\nHere is the result.");
    }
}
