// Source: ~/claudecode/openclaudecode/src/utils/tokens.ts
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    pub iterations: Option<Vec<IterationUsage>>,
}

/// Per-iteration usage from the Anthropic API (server-side tool loops)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IterationUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub msg_type: String,
    pub message: InnerMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerMessage {
    pub content: Vec<ContentBlock>,
    pub usage: Option<TokenUsage>,
    pub id: Option<String>,
    pub model: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        input: serde_json::Value,
        name: Option<String>,
    },
}

const SYNTHETIC_MODEL: &str = "synthetic";

pub fn get_token_usage(message: &Message) -> Option<&TokenUsage> {
    if message.msg_type != "assistant" {
        return None;
    }

    let usage = message.message.usage.as_ref()?;

    if message.message.model.as_deref() == Some(SYNTHETIC_MODEL) {
        return None;
    }

    if let Some(ContentBlock::Text { text }) = message.message.content.first() {
        if text.contains("SYNTHETIC") {
            return None;
        }
    }

    Some(usage)
}

pub fn get_token_count_from_usage(usage: &TokenUsage) -> u32 {
    let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);
    let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
    (usage.input_tokens + cache_creation + cache_read + usage.output_tokens) as u32
}

/// Extract the message ID/UUID from an assistant message for sibling detection.
pub fn get_assistant_message_id(message: &Message) -> Option<&str> {
    if message.msg_type != "assistant" {
        return None;
    }
    if let Some(ref id) = message.message.id {
        return Some(id);
    }
    message.message.uuid.as_deref()
}

pub fn token_count_from_last_api_response(messages: &[Message]) -> u32 {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            return get_token_count_from_usage(usage);
        }
    }
    0
}

/// Final context window size from the last API response's usage.iterations[-1].
/// Used for task_budget.remaining computation across compaction boundaries.
/// Falls back to top-level input_tokens + output_tokens when iterations is absent.
/// Excludes cache tokens to match server-side budget countdown.
pub fn final_context_tokens_from_last_response(messages: &[Message]) -> u64 {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            if let Some(ref iterations) = usage.iterations {
                if !iterations.is_empty() {
                    if let Some(last) = iterations.last() {
                        return last.input_tokens + last.output_tokens;
                    }
                }
            }
            // No iterations → no server tool loop → top-level usage IS the final window
            return usage.input_tokens + usage.output_tokens;
        }
    }
    0
}

pub fn get_current_usage(messages: &[Message]) -> Option<TokenUsage> {
    for message in messages.iter().rev() {
        if let Some(usage) = get_token_usage(message) {
            return Some(TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
                iterations: usage.iterations.clone(),
            });
        }
    }
    None
}

pub fn does_most_recent_assistant_message_exceed_200k(messages: &[Message]) -> bool {
    const THRESHOLD: u32 = 200_000;

    let last_asst = messages.iter().rev().find(|m| m.msg_type == "assistant");
    let last_asst = match last_asst {
        Some(m) => m,
        None => return false,
    };

    match get_token_usage(last_asst) {
        Some(usage) => get_token_count_from_usage(usage) > THRESHOLD,
        None => false,
    }
}

pub fn get_assistant_message_content_length(message: &Message) -> usize {
    let mut content_length = 0;

    for block in &message.message.content {
        match block {
            ContentBlock::Text { text } => content_length += text.len(),
            ContentBlock::Thinking { thinking } => content_length += thinking.len(),
            ContentBlock::RedactedThinking { data } => content_length += data.len(),
            ContentBlock::ToolUse { input, .. } => {
                content_length += serde_json::to_string(input).map(|s| s.len()).unwrap_or(0);
            }
        }
    }

    content_length
}

/// Rough token estimation for a slice of messages (4 chars per token).
pub fn rough_token_count_estimation_for_messages(messages: &[Message]) -> u32 {
    messages
        .iter()
        .map(|m| {
            let total_chars: usize = m.message.content.iter().map(|b| match b {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::Thinking { thinking } => thinking.len(),
                ContentBlock::RedactedThinking { data } => data.len(),
                ContentBlock::ToolUse { input, .. } => {
                    serde_json::to_string(input).map(|s| s.len()).unwrap_or(0)
                }
            }).sum();
            (total_chars as f64 / 4.0) as u32
        })
        .sum()
}

