// Source: /data/home/swei/claudecode/openclaudecode/src/services/tokenEstimation.ts
//! Token estimation for text.
//!
//! Provides token counting similar to claude code's token estimation.
//! Includes both rough character-based estimation and API-accurate counting
//! via `/v1/messages/count_tokens`.

use crate::types::Message;
use serde::{Deserialize, Serialize};

/// Estimated token count with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEstimate {
    pub tokens: usize,
    pub characters: usize,
    pub words: usize,
    pub method: EstimationMethod,
}

/// Method used for estimation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EstimationMethod {
    /// Fast estimation using character ratio
    CharacterRatio,
    /// Word-based estimation
    WordBased,
    /// Exact TikToken estimation (if available)
    TikToken,
}

// ============================================================================
// Translation of claude code's tokenEstimation.ts - strictly line by line
// ============================================================================

/// Rough token count estimation - matches original TypeScript:
/// `export function roughTokenCountEstimation(content: string, bytesPerToken: number = 4): number`
pub fn rough_token_count_estimation(content: &str, bytes_per_token: f64) -> usize {
    (content.len() as f64 / bytes_per_token).round() as usize
}

/// Returns bytes-per-token ratio for a given file extension
/// Matches original TypeScript:
/// `export function bytesPerTokenForFileType(fileExtension: string): number`
/// Dense JSON has many single-character tokens which makes ratio closer to 2
pub fn bytes_per_token_for_file_type(file_extension: &str) -> f64 {
    match file_extension {
        "json" | "jsonl" | "jsonc" => 2.0,
        _ => 4.0,
    }
}

/// Like roughTokenCountEstimation but uses more accurate bytes-per-token ratio
/// when file type is known - matches original TypeScript:
/// `export function roughTokenCountEstimationForFileType(content: string, fileExtension: string): number`
pub fn rough_token_count_estimation_for_file_type(content: &str, file_extension: &str) -> usize {
    rough_token_count_estimation(content, bytes_per_token_for_file_type(file_extension))
}

/// Estimate tokens for a single message - matches original TypeScript:
/// `export function roughTokenCountEstimationForMessage(message: {...}): number`
pub fn rough_token_count_estimation_for_message(message: &Message) -> usize {
    rough_token_count_estimation_for_content(&message.content)
}

/// Estimate tokens for message content (string or array) - matches original TypeScript:
/// `function roughTokenCountEstimationForContent(content: ...): number`
pub fn rough_token_count_estimation_for_content(content: &str) -> usize {
    if content.is_empty() {
        return 0;
    }
    rough_token_count_estimation(content, 4.0)
}

/// Estimate tokens for an array of messages - matches original TypeScript:
/// `export function roughTokenCountEstimationForMessages(messages: readonly {...}[]): number`
pub fn rough_token_count_estimation_for_messages(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|msg| rough_token_count_estimation_for_message(msg))
        .sum()
}

// ============================================================================
// Legacy estimation functions (kept for backward compatibility)
// ============================================================================

/// Estimate tokens using character ratio method (faster but less accurate)
/// Average ratio is ~4 characters per token for English
pub fn estimate_tokens_characters(text: &str) -> TokenEstimate {
    let characters = text.len();
    let words = text.split_whitespace().count();

    // Use 4:1 character to token ratio as baseline
    // Adjust based on text characteristics
    let ratio = if text.contains("```") {
        // Code blocks have more characters per token
        5.5
    } else if words > 0 {
        let avg_word_len = characters as f64 / words as f64;
        if avg_word_len > 8.0 {
            // Long words = more characters per token
            5.0
        } else if avg_word_len < 3.0 {
            // Short words = fewer characters per token
            3.5
        } else {
            4.0
        }
    } else {
        4.0
    };

    let tokens = (characters as f64 / ratio).ceil() as usize;

    TokenEstimate {
        tokens,
        characters,
        words,
        method: EstimationMethod::CharacterRatio,
    }
}

/// Estimate tokens using word-based method
pub fn estimate_tokens_words(text: &str) -> TokenEstimate {
    let words = text.split_whitespace().count();
    let characters = text.len();

    // Average ~1.3 words per token for English
    let tokens = (words as f64 / 1.3).ceil() as usize;

    TokenEstimate {
        tokens,
        characters,
        words,
        method: EstimationMethod::WordBased,
    }
}

/// Estimate tokens using combined method (best balance of speed and accuracy)
pub fn estimate_tokens(text: &str) -> TokenEstimate {
    let char_estimate = estimate_tokens_characters(text);
    let word_estimate = estimate_tokens_words(text);

    // Use the average of both methods for better accuracy
    let tokens = (char_estimate.tokens + word_estimate.tokens) / 2;

    TokenEstimate {
        tokens,
        characters: char_estimate.characters,
        words: char_estimate.words,
        method: EstimationMethod::CharacterRatio,
    }
}

/// Estimate tokens in messages (handles role/content format)
pub fn estimate_message_tokens<T: MessageContent>(messages: &[T]) -> usize {
    messages
        .iter()
        .map(|m| {
            let content = m.content();
            // Add overhead for role annotation
            let role_overhead = 4;
            estimate_tokens(content).tokens + role_overhead
        })
        .sum()
}

/// Estimate tokens in a conversation string
pub fn estimate_conversation(conversation: &str) -> TokenEstimate {
    // Count turns by looking for common patterns
    let turns = conversation
        .matches("User:")
        .count()
        .max(conversation.matches("Assistant:").count());

    // Each turn has overhead for role prefix
    let turn_overhead = turns * 10;

    let base = estimate_tokens(conversation);
    TokenEstimate {
        tokens: base.tokens + turn_overhead,
        characters: base.characters,
        words: base.words,
        method: base.method,
    }
}

