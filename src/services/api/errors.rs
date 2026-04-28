// Source: /data/home/swei/claudecode/openclaudecode/src/utils/errors.ts
//! API error types and utilities translated from TypeScript errors.ts

/// Prefix for API error messages
pub const API_ERROR_MESSAGE_PREFIX: &str = "API Error";

/// Sanitize an HTTP error body: if it looks like an HTML error page
/// (e.g., 502/503 from a reverse proxy), extract the page title instead
/// of dumping the full HTML into the error message.
pub fn sanitize_html_error(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("<!doctype html") || lower.contains("<html") {
        // Try to extract the <title>...</title> content
        if let Some(title_start) = text.find("<title>") {
            let after_start = &text[title_start + "<title>".len()..];
            if let Some(title_end) = after_start.find("</title>") {
                let title = after_start[..title_end].trim().to_string();
                if !title.is_empty() {
                    return title;
                }
            }
        }
        // No title found; just drop the HTML entirely
        String::new()
    } else {
        text.to_string()
    }
}

/// Check if text starts with API error prefix
pub fn starts_with_api_error_prefix(text: &str) -> bool {
    text.starts_with(API_ERROR_MESSAGE_PREFIX)
        || text.starts_with(&format!("Please run /login · {}", API_ERROR_MESSAGE_PREFIX))
}

/// Prompt too long error message
pub const PROMPT_TOO_LONG_ERROR_MESSAGE: &str = "Prompt is too long";

/// Check if a message is a prompt too long error
pub fn is_prompt_too_long_message(msg: &ApiErrorMessage) -> bool {
    if !msg.is_api_error_message {
        return false;
    }

    let content = match &msg.content {
        Some(c) => c,
        None => return false,
    };

    // Check if content starts with the prompt too long message
    content.starts_with(PROMPT_TOO_LONG_ERROR_MESSAGE)
}

/// Parse actual/limit token counts from a raw prompt-too-long API error
/// message like "prompt is too long: 137500 tokens > 135000 maximum".
/// The raw string may be wrapped in SDK prefixes or JSON envelopes, or
/// have different casing (Vertex), so this is intentionally lenient.
pub fn parse_prompt_too_long_token_counts(raw_message: &str) -> (Option<u64>, Option<u64>) {
    // Regex: prompt is too long followed by any non-digits, then digits, "tokens", >, digits
    // Using simple parsing instead of regex for no_std compatibility
    let lower = raw_message.to_lowercase();

    if !lower.contains("prompt is too long") {
        return (None, None);
    }

    // Find all numbers in the message
    let mut numbers: Vec<u64> = Vec::new();
    let mut current_num = String::new();

    for c in raw_message.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else if !current_num.is_empty() {
            if let Ok(n) = current_num.parse() {
                numbers.push(n);
            }
            current_num.clear();
        }
    }

    // Don't forget the last number if string ends with digits
    if !current_num.is_empty() {
        if let Ok(n) = current_num.parse() {
            numbers.push(n);
        }
    }

    // We expect at least 2 numbers: actual and limit
    if numbers.len() >= 2 {
        // The larger number is likely the actual, smaller is limit
        // But let's be smarter: look for ">" which indicates actual > limit
        if let Some(gt_pos) = raw_message.find('>') {
            let before_gt = &raw_message[..gt_pos];
            let after_gt = &raw_message[gt_pos..];

            // Extract numbers before and after >
            let mut before_nums: Vec<u64> = Vec::new();
            let mut after_nums: Vec<u64> = Vec::new();

            let mut current = String::new();
            for c in before_gt.chars().rev() {
                if c.is_ascii_digit() {
                    current.push(c);
                } else if !current.is_empty() {
                    if let Ok(n) = current.chars().rev().collect::<String>().parse() {
                        before_nums.push(n);
                    }
                    current.clear();
                }
            }

            current.clear();
            for c in after_gt.chars() {
                if c.is_ascii_digit() {
                    current.push(c);
                } else if !current.is_empty() {
                    if let Ok(n) = current.parse() {
                        after_nums.push(n);
                    }
                    current.clear();
                }
            }

            if let (Some(actual), Some(limit)) = (before_nums.first(), after_nums.first()) {
                return (Some(*actual), Some(*limit));
            }
        }
    }

    // Fallback: just take first two numbers if available
    if numbers.len() >= 2 {
        return (Some(numbers[0]), Some(numbers[1]));
    }

    (None, None)
}

