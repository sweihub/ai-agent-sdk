//! Away summary service - generates a short session recap for the "while you were away" card.
//!
//! Translates awaySummary.ts from claude code.

use std::sync::atomic::{AtomicBool, Ordering};

/// Recap only needs recent context — truncate to avoid "prompt too long" on
/// large sessions. 30 messages ≈ ~15 exchanges, plenty for "where we left off."
pub const RECENT_MESSAGE_WINDOW: usize = 30;

/// Build the prompt for generating away summary
pub fn build_away_summary_prompt(memory: Option<&str>) -> String {
    let memory_block = memory
        .map(|m| format!("Session memory (broker context):\n{}\n\n", m))
        .unwrap_or_default();

    format!(
        "{}The user stepped away and is coming back. Write exactly 1-3 short sentences. Start by stating the high-level task — what they are building or debugging, not implementation details. Next: the concrete next step. Skip status reports and commit recaps.",
        memory_block
    )
}

/// Generate away summary result
#[derive(Debug, Clone)]
pub struct AwaySummaryResult {
    pub summary: Option<String>,
    pub was_aborted: bool,
}

impl AwaySummaryResult {
    pub fn aborted() -> Self {
        Self {
            summary: None,
            was_aborted: true,
        }
    }

    pub fn success(summary: String) -> Self {
        Self {
            summary: Some(summary),
            was_aborted: false,
        }
    }

    pub fn empty() -> Self {
        Self {
            summary: None,
            was_aborted: false,
        }
    }
}

/// Generate a short session recap for the "while you were away" card.
/// Takes messages, recent ones only, queries the model, and returns the summary.
/// Returns empty on error, abort, or empty transcript.
pub async fn generate_away_summary(
    messages: &[crate::types::Message],
    api_key: &str,
    abort_signal: &AtomicBool,
) -> AwaySummaryResult {
    if messages.is_empty() {
        return AwaySummaryResult::empty();
    }

    // Get session memory content if available
    let memory = match crate::session_memory::get_session_memory_content().await {
        Ok(m) => m,
        Err(_) => None,
    };

    // Build recent messages (last 30)
    let recent: Vec<&crate::types::Message> = messages
        .iter()
        .skip(messages.len().saturating_sub(RECENT_MESSAGE_WINDOW))
        .collect();

    if recent.is_empty() {
        return AwaySummaryResult::empty();
    }

    // Build API messages — filter out system messages and convert to API format
    let api_messages: Vec<serde_json::Value> = recent
        .iter()
        .filter_map(|msg| {
            // Skip system messages (they're metadata, not conversation content)
            if msg.role == crate::types::MessageRole::System {
                return None;
            }
            // Skip tool messages (tool results are already embedded in conversation)
            if msg.role == crate::types::MessageRole::Tool {
                return None;
            }
            Some(match msg.role {
                crate::types::MessageRole::User => serde_json::json!({
                    "role": "user",
                    "content": msg.content
                }),
                crate::types::MessageRole::Assistant => {
                    if let Some(ref tool_calls) = msg.tool_calls {
                        // Anthropic format: content array with text and tool_use blocks
                        let mut content_blocks: Vec<serde_json::Value> = Vec::new();
                        if !msg.content.is_empty() {
                            content_blocks.push(serde_json::json!({
                                "type": "text",
                                "text": msg.content
                            }));
                        }
                        for tc in tool_calls {
                            content_blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                        serde_json::json!({
                            "role": "assistant",
                            "content": content_blocks
                        })
                    } else {
                        serde_json::json!({
                            "role": "assistant",
                            "content": msg.content
                        })
                    }
                }
                crate::types::MessageRole::System | crate::types::MessageRole::Tool => {
                    return None;
                }
            })
        })
        .collect();

    // Add the away summary prompt as a user message
    let prompt_text = build_away_summary_prompt(memory.as_deref());
    let mut full_messages = api_messages;
    full_messages.push(serde_json::json!({
        "role": "user",
        "content": prompt_text
    }));

    // Get small fast model (Haiku)
    let model = get_small_fast_model();

    // Build the request body — non-streaming
    let request_body = serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "stream": false,
        "messages": full_messages,
        "thinking": { "type": "disabled" }
    });

    // Check abort before making the request
    if abort_signal.load(Ordering::SeqCst) {
        return AwaySummaryResult::aborted();
    }

    // Make the API request
    let result = make_away_api_request(api_key, &request_body).await;

    match result {
        Ok(text) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                AwaySummaryResult::empty()
            } else {
                AwaySummaryResult::success(trimmed)
            }
        }
        Err(_) => {
            if abort_signal.load(Ordering::SeqCst) {
                AwaySummaryResult::aborted()
            } else {
                AwaySummaryResult::empty()
            }
        }
    }
}

/// Get the small fast model name (Haiku)
fn get_small_fast_model() -> String {
    std::env::var("AI_SMALL_FAST_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude-haiku-4-5-20250513".to_string())
}

/// Make the non-streaming API request for away summary
async fn make_away_api_request(api_key: &str, request_body: &serde_json::Value) -> Result<String, String> {
    let client = reqwest::Client::new();
    let base_url = std::env::var("AI_API_BASE_URL")
        .ok()
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());
    let url = format!("{}/v1/messages", base_url);

    // Determine if this is Anthropic API or a third-party API
    let is_anthropic = base_url.contains("anthropic.com");

    let request_builder = if is_anthropic {
        client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("User-Agent", crate::utils::http::get_user_agent())
            .json(request_body)
    } else {
        client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("User-Agent", crate::utils::http::get_user_agent())
            .json(request_body)
    };

    let response = request_builder.send().await.map_err(|e| format!("API request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, error_text));
    }

    // Parse JSON response
    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Check for API error
    if let Some(error) = response_json.get("error") {
        let error_msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
        return Err(format!("API error: {}", error_msg));
    }

    // Extract text content from response
    let content = response_json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|blocks| blocks.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| "No content in response".to_string())?
        .to_string();

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_away_summary_prompt_with_memory() {
        let memory = "Working on the AI agent SDK";
        let prompt = build_away_summary_prompt(Some(memory));
        assert!(prompt.contains("Session memory"));
        assert!(prompt.contains(memory));
    }

    #[test]
    fn test_build_away_summary_prompt_without_memory() {
        let prompt = build_away_summary_prompt(None);
        assert!(!prompt.contains("Session memory"));
        assert!(prompt.contains("stepped away"));
    }

    #[test]
    fn test_away_summary_result() {
        let result = AwaySummaryResult::success("Test summary".to_string());
        assert!(result.summary.is_some());
        assert!(!result.was_aborted);

        let result = AwaySummaryResult::aborted();
        assert!(result.summary.is_none());
        assert!(result.was_aborted);

        let result = AwaySummaryResult::empty();
        assert!(result.summary.is_none());
        assert!(!result.was_aborted);
    }
}