/// Estimate tokens for tool definitions
pub fn estimate_tool_definitions(tools: &[ToolDefinition]) -> usize {
    tools
        .iter()
        .map(|t| {
            let name_tokens = estimate_tokens(&t.name).tokens;
            let desc_tokens = t
                .description
                .as_ref()
                .map(|d| estimate_tokens(d).tokens)
                .unwrap_or(0);
            let params_tokens = estimate_tokens(&t.input_schema).tokens;
            name_tokens + desc_tokens + params_tokens + 20 // overhead
        })
        .sum()
}

/// Simple message content for estimation
pub trait MessageContent {
    fn content(&self) -> &str;
}

impl MessageContent for String {
    fn content(&self) -> &str {
        self.as_str()
    }
}

impl MessageContent for &str {
    fn content(&self) -> &str {
        self
    }
}

/// Message with role
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl MessageContent for ChatMessage {
    fn content(&self) -> &str {
        &self.content
    }
}

/// Tool definition for estimation
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: String,
}

/// Calculate padding needed for context window
/// Returns the amount of extra input tokens that could fit given the output token budget
pub fn calculate_padding(input_tokens: usize, max_tokens: usize, context_limit: usize) -> usize {
    // Calculate how much room is left for input given the output budget
    let available_for_input = context_limit.saturating_sub(max_tokens);
    if input_tokens < available_for_input {
        available_for_input.saturating_sub(input_tokens)
    } else {
        0
    }
}

/// Estimate if content fits in context
pub fn fits_in_context(content_tokens: usize, max_tokens: usize, context_limit: usize) -> bool {
    content_tokens + max_tokens <= context_limit
}

/// Token encoding utilities
pub mod encoding {
    /// Common tokenization patterns
    pub const CHARS_PER_TOKEN_EN: f64 = 4.0;
    pub const CHARS_PER_TOKEN_CODE: f64 = 5.5;
    pub const CHARS_PER_TOKEN_CJK: f64 = 2.0; // Chinese, Japanese, Korean

    /// Detect if text is primarily code
    pub fn is_code(text: &str) -> bool {
        let code_indicators = [
            "```", "function", "class ", "def ", "const ", "let ", "var ", "import ",
        ];
        code_indicators.iter().any(|i| text.contains(i))
    }

    /// Detect if text is primarily CJK
    pub fn is_cjk(text: &str) -> bool {
        text.chars().any(|c| {
            (c >= '\u{4E00}' && c <= '\u{9FFF}') ||  // CJK Unified Ideographs
            (c >= '\u{3040}' && c <= '\u{309F}') ||  // Hiragana
            (c >= '\u{30A0}' && c <= '\u{30FF}') ||  // Katakana
            (c >= '\u{AC00}' && c <= '\u{D7AF}') // Korean
        })
    }

    /// Get appropriate chars per token ratio
    pub fn chars_per_token(text: &str) -> f64 {
        if is_code(text) {
            super::encoding::CHARS_PER_TOKEN_CODE
        } else if is_cjk(text) {
            super::encoding::CHARS_PER_TOKEN_CJK
        } else {
            super::encoding::CHARS_PER_TOKEN_EN
        }
    }
}

// ============================================================================
// count_tokens API: /v1/messages/count_tokens
// Translated from TypeScript countMessagesTokensWithAPI / countTokensWithAPI
// ============================================================================

/// Minimum thinking budget for token counting when messages contain thinking blocks
/// API constraint: max_tokens must be greater than thinking.budget_tokens
pub const TOKEN_COUNT_THINKING_BUDGET: u32 = 1024;

/// Max tokens for token counting requests (used when thinking is enabled)
pub const TOKEN_COUNT_MAX_TOKENS: u32 = 2048;

/// Error type for count_tokens API operations
#[derive(Debug, Clone)]
pub struct CountTokensError(pub String);

impl std::fmt::Display for CountTokensError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "count_tokens error: {}", self.0)
    }
}

impl std::error::Error for CountTokensError {}

/// Check if messages contain thinking or redacted_thinking blocks
/// Matches TypeScript: hasThinkingBlocks()
fn has_thinking_blocks(messages: &[serde_json::Value]) -> bool {
    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str());
        if role == Some("assistant") {
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for block in content {
                    let block_type = block.get("type").and_then(|t| t.as_str());
                    if block_type == Some("thinking") || block_type == Some("redacted_thinking") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Strip tool search-specific fields from messages before sending for token counting.
/// Removes 'caller' from tool_use blocks and 'tool_reference' from tool_result content.
/// These fields are only valid with the tool search beta and will cause errors otherwise.
///
/// Matches TypeScript: stripToolSearchFieldsFromMessages()
fn strip_tool_search_fields_from_messages(
    messages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|message| {
            let content = message.get("content");
            let Some(content_array) = content.and_then(|c| c.as_array()) else {
                return message.clone();
            };

            let normalized_content: Vec<serde_json::Value> = content_array
                .iter()
                .map(|block| {
                    let block_type = block.get("type").and_then(|t| t.as_str());

                    // Strip 'caller' from tool_use blocks (assistant messages)
                    if block_type == Some("tool_use") {
                        let id = block.get("id").cloned();
                        let name = block.get("name").cloned();
                        let input = block.get("input").cloned();
                        return serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        });
                    }

                    // Strip tool_reference blocks from tool_result content (user messages)
                    if block_type == Some("tool_result") {
                        let tool_use_id = block.get("tool_use_id").cloned();
                        let result_content = block.get("content");

                        if let Some(result_content_array) = result_content.and_then(|c| c.as_array())
                        {
                            let filtered: Vec<serde_json::Value> = result_content_array
                                .iter()
                                .filter(|c| {
                                    !(c.get("type")
                                        .and_then(|t| t.as_str())
                                        .is_some()
                                        && c.get("type")
                                            .and_then(|t| t.as_str())
                                            == Some("tool_reference"))
                                })
                                .cloned()
                                .collect();

                            if filtered.is_empty() {
                                return serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": [{ "type": "text", "text": "[tool references]" }]
                                });
                            }
                            if filtered.len() != result_content_array.len() {
                                return serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": filtered
                                });
                            }
                        }
                    }

                    block.clone()
                })
                .collect();

            let mut msg = message.clone();
            msg["content"] = serde_json::json!(normalized_content);
            msg
        })
        .collect()
}