/// Returns how many tokens over the limit a prompt-too-long error reports,
/// or undefined if the message isn't PTL or its error_details are unparseable.
pub fn get_prompt_too_long_token_gap(msg: &ApiErrorMessage) -> Option<i64> {
    if !is_prompt_too_long_message(msg) {
        return None;
    }

    let error_details = msg.error_details.as_ref()?;

    let (actual_tokens, limit_tokens) = parse_prompt_too_long_token_counts(error_details);

    let actual = actual_tokens?;
    let limit = limit_tokens?;

    let gap = actual as i64 - limit as i64;
    if gap > 0 { Some(gap) } else { None }
}

/// Is this raw API error text a media-size rejection?
/// Patterns MUST stay in sync with the getAssistantMessageFromError branches
/// that populate error_details (~L523 PDF, ~L560 image, ~L573 many-image) and
/// the classifyAPIError branches (~L929-946).
pub fn is_media_size_error(raw: &str) -> bool {
    let lower = raw.to_lowercase();

    (lower.contains("image exceeds") && lower.contains("maximum"))
        || (lower.contains("image dimensions exceed") && lower.contains("many-image"))
        // Use original string for regex (case-sensitive), like TypeScript
        || regex::Regex::new(r"maximum of \d+ PDF pages")
            .map(|re| re.is_match(raw))
            .unwrap_or(false)
}

/// Message-level predicate: is this assistant message a media-size rejection?
pub fn is_media_size_error_message(msg: &ApiErrorMessage) -> bool {
    msg.is_api_error_message
        && msg
            .error_details
            .as_ref()
            .map(|d| is_media_size_error(d))
            .unwrap_or(false)
}

/// Credit balance too low error message
pub const CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE: &str = "Credit balance is too low";

/// Invalid API key error message
pub const INVALID_API_KEY_ERROR_MESSAGE: &str = "Not logged in · Please run /login";

/// Invalid API key error message for external sources
pub const INVALID_API_KEY_ERROR_MESSAGE_EXTERNAL: &str = "Invalid API key · Fix external API key";

/// Organization disabled error message (env key with OAuth)
pub const ORG_DISABLED_ERROR_MESSAGE_ENV_KEY_WITH_OAUTH: &str = "Your ANTHROPIC_API_KEY belongs to a disabled organization · Unset the environment variable to use your subscription instead";

/// Organization disabled error message (env key)
pub const ORG_DISABLED_ERROR_MESSAGE_ENV_KEY: &str = "Your ANTHROPIC_API_KEY belongs to a disabled organization · Update or unset the environment variable";

/// Token revoked error message
pub const TOKEN_REVOKED_ERROR_MESSAGE: &str = "OAuth token revoked · Please run /login";

/// CCR auth error message
pub const CCR_AUTH_ERROR_MESSAGE: &str =
    "Authentication error · This may be a temporary network issue, please try again";

/// Repeated 529 error message
pub const REPEATED_529_ERROR_MESSAGE: &str = "Repeated 529 Overloaded errors";

/// Custom off switch message
pub const CUSTOM_OFF_SWITCH_MESSAGE: &str =
    "Opus is experiencing high load, please use /model to switch to Sonnet";

/// API timeout error message
pub const API_TIMEOUT_ERROR_MESSAGE: &str = "Request timed out";

/// Get PDF too large error message based on session type
pub fn get_pdf_too_large_error_message(is_non_interactive: bool) -> String {
    // In a real implementation, API_PDF_MAX_PAGES and PDF_TARGET_RAW_SIZE would be imported
    let limits = "max 1000 pages, 32MB".to_string();

    if is_non_interactive {
        format!(
            "PDF too large ({}). Try reading the file a different way (e.g., extract text with pdftotext).",
            limits
        )
    } else {
        format!(
            "PDF too large ({}). Double press esc to go back and try again, or use pdftotext to convert to text first.",
            limits
        )
    }
}