/// Token count with estimation for trailing messages that haven't seen an API response yet.
/// Walks backward to find the last usage-bearing message, then walks back further to
/// find any earlier sibling with the same message.id (parallel tool call splits).
/// Returns usage count + rough estimate for all messages after the first sibling.
pub fn token_count_with_estimation(messages: &[Message]) -> u32 {
    let mut i = messages.len();
    while i > 0 {
        i -= 1;
        let message = &messages[i];
        if let Some(usage) = get_token_usage(message) {
            // Walk back past any earlier sibling records split from the same API
            // response (same message.id) so interleaved tool_results between them
            // are included in the estimation slice.
            if let Some(response_id) = get_assistant_message_id(message) {
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    let prior = &messages[j];
                    if let Some(prior_id) = get_assistant_message_id(prior) {
                        if prior_id == response_id {
                            i = j;
                        } else {
                            break;
                        }
                    }
                    // priorId === undefined: user/tool_result/attachment, keep walking
                }
            }
            let trailing = if i + 1 < messages.len() {
                rough_token_count_estimation_for_messages(&messages[i + 1..])
            } else {
                0
            };
            return get_token_count_from_usage(usage) + trailing;
        }
    }
    rough_token_count_estimation_for_messages(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_count() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: Some(20),
            cache_read_input_tokens: Some(30),
            iterations: None,
        };
        assert_eq!(get_token_count_from_usage(&usage), 200);
    }

    #[test]
    fn test_final_context_tokens_with_iterations() {
        let msg = Message {
            msg_type: "assistant".to_string(),
            message: InnerMessage {
                content: vec![],
                usage: Some(TokenUsage {
                    input_tokens: 1000,
                    output_tokens: 500,
                    cache_creation_input_tokens: Some(200),
                    cache_read_input_tokens: Some(100),
                    iterations: Some(vec![IterationUsage {
                        input_tokens: 800,
                        output_tokens: 400,
                    }]),
                }),
                id: Some("msg-1".to_string()),
                model: None,
                uuid: None,
            },
        };
        let tokens = final_context_tokens_from_last_response(&[msg]);
        // Should use iterations[-1].input + output = 800 + 400 = 1200 (no cache)
        assert_eq!(tokens, 1200);
    }

    #[test]
    fn test_final_context_tokens_without_iterations() {
        let msg = Message {
            msg_type: "assistant".to_string(),
            message: InnerMessage {
                content: vec![],
                usage: Some(TokenUsage {
                    input_tokens: 1000,
                    output_tokens: 500,
                    cache_creation_input_tokens: Some(200),
                    cache_read_input_tokens: Some(100),
                    iterations: None,
                }),
                id: Some("msg-1".to_string()),
                model: None,
                uuid: None,
            },
        };
        let tokens = final_context_tokens_from_last_response(&[msg]);
        // Should use input + output = 1500 (no cache, no iterations)
        assert_eq!(tokens, 1500);
    }

    #[test]
    fn test_token_count_with_estimation_basic() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            iterations: None,
        };
        let msg = Message {
            msg_type: "assistant".to_string(),
            message: InnerMessage {
                content: vec![ContentBlock::Text { text: "hello".to_string() }],
                usage: Some(usage),
                id: Some("msg-1".to_string()),
                model: None,
                uuid: None,
            },
        };
        let count = token_count_with_estimation(&[msg.clone()]);
        assert_eq!(count, 150);
    }

    #[test]
    fn test_rough_token_estimation_for_messages() {
        let msg = Message {
            msg_type: "user".to_string(),
            message: InnerMessage {
                content: vec![ContentBlock::Text { text: "Hello world".to_string() }],
                usage: None,
                id: None,
                model: None,
                uuid: None,
            },
        };
        // "Hello world" = 11 chars / 4 = 2.75 → 2
        let est = rough_token_count_estimation_for_messages(&[msg]);
        assert!(est >= 2 && est <= 3);
    }
}