/// Estimate tokens for a single content block.
///
/// Matches TypeScript: roughTokenCountEstimationForBlock()
/// Handles text, image, document, tool_result, tool_use, thinking, redacted_thinking blocks.
///
/// # Returns
/// Estimated token count for this block.
pub fn rough_token_count_estimation_for_block(block: &serde_json::Value) -> usize {
    let block_type = block.get("type").and_then(|t| t.as_str());

    match block_type {
        Some("text") => {
            let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
            rough_token_count_estimation(text, 4.0)
        }
        Some("image") | Some("document") => {
            // Images are resized to max 2000x2000 (5333 tokens).
            // Use conservative estimate matching microCompact's IMAGE_MAX_TOKEN_SIZE.
            // Documents (base64 PDF) also capped at 2000 API tokens.
            2000
        }
        Some("tool_result") => {
            // Recursively estimate tool_result content array
            if let Some(content) = block.get("content") {
                rough_token_count_estimation_for_content_array(content)
            } else {
                0
            }
        }
        Some("tool_use") => {
            // input is the JSON the model generated — arbitrarily large
            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let input_str = serde_json::to_string(&block.get("input").unwrap_or(&serde_json::json!({})))
                .unwrap_or_default();
            rough_token_count_estimation(&(name.to_string() + &input_str), 4.0)
        }
        Some("thinking") => {
            let text = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
            rough_token_count_estimation(text, 4.0)
        }
        Some("redacted_thinking") => {
            let text = block.get("data").and_then(|t| t.as_str()).unwrap_or("");
            rough_token_count_estimation(text, 4.0)
        }
        // server_tool_use, web_search_tool_result, mcp_tool_use, etc. —
        // text-like payloads, stringify to estimate.
        _ => {
            let serialized = serde_json::to_string(block).unwrap_or_default();
            rough_token_count_estimation(&serialized, 4.0)
        }
    }
}

/// Estimate tokens for a content array (handles both string and array content).
///
/// Matches TypeScript: roughTokenCountEstimationForContent()
///
/// # Arguments
/// * `content` - Either a JSON string value or an array of content blocks
///
/// # Returns
/// Estimated total token count for all blocks.
pub fn rough_token_count_estimation_for_content_array(content: &serde_json::Value) -> usize {
    if let Some(text) = content.as_str() {
        return rough_token_count_estimation(text, 4.0);
    }
    let Some(blocks) = content.as_array() else {
        return 0;
    };
    blocks.iter().map(|b| rough_token_count_estimation_for_block(b)).sum()
}

/// Fixed token overhead added by the API when tools are present.
/// The API adds a tool prompt preamble (~500 tokens) once per API call when tools are present.
/// When we count tools individually via the token counting API, each call includes this overhead,
/// leading to N × overhead instead of 1 × overhead for N tools.
/// We subtract this overhead from per-tool counts to show accurate tool content sizes.
///
/// Matches TypeScript: TOOL_TOKEN_COUNT_OVERHEAD
pub const TOOL_TOKEN_COUNT_OVERHEAD: u64 = 500;

/// Get the base API URL from environment, defaulting to Anthropic API
fn get_base_url() -> String {
    std::env::var("AI_CODE_API_URL")
        .or_else(|_| std::env::var("AI_CODE_BASE_URL"))
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
}

/// Get the API key from environment
fn get_api_key() -> Option<String> {
    std::env::var("AI_CODE_API_KEY")
        .ok()
        .or_else(|| std::env::var("AI_AUTH_TOKEN").ok())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
}

/// Check if using Vertex provider
fn is_using_vertex() -> bool {
    let is_truthy = |v: Option<String>| {
        v.map(|x| x == "1" || x.to_lowercase() == "true")
            .unwrap_or(false)
    };
    is_truthy(std::env::var("AI_CODE_USE_VERTEX").ok())
}

/// Check if using Bedrock provider
fn is_using_bedrock() -> bool {
    let is_truthy = |v: Option<String>| {
        v.map(|x| x == "1" || x.to_lowercase() == "true")
            .unwrap_or(false)
    };
    is_truthy(std::env::var("AI_CODE_USE_BEDROCK").ok())
}

/// Check if Vertex is using a global region endpoint (Haiku not available there)
fn is_vertex_global_endpoint() -> bool {
    is_using_vertex() && get_vertex_region_for_model() == "global"
}