/// Get PDF password protected error message
pub fn get_pdf_password_protected_error_message(is_non_interactive: bool) -> String {
    if is_non_interactive {
        "PDF is password protected. Try using a CLI tool to extract or convert the PDF.".to_string()
    } else {
        "PDF is password protected. Please double press esc to edit your message and try again."
            .to_string()
    }
}

/// Get PDF invalid error message
pub fn get_pdf_invalid_error_message(is_non_interactive: bool) -> String {
    if is_non_interactive {
        "The PDF file was not valid. Try converting it to a text first (e.g., pdftotext)."
            .to_string()
    } else {
        "The PDF file was not valid. Double press esc to go back and try again with a different file.".to_string()
    }
}

/// Get image too large error message
pub fn get_image_too_large_error_message(is_non_interactive: bool) -> String {
    if is_non_interactive {
        "Image was too large. Try resizing the image or using a different approach.".to_string()
    } else {
        "Image was too large. Double press esc to go back and try again with a smaller image."
            .to_string()
    }
}

/// Get request too large error message
pub fn get_request_too_large_error_message(is_non_interactive: bool) -> String {
    let limits = "max 32MB".to_string();

    if is_non_interactive {
        format!("Request too large ({}). Try with a smaller file.", limits)
    } else {
        format!(
            "Request too large ({}). Double press esc to go back and try with a smaller file.",
            limits
        )
    }
}

/// OAuth org not allowed error message
pub const OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE: &str =
    "Your account does not have access to Claude Code. Please run /login.";

/// Get token revoked error message
pub fn get_token_revoked_error_message(is_non_interactive: bool) -> String {
    if is_non_interactive {
        "Your account does not have access to Claude. Please login again or contact your administrator."
            .to_string()
    } else {
        TOKEN_REVOKED_ERROR_MESSAGE.to_string()
    }
}

/// Get OAuth org not allowed error message
pub fn get_oauth_org_not_allowed_error_message(is_non_interactive: bool) -> String {
    if is_non_interactive {
        "Your organization does not have access to Claude. Please login again or contact your administrator."
            .to_string()
    } else {
        OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE.to_string()
    }
}

/// API error types for classification
#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum ApiErrorType {
    aborted,
    api_timeout,
    repeated_529,
    capacity_off_switch,
    rate_limit,
    server_overload,
    prompt_too_long,
    pdf_too_large,
    pdf_password_protected,
    image_too_large,
    tool_use_mismatch,
    unexpected_tool_result,
    duplicate_tool_use_id,
    invalid_model,
    credit_balance_low,
    invalid_api_key,
    token_revoked,
    oauth_org_not_allowed,
    auth_error,
    bedrock_model_access,
    server_error,
    client_error,
    ssl_cert_error,
    connection_error,
    unknown,
}

/// SDK assistant message error types
#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum SDKAssistantMessageError {
    rate_limit,
    authentication_failed,
    server_error,
    unknown,
}

/// Assistant message structure for API errors
#[derive(Debug, Clone)]
pub struct ApiErrorMessage {
    pub is_api_error_message: bool,
    pub content: Option<String>,
    pub error: Option<String>,
    pub error_details: Option<String>,
}

impl Default for ApiErrorMessage {
    fn default() -> Self {
        Self {
            is_api_error_message: true,
            content: Some(API_ERROR_MESSAGE_PREFIX.to_string()),
            error: Some("unknown".to_string()),
            error_details: None,
        }
    }
}

/// Create an assistant API error message
pub fn create_assistant_api_error_message(content: &str) -> ApiErrorMessage {
    ApiErrorMessage {
        is_api_error_message: true,
        content: Some(content.to_string()),
        error: Some("unknown".to_string()),
        error_details: None,
    }
}

/// Create an assistant API error message with optional parameters
pub fn create_assistant_api_error_message_with_options(
    content: &str,
    error: Option<&str>,
    error_details: Option<&str>,
) -> ApiErrorMessage {
    ApiErrorMessage {
        is_api_error_message: true,
        content: Some(content.to_string()),
        error: error.map(String::from),
        error_details: error_details.map(String::from),
    }
}