/// Get the Vertex region for the small/fast model
fn get_vertex_region_for_model() -> String {
    std::env::var("AI_CODE_VERTEX_REGION")
        .or_else(|_| std::env::var("AI_VERTEX_REGION"))
        .unwrap_or_else(|_| "us-east5".to_string())
}

/// Get model-specific beta features as a list of strings.
///
/// Matches TypeScript: getModelBetas(model) → getAllModelBetas(model)
/// Returns the betas that should be sent in the request for a given model.
/// Non-Haiku models get the claude-code beta header.
/// Models with 1M+ context get the context-1m beta header.
/// Filters out Bedrock-incompatible betas when on Bedrock provider.
fn get_model_betas(model: &str) -> Vec<String> {
    let mut betas = Vec::new();
    let is_haiku = model.to_lowercase().contains("haiku");

    // Non-Haiku models get the core Claude Code beta header
    if !is_haiku {
        betas.push(crate::constants::betas::CLAUDE_CODE_20250219_BETA_HEADER.to_string());
    }

    // Models with 1M+ context get the context management beta
    if model.contains("1m") || model.contains("[1m]") {
        betas.push(crate::constants::betas::CONTEXT_1M_BETA_HEADER.to_string());
    }

    // Filter for Bedrock - some betas don't work as headers on Bedrock
    if is_using_bedrock() {
        let bedrock_extra_only =
            crate::constants::betas::get_bedrock_extra_params_headers();
        betas.retain(|b| !bedrock_extra_only.contains(b.as_str()));
    }

    betas
}

/// Get extra body parameters from AI_CODE_EXTRA_BODY environment variable.
///
/// Matches TypeScript: getExtraBodyParams()
/// Parses a JSON object from the environment and returns it for spreading into API requests.
fn get_extra_body_params() -> Option<serde_json::Value> {
    let extra_str = std::env::var("AI_CODE_EXTRA_BODY").ok()?;
    serde_json::from_str(&extra_str).ok()
}

/// Normalize model string for API (strip display wrappers and context window indicators).
/// Matches TypeScript: normalizeModelStringForAPI() which strips `[1m]`/`[2m]` suffixes.
fn normalize_model_string_for_api(model: &str) -> String {
    let s = model.trim_start_matches("claude/");
    regex::Regex::new(r"\[(1|2)m\]")
        .map(|re| re.replace_all(s, "").to_string())
        .unwrap_or_else(|_| s.to_string())
}

/// Count tokens via the Anthropic `/v1/messages/count_tokens` API.
///
/// Matches TypeScript: `countMessagesTokensWithAPI(messages, tools)`
///
/// # Arguments
/// * `api_key` - Anthropic API key (or None to read from env)
/// * `base_url` - Base API URL (or None to read from env)
/// * `model` - The model to use for counting
/// * `messages` - Messages in API format (already serialized as JSON)
/// * `tools` - Optional tool definitions in Anthropic API format
/// * `betas` - Optional beta headers to include
///
/// # Returns
/// `Some(input_tokens)` on success, `None` on any error (matching TS behavior)
pub async fn count_messages_tokens_with_api(
    api_key: Option<String>,
    base_url: Option<String>,
    model: &str,
    messages: &[serde_json::Value],
    tools: Option<&[serde_json::Value]>,
    betas: Option<&[String]>,
) -> Option<u64> {
    let api_key = api_key.or_else(get_api_key)?;
    let base_url = base_url.or_else(|| Some(get_base_url()))?;
    let client = reqwest::Client::new();

    // Build request body
    let contains_thinking = has_thinking_blocks(messages);
    let messages_to_send: Vec<serde_json::Value> = if messages.is_empty() {
        // When we pass tools and no messages, we need a dummy message
        vec![serde_json::json!({ "role": "user", "content": "foo" })]
    } else {
        messages.to_vec()
    };
    let mut body = serde_json::json!({
        "model": normalize_model_string_for_api(model),
        "messages": messages_to_send
    });

    // Add tools if provided
    if let Some(tools_list) = tools {
        if !tools_list.is_empty() {
            body["tools"] = serde_json::json!(tools_list);
        }
    }

    // Add betas (filter for Vertex if needed)
    if let Some(betas_list) = betas {
        let filtered = if is_using_vertex() {
            let allowed = crate::constants::betas::get_vertex_count_tokens_allowed_betas();
            betas_list
                .iter()
                .filter(|b| allowed.contains(b.as_str()))
                .cloned()
                .collect::<Vec<String>>()
        } else {
            betas_list.to_vec()
        };
        if !filtered.is_empty() {
            body["betas"] = serde_json::json!(filtered);
        }
    }

    // Enable thinking if messages contain thinking blocks
    if contains_thinking {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": TOKEN_COUNT_THINKING_BUDGET
        });
        body["max_tokens"] = serde_json::json!(TOKEN_COUNT_MAX_TOKENS);
    }

    let url = format!("{}/v1/messages/count_tokens", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            log::debug!("count_tokens API request failed: {}", e);
            return None;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        log::debug!("count_tokens API error {}: {}", status, body_text);
        return None;
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            log::debug!("count_tokens failed to parse response: {}", e);
            return None;
        }
    };

    json.get("input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            // Vertex / Bedrock may return different shapes
            log::debug!("count_tokens response missing input_tokens field: {}", json);
            None
        })
}

/// Convenience wrapper: count tokens for a single text content string.
///
/// Matches TypeScript: `countTokensWithAPI(content)`
///
/// # Arguments
/// * `content` - The text content to count
/// * `api_key` - API key (or None to read from env)
/// * `base_url` - Base API URL (or None to read from env)
/// * `model` - The model to use for counting
///
/// # Returns
/// `Some(tokens)` on success, `None` on error. Returns `Some(0)` for empty content.
pub async fn count_tokens_with_api(
    content: &str,
    api_key: Option<String>,
    base_url: Option<String>,
    model: &str,
) -> Option<u64> {
    // API doesn't accept empty messages
    if content.is_empty() {
        return Some(0);
    }

    let message = serde_json::json!({
        "role": "user",
        "content": content
    });

    count_messages_tokens_with_api(api_key, base_url, model, &[message], None, None).await
}

/// Fallback token counting via a real `messages.create` call with a fast (Haiku) model.
///
/// Matches TypeScript: `countTokensViaHaikuFallback(messages, tools)`
///
/// Makes an actual API call with `max_tokens: 1` (or TOKEN_COUNT_MAX_TOKENS if thinking
/// is needed) and reads the `usage.input_tokens` from the response.
///
/// # Returns
/// `Some(input_tokens)` on success, `None` on error.
pub async fn count_tokens_via_haiku_fallback(
    api_key: Option<String>,
    base_url: Option<String>,
    messages: &[serde_json::Value],
    tools: Option<&[serde_json::Value]>,
) -> Option<u64> {
    let api_key = api_key.or_else(get_api_key)?;
    let base_url = base_url.or_else(|| Some(get_base_url()))?;
    let client = reqwest::Client::new();

    let contains_thinking = has_thinking_blocks(messages);

    // Use Haiku for token counting by default (faster / cheaper).
    // Use Sonnet when:
    // - Vertex global endpoint (Haiku not available there)
    // - Bedrock with thinking blocks (Haiku 3.5 doesn't support thinking on Bedrock)
    // - Vertex with thinking blocks (Haiku 3.5 doesn't support thinking on Vertex)
    // Note: Haiku 4.5 supports thinking blocks on Anthropic API directly.
    let model = if is_vertex_global_endpoint()
        || (is_using_bedrock() && contains_thinking)
        || (is_using_vertex() && contains_thinking)
    {
        crate::utils::model::get_default_sonnet_model()
    } else {
        crate::utils::model::get_small_fast_model()
    };

    // Strip tool search-specific fields (caller, tool_reference) before sending
    // These fields are only valid with the tool search beta header
    let normalized_messages = if messages.is_empty() {
        vec![serde_json::json!({ "role": "user", "content": "count" })]
    } else {
        strip_tool_search_fields_from_messages(messages)
    };
    let mut body = serde_json::json!({
        "model": normalize_model_string_for_api(&model),
        "max_tokens": if contains_thinking { TOKEN_COUNT_MAX_TOKENS } else { 1 },
        "messages": normalized_messages
    });

    // Add tools if provided
    if let Some(tools_list) = tools {
        if !tools_list.is_empty() {
            body["tools"] = serde_json::json!(tools_list);
        }
    }

    // Enable thinking if messages contain thinking blocks
    if contains_thinking {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": TOKEN_COUNT_THINKING_BUDGET
        });
    }

    // Add model-specific betas (filter for Vertex if needed)
    let mut betas = get_model_betas(&model);
    if is_using_vertex() && !betas.is_empty() {
        let allowed = crate::constants::betas::get_vertex_count_tokens_allowed_betas();
        betas.retain(|b| allowed.contains(b.as_str()));
    }
    if !betas.is_empty() {
        body["betas"] = serde_json::json!(betas);
    }

    // Add extra body parameters from AI_CODE_EXTRA_BODY env var
    if let Some(extra) = get_extra_body_params() {
        if let Some(extra_obj) = extra.as_object() {
            for (k, v) in extra_obj {
                body[k] = v.clone();
            }
        }
    }

    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            log::debug!("count_tokens Haiku fallback request failed: {}", e);
            return None;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        log::debug!("count_tokens Haiku fallback error {}: {}", status, body_text);
        return None;
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            log::debug!("count_tokens Haiku fallback parse error: {}", e);
            return None;
        }
    };

    // Extract usage: input_tokens + cache_creation + cache_read
    let usage = json.get("usage")?;
    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Some(input_tokens + cache_creation + cache_read)
}

/// Orchestrator: try API count_tokens first, fall back to Haiku if it fails.
///
/// Matches TypeScript: `countTokensWithFallback(messages, tools)` from analyzeContext.ts
///
/// # Arguments
/// * `api_key` - API key (or None to read from env)
/// * `base_url` - Base API URL (or None to read from env)
/// * `model` - The model to use for counting (primary API call)
/// * `messages` - Messages in API format
/// * `tools` - Optional tool definitions in API format
///
/// # Returns
/// `Some(input_tokens)` on success, `None` if both API and fallback fail.
pub async fn count_tokens_with_fallback(
    api_key: Option<String>,
    base_url: Option<String>,
    model: &str,
    messages: &[serde_json::Value],
    tools: Option<&[serde_json::Value]>,
) -> Option<u64> {
    // Try primary count_tokens API first
    let betas = get_model_betas(model);
    if let Some(count) = count_messages_tokens_with_api(api_key.clone(), base_url.clone(), model, messages, tools, Some(&betas)).await {
        return Some(count);
    }
    log::debug!(
        "count_tokens API returned null, trying Haiku fallback ({} tools)",
        tools.map(|t| t.len()).unwrap_or(0)
    );

    // Haiku fallback
    if let Some(count) = count_tokens_via_haiku_fallback(api_key, base_url, messages, tools).await {
        return Some(count);
    }
    log::debug!("count_tokens Haiku fallback also returned null");
    None
}