/// Check if we're in CCR (Claude Code Remote) mode.
/// In CCR mode, auth is handled via JWTs provided by the infrastructure,
/// not via /login. Transient auth errors should suggest retrying, not logging in.
/// Note: This is a placeholder - actual implementation would check environment
pub fn is_ccr_mode() -> bool {
    // Would check process.env.CLAUDE_CODE_REMOTE
    false
}

/// Type guard to check if a value is a valid API message response
pub fn is_valid_api_message(value: &serde_json::Value) -> bool {
    value.get("content").is_some()
        && value.get("model").is_some()
        && value.get("usage").is_some()
        && value["content"].is_array()
        && value["model"].is_string()
        && value["usage"].is_object()
}

/// Lower-level error that AWS can return
#[derive(Debug, Clone, Default)]
pub struct AmazonError {
    pub output: Option<AmazonOutput>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AmazonOutput {
    pub type_: Option<String>,
}

impl AmazonError {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        let output = value.get("Output")?;
        let output_type = output
            .get("__type")
            .and_then(|v| v.as_str())
            .map(String::from);

        Some(AmazonError {
            output: Some(AmazonOutput { type_: output_type }),
            version: value
                .get("Version")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    }
}

/// Given a response that doesn't look quite right, see if it contains any known error types
pub fn extract_unknown_error_format(value: &serde_json::Value) -> Option<String> {
    // Check if value is a valid object first
    if !value.is_object() {
        return None;
    }

    // Amazon Bedrock routing errors
    if let Some(output) = value.get("Output") {
        if let Some(output_type) = output.get("__type").and_then(|v| v.as_str()) {
            return Some(output_type.to_string());
        }
    }

    None
}

/// Classifies an API error into a specific error type for analytics tracking.
/// Returns a standardized error type string suitable for tagging.
pub fn classify_api_error(error_message: &str, status: Option<u16>) -> ApiErrorType {
    let lower = error_message.to_lowercase();

    // Aborted requests
    if error_message == "Request was aborted." {
        return ApiErrorType::aborted;
    }

    // Timeout errors
    if lower.contains("timeout") {
        return ApiErrorType::api_timeout;
    }

    // Check for repeated 529 errors
    if error_message.contains(REPEATED_529_ERROR_MESSAGE) {
        return ApiErrorType::repeated_529;
    }

    // Check for emergency capacity off switch
    if error_message.contains(CUSTOM_OFF_SWITCH_MESSAGE) {
        return ApiErrorType::capacity_off_switch;
    }

    // Rate limiting
    if status == Some(429) {
        return ApiErrorType::rate_limit;
    }

    // Server overload (529)
    if status == Some(529) || error_message.contains(r#""type":"overloaded_error""#) {
        return ApiErrorType::server_overload;
    }

    // Prompt/content size errors
    if lower.contains(&PROMPT_TOO_LONG_ERROR_MESSAGE.to_lowercase()) {
        return ApiErrorType::prompt_too_long;
    }

    // PDF errors
    if is_media_size_error(error_message) && error_message.to_lowercase().contains("pdf") {
        if error_message.to_lowercase().contains("password") {
            return ApiErrorType::pdf_password_protected;
        }
        return ApiErrorType::pdf_too_large;
    }

    // Image size errors
    if status == Some(400)
        && lower.contains("image")
        && lower.contains("exceeds")
        && lower.contains("maximum")
    {
        return ApiErrorType::image_too_large;
    }

    // Many-image dimension errors
    if status == Some(400)
        && lower.contains("image dimensions exceed")
        && lower.contains("many-image")
    {
        return ApiErrorType::image_too_large;
    }

    // Tool use errors (400)
    if status == Some(400)
        && error_message.contains("`tool_use` ids were found without `tool_result`")
    {
        return ApiErrorType::tool_use_mismatch;
    }

    if status == Some(400)
        && error_message.contains("unexpected `tool_use_id` found in `tool_result`")
    {
        return ApiErrorType::unexpected_tool_result;
    }

    if status == Some(400) && error_message.contains("`tool_use` ids must be unique") {
        return ApiErrorType::duplicate_tool_use_id;
    }

    // Invalid model errors (400)
    if status == Some(400) && lower.contains("invalid model name") {
        return ApiErrorType::invalid_model;
    }

    // Credit/billing errors
    if lower.contains(&CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE.to_lowercase()) {
        return ApiErrorType::credit_balance_low;
    }

    // Authentication errors
    if lower.contains("x-api-key") {
        return ApiErrorType::invalid_api_key;
    }

    if status == Some(403) && error_message.contains("OAuth token has been revoked") {
        return ApiErrorType::token_revoked;
    }

    if (status == Some(401) || status == Some(403))
        && error_message
            .contains("OAuth authentication is currently not allowed for this organization")
    {
        return ApiErrorType::oauth_org_not_allowed;
    }

    // Generic auth errors
    if status == Some(401) || status == Some(403) {
        return ApiErrorType::auth_error;
    }

    // Bedrock-specific errors
    // if is_env_truthy(process.env.CLAUDE_CODE_USE_BEDROCK) && lower.contains("model id") {
    //     return "bedrock_model_access";
    // }

    // Status code based fallbacks
    if let Some(s) = status {
        if s >= 500 {
            return ApiErrorType::server_error;
        }
        if s >= 400 {
            return ApiErrorType::client_error;
        }
    }

    // Connection errors
    if lower.contains("connection") || lower.contains("ssl") || lower.contains("tls") {
        if lower.contains("ssl") || lower.contains("certificate") {
            return ApiErrorType::ssl_cert_error;
        }
        return ApiErrorType::connection_error;
    }

    ApiErrorType::unknown
}

/// Categorize retryable API errors
pub fn categorize_retryable_api_error(status: u16, message: &str) -> SDKAssistantMessageError {
    if status == 529 || message.contains(r#""type":"overloaded_error""#) {
        return SDKAssistantMessageError::rate_limit;
    }
    if status == 429 {
        return SDKAssistantMessageError::rate_limit;
    }
    if status == 401 || status == 403 {
        return SDKAssistantMessageError::authentication_failed;
    }
    if status >= 408 {
        return SDKAssistantMessageError::server_error;
    }
    SDKAssistantMessageError::unknown
}

/// Get error message if refusal
pub fn get_error_message_if_refusal(
    stop_reason: Option<&str>,
    model: &str,
    is_non_interactive: bool,
) -> Option<ApiErrorMessage> {
    if stop_reason != Some("refusal") {
        return None;
    }

    // In a real implementation, this would log an event
    // logEvent('tengu_refusal_api_response', {});

    let base_message = if is_non_interactive {
        format!(
            "{}: Claude Code is unable to respond to this request, which appears to violate our Usage Policy (https://www.anthropic.com/legal/aup). Try rephrasing the request or attempting a different approach.",
            API_ERROR_MESSAGE_PREFIX
        )
    } else {
        format!(
            "{}: Claude Code is unable to respond to this request, which appears to violate our Usage Policy (https://www.anthropic.com/legal/aup). Please double press esc to edit your last message or start a new session for Claude Code to assist with a different task.",
            API_ERROR_MESSAGE_PREFIX
        )
    };

    let model_suggestion = if model != "claude-sonnet-4-20250514" {
        " If you are seeing this refusal repeatedly, try running /model claude-sonnet-4-20250514 to switch models."
    } else {
        ""
    };

    Some(create_assistant_api_error_message_with_options(
        &(base_message + model_suggestion),
        Some("invalid_request"),
        None,
    ))
}

/// Constant for no response requested
pub const NO_RESPONSE_REQUESTED: &str = "NO_RESPONSE_REQUESTED";

/// Map a raw error message to a rich `ApiErrorMessage` with structured content,
/// error type, and optional error_details. Mirrors the TypeScript
/// `getAssistantMessageFromError()` function.
pub fn error_to_api_message(error_msg: &str, status: Option<u16>) -> ApiErrorMessage {
    let lower = error_msg.to_lowercase();

    // Aborted requests
    if error_msg == "Request was aborted." || error_msg == "User aborted the request" {
        return create_assistant_api_error_message_with_options(
            "Request was aborted",
            Some("aborted"),
            Some(error_msg),
        );
    }

    // Timeout errors
    if lower.contains("timeout") || lower.contains("timed out") {
        return create_assistant_api_error_message_with_options(
            API_TIMEOUT_ERROR_MESSAGE,
            Some("unknown"),
            Some(error_msg),
        );
    }

    // Repeated 529 errors
    if error_msg.contains(REPEATED_529_ERROR_MESSAGE) {
        return create_assistant_api_error_message_with_options(
            REPEATED_529_ERROR_MESSAGE,
            Some("server_overload"),
            Some(error_msg),
        );
    }

    // Rate limiting (429)
    if status == Some(429) || lower.contains("rate_limit") || lower.contains("rate limit") {
        return create_assistant_api_error_message_with_options(
            "Rate limit exceeded. Please try again shortly.",
            Some("rate_limit"),
            Some(error_msg),
        );
    }

    // Server overload (529)
    if status == Some(529) || lower.contains("overloaded") {
        return create_assistant_api_error_message_with_options(
            "Server is overloaded. Retrying...",
            Some("server_overload"),
            Some(error_msg),
        );
    }

    // Prompt too long (413)
    if lower.contains(&PROMPT_TOO_LONG_ERROR_MESSAGE.to_lowercase())
        || lower.contains("prompt is too long")
        || (status == Some(413) && lower.contains("too long"))
    {
        return create_assistant_api_error_message_with_options(
            PROMPT_TOO_LONG_ERROR_MESSAGE,
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // PDF errors
    if is_media_size_error(error_msg) && lower.contains("pdf") {
        if lower.contains("password") {
            return create_assistant_api_error_message_with_options(
                &get_pdf_password_protected_error_message(false),
                Some("invalid_request"),
                Some(error_msg),
            );
        }
        return create_assistant_api_error_message_with_options(
            &get_pdf_too_large_error_message(false),
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Image size errors
    if (status == Some(400) && lower.contains("image") && lower.contains("exceeds") && lower.contains("maximum"))
        || (lower.contains("image exceeds") && lower.contains("maximum"))
    {
        return create_assistant_api_error_message_with_options(
            &get_image_too_large_error_message(false),
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Request too large (413)
    if status == Some(413) {
        return create_assistant_api_error_message_with_options(
            &get_request_too_large_error_message(false),
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Tool use mismatch
    if status == Some(400) && error_msg.contains("`tool_use` ids were found without `tool_result`") {
        return create_assistant_api_error_message_with_options(
            &format!("{}: Tool use mismatch. Try /rewind to fix.", API_ERROR_MESSAGE_PREFIX),
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Duplicate tool use ID
    if status == Some(400) && error_msg.contains("`tool_use` ids must be unique") {
        return create_assistant_api_error_message_with_options(
            &format!("{}: Duplicate tool use ID. Try /rewind to fix.", API_ERROR_MESSAGE_PREFIX),
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Invalid model
    if status == Some(400) && lower.contains("invalid model") {
        return create_assistant_api_error_message_with_options(
            "Model is not available. Try /model to switch.",
            Some("invalid_request"),
            Some(error_msg),
        );
    }

    // Credit balance too low
    if lower.contains(&CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE.to_lowercase()) {
        return create_assistant_api_error_message_with_options(
            CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE,
            Some("billing_error"),
            Some(error_msg),
        );
    }

    // Authentication errors
    if lower.contains("x-api-key") || lower.contains("api key") && status == Some(401) {
        return create_assistant_api_error_message_with_options(
            INVALID_API_KEY_ERROR_MESSAGE,
            Some("authentication_failed"),
            Some(error_msg),
        );
    }

    // Token revoked
    if status == Some(403) && error_msg.contains("OAuth token has been revoked") {
        return create_assistant_api_error_message_with_options(
            TOKEN_REVOKED_ERROR_MESSAGE,
            Some("authentication_failed"),
            Some(error_msg),
        );
    }

    // OAuth org not allowed
    if (status == Some(401) || status == Some(403))
        && error_msg.contains("OAuth authentication is currently not allowed for this organization")
    {
        return create_assistant_api_error_message_with_options(
            OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE,
            Some("authentication_failed"),
            Some(error_msg),
        );
    }

    // Generic auth errors
    if status == Some(401) || status == Some(403) {
        return create_assistant_api_error_message_with_options(
            "Authentication failed.",
            Some("authentication_failed"),
            Some(error_msg),
        );
    }

    // Connection errors
    if lower.contains("connection") || lower.contains("ssl") || lower.contains("tls") {
        return create_assistant_api_error_message_with_options(
            "Connection error. Check your network and try again.",
            Some("connection_error"),
            Some(error_msg),
        );
    }

    // Generic fallback — use the raw message prefixed with "API Error"
    if lower.starts_with("api error") {
        create_assistant_api_error_message_with_options(error_msg, Some("unknown"), Some(error_msg))
    } else {
        create_assistant_api_error_message_with_options(
            &format!("{}: {}", API_ERROR_MESSAGE_PREFIX, error_msg),
            Some("unknown"),
            Some(error_msg),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_with_api_error_prefix() {
        assert!(starts_with_api_error_prefix(
            "API Error: something went wrong"
        ));
        assert!(starts_with_api_error_prefix(
            "Please run /login · API Error: test"
        ));
        assert!(!starts_with_api_error_prefix("Something else"));
    }

    #[test]
    fn test_is_terminal_task_status() {
        assert!(!is_media_size_error("some random error"));
        assert!(is_media_size_error("image exceeds 5 MB maximum"));
        assert!(is_media_size_error(
            "image dimensions exceed limit for many-image"
        ));
        assert!(is_media_size_error("maximum of 1000 PDF pages"));
    }

    #[test]
    fn test_classify_api_error() {
        assert_eq!(
            classify_api_error("Request was aborted.", None),
            ApiErrorType::aborted
        );
        assert_eq!(
            classify_api_error("timeout error", None),
            ApiErrorType::api_timeout
        );
        assert_eq!(
            classify_api_error("rate limit", Some(429)),
            ApiErrorType::rate_limit
        );
        assert_eq!(
            classify_api_error("server overloaded", Some(529)),
            ApiErrorType::server_overload
        );
        assert_eq!(
            classify_api_error("Prompt is too long", None),
            ApiErrorType::prompt_too_long
        );
    }

    #[test]
    fn test_categorize_retryable_api_error() {
        assert_eq!(
            categorize_retryable_api_error(529, "overloaded"),
            SDKAssistantMessageError::rate_limit
        );
        assert_eq!(
            categorize_retryable_api_error(429, "rate limit"),
            SDKAssistantMessageError::rate_limit
        );
        assert_eq!(
            categorize_retryable_api_error(401, "unauthorized"),
            SDKAssistantMessageError::authentication_failed
        );
        assert_eq!(
            categorize_retryable_api_error(500, "server error"),
            SDKAssistantMessageError::server_error
        );
    }

    #[test]
    fn test_sanitize_html_error() {
        // HTML with title
        let html = "<html><head><title>502 Bad Gateway</title></head><body><p>error</p></body></html>";
        assert_eq!(sanitize_html_error(html), "502 Bad Gateway");

        // HTML without title
        let html_no_title = "<html><body><p>error</p></body></html>";
        assert_eq!(sanitize_html_error(html_no_title), "");

        // DOCTYPE HTML
        let doctype = "<!DOCTYPE html><html><head><title>503 Service Unavailable</title></head>";
        assert_eq!(sanitize_html_error(doctype), "503 Service Unavailable");

        // Plain text error (not HTML) — returned as-is
        let plain = "{\"error\":{\"message\":\"rate limited\"}}";
        assert_eq!(sanitize_html_error(plain), "{\"error\":{\"message\":\"rate limited\"}}");

        // Empty string
        assert_eq!(sanitize_html_error(""), "");
    }

    #[test]
    fn test_parse_prompt_too_long_token_counts() {
        let (actual, limit) = parse_prompt_too_long_token_counts(
            "prompt is too long: 137500 tokens > 135000 maximum",
        );
        assert_eq!(actual, Some(137500));
        assert_eq!(limit, Some(135000));
    }
}