// ============================================================================
// FileReadTool token budget validation
// Translated from TypeScript validateContentTokens
// ============================================================================

/// Maximum token limit for file read tool output
pub const DEFAULT_FILE_READ_MAX_TOKENS: u64 = 25_000;

/// Error thrown when file content exceeds token budget
#[derive(Debug, Clone)]
pub struct MaxFileReadTokenExceededError {
    pub token_count: u64,
    pub max_tokens: u64,
}

impl std::fmt::Display for MaxFileReadTokenExceededError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "File content ({} tokens) exceeds maximum allowed tokens ({}). Use offset and limit parameters to read specific portions of the file, or search for specific content instead of reading the whole file.",
            self.token_count, self.max_tokens
        )
    }
}

impl std::error::Error for MaxFileReadTokenExceededError {}

/// Get the default file reading max tokens limit from environment or default.
/// Matches TypeScript: `getDefaultFileReadingLimits().maxTokens`
pub fn get_default_file_read_max_tokens() -> u64 {
    std::env::var("AI_CODE_FILE_READ_MAX_OUTPUT_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_FILE_READ_MAX_TOKENS)
}

/// Validate that file content does not exceed the token budget.
///
/// Two-phase approach matching TypeScript:
/// 1. Cheap rough estimate — if under `max_tokens / 4`, short-circuit and return OK
/// 2. If rough estimate exceeds threshold, call count_tokens API for exact count
/// 3. Throw if exact count exceeds limit
///
/// # Arguments
/// * `content` - The file content to validate
/// * `ext` - File extension (for bytes-per-token ratio)
/// * `max_tokens` - Maximum allowed tokens (or None for default limit)
/// * `api_key` - API key for exact counting (or None to read from env)
/// * `base_url` - Base API URL (or None to read from env)
/// * `model` - Model for count_tokens API call
pub async fn validate_content_tokens(
    content: &str,
    ext: &str,
    max_tokens: Option<u64>,
    api_key: Option<String>,
    base_url: Option<String>,
    model: &str,
) -> Result<(), MaxFileReadTokenExceededError> {
    let effective_max = max_tokens.unwrap_or(get_default_file_read_max_tokens());

    // Phase 1: cheap rough estimate
    let rough_estimate = rough_token_count_estimation_for_file_type(content, ext) as u64;
    if rough_estimate <= effective_max / 4 {
        return Ok(());
    }

    // Phase 2: API-based exact count
    let exact_count = count_tokens_with_api(content, api_key, base_url, model).await;
    let effective_count = exact_count.unwrap_or(rough_estimate);

    if effective_count > effective_max {
        Err(MaxFileReadTokenExceededError {
            token_count: effective_count,
            max_tokens: effective_max,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageRole;

    // ============================================================================
    // Tests for the translated TypeScript functions
    // ============================================================================

    #[test]
    fn test_rough_token_count_estimation() {
        // "Hello world" = 11 chars, 11/4 = 2.75 rounds to 3
        assert_eq!(rough_token_count_estimation("Hello world", 4.0), 3);
        // 100 chars / 4 = 25 tokens
        assert_eq!(rough_token_count_estimation(&"a".repeat(100), 4.0), 25);
    }

    #[test]
    fn test_bytes_per_token_for_file_type() {
        assert_eq!(bytes_per_token_for_file_type("json"), 2.0);
        assert_eq!(bytes_per_token_for_file_type("jsonl"), 2.0);
        assert_eq!(bytes_per_token_for_file_type("rs"), 4.0);
        assert_eq!(bytes_per_token_for_file_type("txt"), 4.0);
    }

    #[test]
    fn test_rough_token_count_estimation_for_file_type() {
        // JSON: 100 chars / 2 = 50 tokens
        assert_eq!(
            rough_token_count_estimation_for_file_type(&"a".repeat(100), "json"),
            50
        );
        // Rust: 100 chars / 4 = 25 tokens
        assert_eq!(
            rough_token_count_estimation_for_file_type(&"a".repeat(100), "rs"),
            25
        );
    }

    #[test]
    fn test_rough_token_count_estimation_for_content() {
        assert_eq!(rough_token_count_estimation_for_content(""), 0);
        // "Hello" = 5 chars, 5/4 = 1.25 rounds to 1
        assert_eq!(rough_token_count_estimation_for_content("Hello"), 1);
    }

    #[test]
    fn test_rough_token_count_estimation_for_message() {
        let msg = crate::types::Message {
            role: MessageRole::User,
            content: "Hello world".to_string(),
            ..Default::default()
        };
        // "Hello world" = 11 chars, 11/4 = 2.75 rounds to 3
        assert_eq!(rough_token_count_estimation_for_message(&msg), 3);
    }

    #[test]
    fn test_rough_token_count_estimation_for_messages() {
        let messages = vec![
            crate::types::Message {
                role: MessageRole::User,
                content: "Hello".to_string(),
                ..Default::default()
            },
            crate::types::Message {
                role: MessageRole::Assistant,
                content: "Hi there".to_string(),
                ..Default::default()
            },
        ];
        // "Hello" = 5 chars / 4 = 1.25 -> 1 token
        // "Hi there" = 8 chars / 4 = 2 tokens
        // Total = 3 tokens
        assert_eq!(rough_token_count_estimation_for_messages(&messages), 3);
    }

    // ============================================================================
    // Tests for legacy estimation functions
    // ============================================================================

    #[test]
    fn test_estimate_tokens_characters() {
        let result = estimate_tokens_characters("Hello, world!");
        assert!(result.tokens >= 3);
        assert_eq!(result.characters, 13);
    }

    #[test]
    fn test_estimate_tokens_words() {
        let result = estimate_tokens_words("Hello world this is a test");
        assert!(result.tokens > 0);
        assert_eq!(result.words, 6);
    }

    #[test]
    fn test_estimate_tokens() {
        let result = estimate_tokens("The quick brown fox jumps over the lazy dog");
        assert!(result.tokens > 0);
    }

    #[test]
    fn test_estimate_conversation() {
        let conv = "User: Hello\nAssistant: Hi there!\nUser: How are you?";
        let result = estimate_conversation(conv);
        assert!(result.tokens > 0);
    }

    #[test]
    fn test_estimate_tool_definitions() {
        let tools = vec![ToolDefinition {
            name: "Read".to_string(),
            description: Some("Read a file".to_string()),
            input_schema: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#
                .to_string(),
        }];
        let tokens = estimate_tool_definitions(&tools);
        assert!(tokens > 0);
    }

    #[test]
    fn test_calculate_padding() {
        assert_eq!(calculate_padding(1000, 500, 2000), 500);
        assert_eq!(calculate_padding(1500, 500, 2000), 0);
    }

    #[test]
    fn test_fits_in_context() {
        assert!(fits_in_context(1000, 500, 2000));
        assert!(!fits_in_context(1600, 500, 2000));
    }

    #[test]
    fn test_encoding_chars_per_token() {
        assert_eq!(
            encoding::chars_per_token("Hello world"),
            encoding::CHARS_PER_TOKEN_EN
        );
        assert_eq!(
            encoding::chars_per_token("function test() {}"),
            encoding::CHARS_PER_TOKEN_CODE
        );
    }

    #[test]
    fn test_is_code() {
        assert!(encoding::is_code("function foo() { return 1; }"));
        assert!(!encoding::is_code("Hello world"));
    }

    #[test]
    fn test_is_cjk() {
        assert!(encoding::is_cjk("你好世界"));
        assert!(!encoding::is_cjk("Hello world"));
    }

    #[test]
    fn test_message_content_trait() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        assert_eq!(msg.content(), "Hello");
    }

    // ============================================================================
    // Tests for count_tokens API helpers
    // ============================================================================

    #[test]
    fn test_has_thinking_blocks_detects_thinking() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [
                { "type": "thinking", "thinking": "let me think..." },
                { "type": "text", "text": "I think the answer is 42" }
            ]
        })];
        assert!(has_thinking_blocks(&messages));
    }

    #[test]
    fn test_has_thinking_blocks_detects_redacted_thinking() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [
                { "type": "redacted_thinking", "data": "xxx" }
            ]
        })];
        assert!(has_thinking_blocks(&messages));
    }

    #[test]
    fn test_has_thinking_blocks_no_thinking() {
        let messages = vec![
            serde_json::json!({ "role": "user", "content": "Hello" }),
            serde_json::json!({ "role": "assistant", "content": "Hi there" }),
        ];
        assert!(!has_thinking_blocks(&messages));
    }

    #[test]
    fn test_has_thinking_blocks_empty() {
        let messages: Vec<serde_json::Value> = vec![];
        assert!(!has_thinking_blocks(&messages));
    }

    #[test]
    fn test_has_thinking_blocks_tool_use_only() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [
                { "type": "tool_use", "id": "tool_1", "name": "Read", "input": {} }
            ]
        })];
        assert!(!has_thinking_blocks(&messages));
    }

    #[test]
    fn test_normalize_model_string_for_api() {
        // Strip claude/ prefix
        assert_eq!(normalize_model_string_for_api("claude/sonnet-4-6"), "sonnet-4-6");
        // Pass through model without prefix
        assert_eq!(
            normalize_model_string_for_api("claude-sonnet-4-6"),
            "claude-sonnet-4-6"
        );
        // Strip context window indicators
        assert_eq!(
            normalize_model_string_for_api("claude-sonnet-4-6[1m]"),
            "claude-sonnet-4-6"
        );
        assert_eq!(
            normalize_model_string_for_api("claude-opus-4-7[2m]"),
            "claude-opus-4-7"
        );
    }

    #[test]
    fn test_token_count_constants() {
        // max_tokens must be greater than thinking budget
        assert!(TOKEN_COUNT_MAX_TOKENS > TOKEN_COUNT_THINKING_BUDGET);
        assert_eq!(TOKEN_COUNT_THINKING_BUDGET, 1024);
        assert_eq!(TOKEN_COUNT_MAX_TOKENS, 2048);
    }

    #[test]
    fn test_default_file_read_max_tokens() {
        assert_eq!(get_default_file_read_max_tokens(), 25_000);
    }

    #[test]
    fn test_max_file_read_error_display() {
        let err = MaxFileReadTokenExceededError {
            token_count: 30_000,
            max_tokens: 25_000,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("30000"));
        assert!(msg.contains("25000"));
        assert!(msg.contains("tokens"));
    }

    #[tokio::test]
    async fn test_validate_content_tokens_short_content() {
        // Content under max_tokens / 4 → should pass without API call
        let result = validate_content_tokens(
            "short content",
            "txt",
            Some(25_000),
            None, // no API key
            None,
            "claude-sonnet-4-6",
        )
        .await;
        assert!(result.is_ok());
    }

    // ============================================================================
    // Tests for tool search field stripping
    // ============================================================================

    #[test]
    fn test_strip_tool_search_fields_removes_caller_from_tool_use() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "tool_1",
                    "name": "Read",
                    "input": { "file_path": "/test.txt" },
                    "caller": "some_agent"
                }
            ]
        })];
        let result = strip_tool_search_fields_from_messages(&messages);
        let tool_use = &result[0]["content"][0];
        assert_eq!(tool_use.get("caller"), None);
        assert_eq!(tool_use["type"], "tool_use");
        assert_eq!(tool_use["id"], "tool_1");
        assert_eq!(tool_use["name"], "Read");
    }

    #[test]
    fn test_strip_tool_search_fields_removes_tool_reference() {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": "tool_1",
                    "content": [
                        { "type": "tool_reference", "tool_use_id": "tool_1" },
                        { "type": "text", "text": "real result" }
                    ]
                }
            ]
        })];
        let result = strip_tool_search_fields_from_messages(&messages);
        let content = &result[0]["content"][0]["content"];
        assert_eq!(content.as_array().unwrap().len(), 1);
        assert_eq!(content[0]["type"], "text");
    }

    #[test]
    fn test_strip_tool_search_fields_all_references_replaced() {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": "tool_1",
                    "content": [
                        { "type": "tool_reference", "tool_use_id": "tool_1" }
                    ]
                }
            ]
        })];
        let result = strip_tool_search_fields_from_messages(&messages);
        let content = &result[0]["content"][0]["content"];
        assert_eq!(content.as_array().unwrap().len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "[tool references]");
    }

    #[test]
    fn test_strip_tool_search_fields_passthrough() {
        let messages = vec![
            serde_json::json!({ "role": "user", "content": "Hello" }),
            serde_json::json!({ "role": "assistant", "content": [{ "type": "text", "text": "Hi" }] }),
        ];
        let result = strip_tool_search_fields_from_messages(&messages);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["content"], "Hello");
    }

    // ============================================================================
    // Tests for content block estimation
    // ============================================================================

    #[test]
    fn test_rough_token_count_estimation_for_block_text() {
        let block = serde_json::json!({ "type": "text", "text": "Hello world" });
        assert_eq!(rough_token_count_estimation_for_block(&block), 3);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_image() {
        let block = serde_json::json!({ "type": "image", "source": {} });
        assert_eq!(rough_token_count_estimation_for_block(&block), 2000);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_document() {
        let block = serde_json::json!({ "type": "document", "source": {} });
        assert_eq!(rough_token_count_estimation_for_block(&block), 2000);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_tool_use() {
        let block = serde_json::json!({
            "type": "tool_use",
            "name": "Read",
            "input": { "file_path": "/test.txt" }
        });
        let tokens = rough_token_count_estimation_for_block(&block);
        // "Read" + '{"file_path":"/test.txt"}' = ~30 chars / 4 = ~8 tokens
        assert!(tokens > 0);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_thinking() {
        let block = serde_json::json!({
            "type": "thinking",
            "thinking": "Let me think about this carefully"
        });
        let tokens = rough_token_count_estimation_for_block(&block);
        assert!(tokens > 0);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_redacted_thinking() {
        let block = serde_json::json!({
            "type": "redacted_thinking",
            "data": "xxx"
        });
        assert_eq!(rough_token_count_estimation_for_block(&block), 1);
    }

    #[test]
    fn test_rough_token_count_estimation_for_block_fallback() {
        let block = serde_json::json!({
            "type": "server_tool_use",
            "name": "magic",
            "input": {}
        });
        let tokens = rough_token_count_estimation_for_block(&block);
        assert!(tokens > 0);
    }

    #[test]
    fn test_rough_token_count_estimation_for_content_array_string() {
        let content = serde_json::json!("Hello world");
        assert_eq!(rough_token_count_estimation_for_content_array(&content), 3);
    }

    #[test]
    fn test_rough_token_count_estimation_for_content_array_blocks() {
        let content = serde_json::json!([
            { "type": "text", "text": "Hello" },
            { "type": "text", "text": " world" }
        ]);
        let tokens = rough_token_count_estimation_for_content_array(&content);
        // "Hello" = 5/4 = 1, " world" = 6/4 = 2 → 3
        assert_eq!(tokens, 3);
    }

    // ============================================================================
    // Tests for get_model_betas
    // ============================================================================

    #[test]
    fn test_get_model_betas_non_haiku() {
        let betas = get_model_betas("claude-sonnet-4-6");
        assert!(betas.contains(&crate::constants::betas::CLAUDE_CODE_20250219_BETA_HEADER.to_string()));
    }

    #[test]
    fn test_get_model_betas_haiku_no_code_beta() {
        let betas = get_model_betas("claude-haiku-4-5");
        assert!(!betas.contains(&crate::constants::betas::CLAUDE_CODE_20250219_BETA_HEADER.to_string()));
    }

    #[test]
    fn test_get_model_betas_context_1m() {
        let betas = get_model_betas("claude-sonnet-4-6[1m]");
        assert!(betas.contains(&crate::constants::betas::CONTEXT_1M_BETA_HEADER.to_string()));
    }

    // ============================================================================
    // Tests for get_extra_body_params
    // ============================================================================

    #[test]
    fn test_get_extra_body_params_not_set() {
        // AI_CODE_EXTRA_BODY not set → returns None
        let result = get_extra_body_params();
        assert!(result.is_none());
    }
}
