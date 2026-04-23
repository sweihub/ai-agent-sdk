// Source: /data/home/swei/claudecode/openclaudecode/src/commands/compact/compact.ts
//! Context compaction module.
//!
//! Handles automatic context compaction when the conversation gets too long.
//! This includes token threshold detection, summary generation, and message management.

use crate::constants::env::{ai, ai_code};
pub use crate::services::token_estimation::{
    rough_token_count_estimation, rough_token_count_estimation_for_content,
    rough_token_count_estimation_for_message,
};
use crate::types::*;

/// Default context window sizes by model (in tokens)
pub const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;

/// Get default context window from environment or use default
pub fn get_default_context_window() -> u32 {
    if let Ok(override_val) = std::env::var(ai::CONTEXT_WINDOW) {
        if let Ok(parsed) = override_val.parse::<u32>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    DEFAULT_CONTEXT_WINDOW
}

/// Get the prompt for generating conversation summary
/// Translated from: getCompactPrompt in prompt.ts
pub fn get_compact_prompt() -> String {
    r#"CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.

- Do NOT use Read, Bash, Grep, Glob, Edit, Write, or ANY other tool.
- You already have all the context you need in the conversation above.
- Tool calls will be REJECTED and will waste your only turn — you will fail the task.
- Your entire response must be plain text: an <analysis> block followed by a <summary> block.

Your task is to create a detailed summary of the conversation so far, paying close attention to the user's explicit requests and your previous actions.
This summary should be thorough in capturing technical details, code patterns, and architectural decisions that would be essential for continuing development work without losing context.

Before providing your final summary, wrap your analysis in <analysis> tags to organize your thoughts and ensure you've covered all necessary points. In your analysis process:

1. Chronologically analyze each message and section of the conversation. For each section thoroughly identify:
   - The user's explicit requests and intents
   - Your approach to addressing the user's requests
   - Key decisions, technical concepts and code patterns
   - Specific details like:
     - file names
     - full code snippets
     - function signatures
     - file edits
   - Errors that you ran into and how you fixed them
   - Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.
2. Double-check for technical accuracy and completeness, addressing each required element thoroughly.

Your summary should include the following sections:

1. Primary Request and Intent: Capture all of the user's explicit requests and intents in detail
2. Key Technical Concepts: List all important technical concepts, technologies, and frameworks discussed.
3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or created. Pay special attention to the most recent messages and include full code snippets where applicable and include a summary of why this file read or edit is important.
4. Errors and fixes: List all errors that you ran into, and how you fixed them. Pay special attention to specific user feedback that you received, especially if the user told you to do something differently.
5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.
6. All user messages: List ALL user messages that are not tool results. These are critical for understanding the users' feedback and changing intent.
7. Pending Tasks: Outline any pending tasks that you have explicitly been asked to work on.
8. Current Work: Describe in detail precisely what was being worked on immediately before this summary request, paying special attention to the most recent messages from both user and assistant. Include file names and code snippets where applicable.
9. Context for Continuing Work: Key context, decisions, or state needed to continue the work.

IMPORTANT: Be extremely thorough — include ALL important technical details, code patterns, and architectural decisions. This summary must provide enough context for the next turn to continue seamlessly.

REMINDER: Do NOT call any tools. Respond with plain text only — an <analysis> block followed by a <summary> block. Tool calls will be rejected and you will fail the task.
"#.to_string()
}

/// Reserve tokens for output during compaction
/// Based on p99.99 of compact summary output
pub const MAX_OUTPUT_TOKENS_FOR_SUMMARY: u32 = 20_000;

/// Buffer tokens for auto-compact trigger
pub const AUTOCOMPACT_BUFFER_TOKENS: u32 = 13_000;

/// Buffer tokens for warning threshold
pub const WARNING_THRESHOLD_BUFFER_TOKENS: u32 = 20_000;

/// Buffer tokens for error threshold
pub const ERROR_THRESHOLD_BUFFER_TOKENS: u32 = 20_000;

/// Get the blocking limit (when to block further input)
pub fn get_blocking_limit(model: &str) -> u32 {
    let effective_window = get_effective_context_window_size(model);
    let default_blocking_limit = effective_window.saturating_sub(MANUAL_COMPACT_BUFFER_TOKENS);

    // Allow override for testing
    if let Ok(override_val) = std::env::var(ai::BLOCKING_LIMIT_OVERRIDE) {
        if let Ok(parsed) = override_val.parse::<u32>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }

    default_blocking_limit
}

/// Manual compact uses smaller buffer (more aggressive)
pub const MANUAL_COMPACT_BUFFER_TOKENS: u32 = 3_000;

/// Maximum consecutive auto-compact failures before giving up
pub const MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES: u32 = 3;

/// Post-compaction: max files to restore
pub const POST_COMPACT_MAX_FILES_TO_RESTORE: u32 = 5;

/// Post-compaction: token budget for restored files
pub const POST_COMPACT_TOKEN_BUDGET: u32 = 50_000;

/// Post-compaction: max tokens per file
pub const POST_COMPACT_MAX_TOKENS_PER_FILE: u32 = 5_000;

/// Post-compaction: max tokens per skill
pub const POST_COMPACT_MAX_TOKENS_PER_SKILL: u32 = 5_000;

/// Post-compaction: skills token budget
pub const POST_COMPACT_SKILLS_TOKEN_BUDGET: u32 = 25_000;

/// Get effective context window size (total - output reserve)
pub fn get_effective_context_window_size(model: &str) -> u32 {
    let context_window = get_context_window_for_model(model);
    context_window.saturating_sub(MAX_OUTPUT_TOKENS_FOR_SUMMARY)
}

/// Get context window size for a model
pub fn get_context_window_for_model(model: &str) -> u32 {
    // Check environment override for auto compact window
    if let Ok(override_val) = std::env::var(ai::AUTO_COMPACT_WINDOW) {
        if let Ok(parsed) = override_val.parse::<u32>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }

    // Default context windows by model
    let lower = model.to_lowercase();
    if lower.contains("sonnet") {
        // Claude Sonnet models typically have 200K context
        get_default_context_window()
    } else if lower.contains("haiku") {
        // Haiku has 200K context
        get_default_context_window()
    } else if lower.contains("opus") {
        // Opus models typically have 200K context
        get_default_context_window()
    } else {
        get_default_context_window()
    }
}

/// Get the auto-compact threshold (when to trigger compaction)
pub fn get_auto_compact_threshold(model: &str) -> u32 {
    let effective_window = get_effective_context_window_size(model);

    let autocompact_threshold = effective_window.saturating_sub(AUTOCOMPACT_BUFFER_TOKENS);

    // Override for easier testing of autocompact
    if let Ok(env_percent) = std::env::var(ai::AUTOCOMPACT_PCT_OVERRIDE) {
        if let Ok(parsed) = env_percent.parse::<f64>() {
            if parsed > 0.0 && parsed <= 100.0 {
                let percentage_threshold =
                    ((effective_window as f64 * (parsed / 100.0)) as u32).min(effective_window);
                return percentage_threshold.min(autocompact_threshold);
            }
        }
    }

    autocompact_threshold
}

/// Calculate token warning state
/// Translated from: calculateTokenWarningState in autoCompact.ts
#[derive(Debug, Clone)]
pub struct TokenWarningState {
    pub percent_left: f64,
    pub is_above_warning_threshold: bool,
    pub is_above_error_threshold: bool,
    pub is_above_auto_compact_threshold: bool,
    pub is_at_blocking_limit: bool,
}

pub fn calculate_token_warning_state(token_usage: u32, model: &str) -> TokenWarningState {
    let auto_compact_threshold = get_auto_compact_threshold(model);
    let effective_window = get_effective_context_window_size(model);

    // Use auto_compact_threshold if enabled, otherwise use effective window
    let threshold = if is_auto_compact_enabled_for_calculation() {
        auto_compact_threshold
    } else {
        effective_window
    };

    let percent_left = if threshold > 0 {
        ((threshold.saturating_sub(token_usage) as f64 / threshold as f64) * 100.0).max(0.0)
    } else {
        100.0
    };

    let warning_threshold = threshold.saturating_sub(WARNING_THRESHOLD_BUFFER_TOKENS);
    let error_threshold = threshold.saturating_sub(ERROR_THRESHOLD_BUFFER_TOKENS);

    let is_above_warning_threshold = token_usage >= warning_threshold;
    let is_above_error_threshold = token_usage >= error_threshold;
    let is_above_auto_compact_threshold =
        is_auto_compact_enabled_for_calculation() && token_usage >= auto_compact_threshold;

    // Calculate blocking limit
    let default_blocking_limit = effective_window.saturating_sub(MANUAL_COMPACT_BUFFER_TOKENS);

    // Allow override for testing (translate from CLAUDE_CODE_BLOCKING_LIMIT_OVERRIDE)
    let blocking_limit = if let Ok(override_val) = std::env::var(ai_code::BLOCKING_LIMIT_OVERRIDE) {
        if let Ok(parsed) = override_val.parse::<u32>() {
            if parsed > 0 {
                parsed
            } else {
                default_blocking_limit
            }
        } else {
            default_blocking_limit
        }
    } else {
        default_blocking_limit
    };

    let is_at_blocking_limit = token_usage >= blocking_limit;

    TokenWarningState {
        percent_left,
        is_above_warning_threshold,
        is_above_error_threshold,
        is_above_auto_compact_threshold,
        is_at_blocking_limit,
    }
}

/// Check if auto-compact is enabled (used in calculation)
/// Translated from: isAutoCompactEnabled in autoCompact.ts
fn is_auto_compact_enabled_for_calculation() -> bool {
    use crate::utils::env_utils::is_env_truthy;

    if is_env_truthy(Some("DISABLE_COMPACT")) {
        return false;
    }
    if is_env_truthy(Some("DISABLE_AUTO_COMPACT")) {
        return false;
    }
    // Check user config - for now default to true
    // In full implementation: getGlobalConfig().autoCompactEnabled
    true
}

/// Compact result containing the new messages after compaction
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// The boundary marker message
    pub boundary_marker: Message,
    /// Summary messages to keep
    pub summary_messages: Vec<Message>,
    /// Messages that were kept (not summarized)
    pub messages_to_keep: Option<Vec<Message>>,
    /// Attachments to include
    pub attachments: Vec<Message>,
    /// Pre-compaction token count
    pub pre_compact_token_count: u32,
    /// Post-compaction token count
    pub post_compact_token_count: u32,
}

/// Strip images from messages before sending for compaction
/// Images are replaced with `[image]` text markers, documents with `[document]` markers
/// to prevent compaction API from hitting prompt-too-long
pub fn strip_images_from_messages(messages: &[Message]) -> Vec<Message> {
    use crate::types::MessageRole;

    messages
        .iter()
        .map(|msg| {
            match msg.role {
                MessageRole::User | MessageRole::Assistant => {
                    // For user/assistant messages, strip image/document blocks
                    // In the simple String content model, we look for image-like patterns
                    let content = msg.content.clone();
                    // Check for image markdown patterns
                    if content.contains("![") || content.contains("<img") {
                        // Strip markdown images: ![alt](url)
                        let stripped = strip_image_markdown(&content);
                        if stripped != content {
                            return Message {
                                role: msg.role.clone(),
                                content: stripped,
                                ..msg.clone()
                            };
                        }
                    }
                    msg.clone()
                }
                MessageRole::Tool => {
                    // Tool results might contain image references
                    let content = msg.content.clone();
                    if content.contains("![")
                        || content.contains("<img")
                        || content.contains("image")
                        || content.contains("document")
                    {
                        let stripped = strip_image_markdown(&content);
                        if stripped != content {
                            return Message {
                                role: msg.role.clone(),
                                content: stripped,
                                ..msg.clone()
                            };
                        }
                    }
                    msg.clone()
                }
                MessageRole::System => msg.clone(),
            }
        })
        .collect()
}

/// Strip markdown image patterns from content, replacing with text markers
fn strip_image_markdown(content: &str) -> String {
    // Replace markdown images ![alt](url) with [image]
    let mut result = content.to_string();

    // Simple regex-like replacement for markdown images
    // ![...](...) → [image]
    let mut output = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            // Find the closing ](
            if let Some(close_bracket) = chars[i..].iter().position(|&c| c == ']') {
                let bracket_pos = i + close_bracket;
                if bracket_pos + 1 < chars.len() && chars[bracket_pos + 1] == '(' {
                    // Find the closing )
                    if let Some(close_paren) =
                        chars[bracket_pos + 2..].iter().position(|&c| c == ')')
                    {
                        let paren_pos = bracket_pos + 2 + close_paren;
                        // Extract alt text
                        let alt: String = chars[i + 2..bracket_pos].iter().collect();
                        let marker = if alt.to_lowercase().contains("doc")
                            || alt.to_lowercase().contains("pdf")
                            || alt.to_lowercase().contains("file")
                        {
                            "[document]"
                        } else {
                            "[image]"
                        };
                        output.push_str(marker);
                        i = paren_pos + 1;
                        continue;
                    }
                }
            }
        }
        output.push(chars[i]);
        i += 1;
    }

    output
}

/// Strip reinjected attachments (skill_discovery/skill_listing) that will be
/// re-injected post-compaction anyway
pub fn strip_reinjected_attachments(messages: &[Message]) -> Vec<Message> {
    // In the simple String content model, we look for skill attachment patterns
    messages
        .iter()
        .map(|msg| {
            if msg.content.contains("skill_discovery") || msg.content.contains("skill_listing") {
                Message {
                    role: msg.role.clone(),
                    content: "[Skill attachment content cleared for compaction]".to_string(),
                    ..msg.clone()
                }
            } else {
                msg.clone()
            }
        })
        .collect()
}

/// Estimate token count for messages (rough estimation)
/// Uses 4 chars per token for regular text (matching original TypeScript)
/// Uses 2 chars per token for tool results (JSON is more token-efficient)
/// Takes optional max_output_tokens to ensure we leave room for the response
pub fn estimate_token_count(messages: &[Message], max_output_tokens: u32) -> u32 {
    // Regular text: 4 chars per token (original TypeScript default)
    let non_tool_chars: usize = messages
        .iter()
        .filter(|msg| msg.role != MessageRole::Tool)
        .map(|msg| msg.content.len())
        .sum();

    // Tool results (JSON): 2 chars per token (more efficient encoding)
    // Original: "Dense JSON has many single-character tokens..."
    let tool_result_chars: usize = messages
        .iter()
        .filter(|msg| msg.role == MessageRole::Tool)
        .map(|msg| msg.content.len())
        .sum();

    let base_estimate = (non_tool_chars / 4) as u32;
    let tool_buffer = (tool_result_chars / 2) as u32; // More efficient for JSON

    // Add the requested output tokens to ensure we leave room for the response
    base_estimate + tool_buffer + max_output_tokens
}

/// Check if conversation should be compacted
pub fn should_compact(token_usage: u32, model: &str) -> bool {
    let state = calculate_token_warning_state(token_usage, model);
    state.is_above_auto_compact_threshold
}

/// Truncate messages to fit within a safe token limit for summarization
/// This is used when the conversation is too large to fit in context
/// Skips ALL system messages (they contain huge compaction summaries)
/// Returns (truncated_messages, estimated_tokens)
pub fn truncate_messages_for_summary(
    messages: &[Message],
    model: &str,
    max_output_tokens: u32,
) -> (Vec<Message>, u32) {
    let context_window = get_context_window_for_model(model);
    // Leave room for output tokens and buffer - use 50% of available space for safety
    let safe_limit = ((context_window.saturating_sub(max_output_tokens)) as f64 * 0.50) as u32;

    let total_messages = messages.len();
    if total_messages == 0 {
        return (vec![], 0);
    }

    // Skip ALL system messages - they contain huge compaction summaries from previous rounds
    // For summarization, we only need the conversation history (user/assistant/tool messages)
    let non_system_messages: Vec<Message> = messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .cloned()
        .collect();

    // Now take most recent non-system messages using proper token estimation
    let mut current_tokens = 0u32;
    let mut history_messages = Vec::new();

    for msg in non_system_messages.iter().rev() {
        let msg_tokens = rough_token_count_estimation_for_message(msg) as u32;
        if current_tokens + msg_tokens > safe_limit {
            break;
        }
        current_tokens += msg_tokens;
        history_messages.insert(0, msg.clone());
    }

    // If we couldn't fit any history, try to at least get recent messages
    if history_messages.is_empty() && !non_system_messages.is_empty() {
        // Take just the last message, truncated if needed
        let last_msg = non_system_messages.last().unwrap();
        let max_chars = (safe_limit as usize) * 4;
        let chars_to_keep = last_msg.content.len().min(max_chars);
        let truncated_content = last_msg
            .content
            .chars()
            .take(chars_to_keep)
            .collect::<String>();

        current_tokens = rough_token_count_estimation(&truncated_content, 4.0) as u32;

        history_messages = vec![Message {
            role: last_msg.role.clone(),
            content: truncated_content,
            ..Default::default()
        }];
    }

    let total_estimated = current_tokens;

    (history_messages, total_estimated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_context_window() {
        let window = get_effective_context_window_size("claude-sonnet-4-6");
        // 200000 - 20000 = 180000
        assert_eq!(window, 180_000);
    }

    #[test]
    fn test_auto_compact_threshold() {
        let threshold = get_auto_compact_threshold("claude-sonnet-4-6");
        // 180000 - 13000 = 167000
        assert_eq!(threshold, 167_000);
    }

    #[test]
    fn test_token_warning_state_normal() {
        let state = calculate_token_warning_state(50_000, "claude-sonnet-4-6");
        assert!(!state.is_above_warning_threshold);
        assert!(!state.is_above_error_threshold);
        assert!(!state.is_above_auto_compact_threshold);
        assert!(state.percent_left > 50.0);
    }

    #[test]
    fn test_token_warning_state_warning() {
        // warning at 180000 - 20000 = 160000
        let state = calculate_token_warning_state(165_000, "claude-sonnet-4-6");
        assert!(state.is_above_warning_threshold);
        // error uses same buffer, so this is also above error threshold
        assert!(state.is_above_error_threshold);
        assert!(!state.is_above_auto_compact_threshold);
    }

    #[test]
    fn test_token_warning_state_compact() {
        let state = calculate_token_warning_state(170_000, "claude-sonnet-4-6");
        assert!(state.is_above_warning_threshold);
        assert!(state.is_above_auto_compact_threshold);
    }

    #[test]
    fn test_should_compact() {
        assert!(!should_compact(50_000, "claude-sonnet-4-6"));
        assert!(should_compact(170_000, "claude-sonnet-4-6"));
    }

    #[test]
    fn test_estimate_token_count() {
        let messages = vec![
            Message {
                role: MessageRole::User,
                content: "Hello, this is a test message".to_string(),
                ..Default::default()
            },
            Message {
                role: MessageRole::Assistant,
                content: "Hi! How can I help you today?".to_string(),
                ..Default::default()
            },
        ];

        let count = estimate_token_count(&messages, 0);
        // ~60 chars / 4 = 15 tokens
        assert!(count > 0);
    }
}

// ============================================================================
// Compact Command Module (translated from commands/compact/)
// ============================================================================

/// Compact command definition
/// Translates: /data/home/swei/claudecode/openclaudecode/src/commands/compact/index.ts

/// Check if an environment variable is truthy (copied from bridge_enabled)
fn is_env_truthy(env_var: &str) -> bool {
    if env_var.is_empty() {
        return false;
    }
    let binding = env_var.to_lowercase();
    let normalized = binding.trim();
    matches!(normalized, "1" | "true" | "yes" | "on")
}

/// Compact command configuration
#[derive(Debug, Clone)]
pub struct CompactCommand {
    /// Command type
    pub command_type: String,
    /// Command name
    pub name: String,
    /// Command description
    pub description: String,
    /// Whether the command is enabled
    pub is_enabled: fn() -> bool,
    /// Whether it supports non-interactive mode
    pub supports_non_interactive: bool,
    /// Argument hint text
    pub argument_hint: String,
}

impl Default for CompactCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CompactCommand {
    /// Create a new compact command
    pub fn new() -> Self {
        Self {
            command_type: "local".to_string(),
            name: "compact".to_string(),
            description: "Clear conversation history but keep a summary in context. Optional: /compact [instructions for summarization]".to_string(),
            is_enabled: || !is_env_truthy("AI_DISABLE_COMPACT"),
            supports_non_interactive: true,
            argument_hint: "<optional custom summarization instructions>".to_string(),
        }
    }

    /// Check if the command is enabled
    pub fn is_enabled(&self) -> bool {
        (self.is_enabled)()
    }
}

/// Get the compact command
pub fn get_compact_command() -> CompactCommand {
    CompactCommand::new()
}

/// Compact command error messages
pub mod compact_errors {
    /// Error message for incomplete response
    pub const ERROR_MESSAGE_INCOMPLETE_RESPONSE: &str =
        "Incomplete response from model during compaction";
    /// Error message for not enough messages
    pub const ERROR_MESSAGE_NOT_ENOUGH_MESSAGES: &str = "Not enough messages to compact";
    /// Error message for user abort
    pub const ERROR_MESSAGE_USER_ABORT: &str = "User aborted compaction";
}

/// Post-compact restore state — tracks recently accessed files for restoration
#[derive(Debug, Clone, Default)]
pub struct FileReadState {
    /// Maps file path → (content, access order index)
    entries: std::collections::HashMap<String, (String, u64)>,
    /// Monotonic counter for recency tracking
    counter: u64,
}

impl FileReadState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file read. More recent reads get higher priority for restore.
    pub fn record(&mut self, path: String, content: String) {
        self.counter += 1;
        self.entries.insert(path, (content, self.counter));
    }

    /// Get the most recently accessed files, limited to max_files.
    /// Skips files whose paths are already in preserved_read_paths.
    pub fn recent_files(
        &self,
        max_files: usize,
        preserved_read_paths: &std::collections::HashSet<String>,
    ) -> Vec<(String, String)> {
        let mut entries: Vec<(&String, &(String, u64))> = self.entries.iter().collect();
        // Sort by recency (highest counter = most recent)
        entries.sort_by(|a, b| b.1.1.cmp(&a.1.1));
        entries
            .into_iter()
            .filter_map(|(path, (content, _))| {
                if preserved_read_paths.contains(path.as_str()) {
                    None
                } else if should_exclude_from_restore(path) {
                    None
                } else {
                    Some((path.clone(), content.clone()))
                }
            })
            .take(max_files)
            .collect()
    }
}

/// Paths excluded from post-compact restore (plan files, memory files, CLAUDE.md variants)
fn should_exclude_from_restore(path: &str) -> bool {
    let lower = path.to_lowercase();
    // Exclude AI.md / CLAUDE.md variants
    if lower.ends_with("ai.md") || lower.ends_with("claude.md") {
        return true;
    }
    // Exclude memory files
    if lower.contains(".ai/memory/") || lower.contains(".claude/memory/") {
        return true;
    }
    // Exclude plan files
    if lower.contains("/plans/") {
        return true;
    }
    false
}

/// Collect file paths from Read tool results in preserved messages.
/// Returns paths that are already visible and don't need restoration.
pub fn collect_read_tool_file_paths(messages: &[Message]) -> std::collections::HashSet<String> {
    let mut paths = std::collections::HashSet::new();
    for msg in messages {
        if msg.role != MessageRole::Assistant {
            continue;
        }
        // Check if this is a Read tool call
        if let Some(ref calls) = msg.tool_calls {
            for call in calls {
                if call.name == "Read" {
                    if let Some(path) = call.arguments.get("file_path").and_then(|p| p.as_str()) {
                        paths.insert(path.to_string());
                    }
                }
            }
        }
    }
    paths
}

/// SKILL_TRUNCATION_MARKER appended when a skill is truncated for post-compact restore.
pub const SKILL_TRUNCATION_MARKER: &str =
    "\n\n[... skill content truncated for compaction; use Read on the skill path if you need the full text]";

/// Truncate content to roughly max_tokens, keeping the head.
/// rough_token_count_estimation uses ~4 chars/token, so char budget = max_tokens * 4.
pub fn truncate_to_tokens(content: &str, max_tokens: u32) -> String {
    if rough_token_count_estimation_for_content(content) <= max_tokens as usize {
        return content.to_string();
    }
    let char_budget = (max_tokens as usize).saturating_sub(SKILL_TRUNCATION_MARKER.len())
        * 4
        .min(content.len());
    format!("{}{}", &content[..char_budget], SKILL_TRUNCATION_MARKER)
}

/// Post-compact file restore result
pub struct PostCompactRestore {
    /// Attachment messages for recently read files
    pub file_attachments: Vec<Message>,
    /// Attachment messages for invoked skills
    pub skill_attachments: Vec<Message>,
}

/// Create post-compact file restore attachments.
///
/// Reads the most recently accessed files that fit within the token budget
/// and returns them as attachment messages to re-inject after compaction.
pub fn create_post_compact_file_attachments(
    file_state: &FileReadState,
    preserved_messages: &[Message],
    max_files: usize,
) -> Vec<Message> {
    let preserved_paths = collect_read_tool_file_paths(preserved_messages);
    let recent = file_state.recent_files(max_files, &preserved_paths);

    let mut attachments = Vec::new();
    let mut used_tokens: usize = 0;

    for (path, content) in recent {
        let truncated = truncate_to_tokens(&content, POST_COMPACT_MAX_TOKENS_PER_FILE);
        let attachment = create_file_restore_attachment(&path, &truncated);
        let tokens = rough_token_count_estimation_for_content(
            &serde_json::to_string(&attachment).unwrap_or_default(),
        );
        if used_tokens + tokens <= POST_COMPACT_TOKEN_BUDGET as usize {
            used_tokens += tokens;
            attachments.push(attachment);
        }
    }
    attachments
}

/// Create a single file restore attachment message
fn create_file_restore_attachment(path: &str, content: &str) -> Message {
    Message {
        role: MessageRole::User,
        content: format!(
            "<post-compact-file-restore>\nFile: {}\n```\n{}\n```\n</post-compact-file-restore>",
            path, content
        ),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: Some(true),
    }
}

/// Create post-compact skill restore attachments.
///
/// Takes a list of (skill_name, skill_content) pairs and creates attachment
/// messages within the skills token budget.
pub fn create_post_compact_skill_attachments(
    skills: &[(String, String)],
) -> Vec<Message> {
    let mut attachments = Vec::new();
    let mut used_tokens: usize = 0;

    for (name, content) in skills {
        let truncated = truncate_to_tokens(content, POST_COMPACT_MAX_TOKENS_PER_SKILL);
        let attachment = create_skill_restore_attachment(name, &truncated);
        let tokens = rough_token_count_estimation_for_content(
            &serde_json::to_string(&attachment).unwrap_or_default(),
        );
        if used_tokens + tokens <= POST_COMPACT_SKILLS_TOKEN_BUDGET as usize {
            used_tokens += tokens;
            attachments.push(attachment);
        }
    }
    attachments
}

/// Create a single skill restore attachment message
fn create_skill_restore_attachment(name: &str, content: &str) -> Message {
    Message {
        role: MessageRole::User,
        content: format!(
            "<post-compact-skill-restore>\nSkill: {}\n```\n{}\n```\n</post-compact-skill-restore>",
            name, content
        ),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: Some(true),
    }
}

#[cfg(test)]
mod post_compact_tests {
    use super::*;

    #[test]
    fn test_file_read_state_records_and_retrieves() {
        let mut state = FileReadState::new();
        state.record("/a.txt".to_string(), "content a".to_string());
        state.record("/b.txt".to_string(), "content b".to_string());
        let recent = state.recent_files(1, &std::collections::HashSet::new());
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].0, "/b.txt"); // most recent
    }

    #[test]
    fn test_file_read_state_skips_preserved() {
        let mut state = FileReadState::new();
        state.record("/a.txt".to_string(), "content a".to_string());
        state.record("/b.txt".to_string(), "content b".to_string());
        let mut preserved = std::collections::HashSet::new();
        preserved.insert("/a.txt".to_string());
        let recent = state.recent_files(5, &preserved);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].0, "/b.txt");
    }

    #[test]
    fn test_should_exclude_from_restore() {
        assert!(should_exclude_from_restore("/home/user/.ai/ai.md"));
        assert!(should_exclude_from_restore("/home/user/.ai/memory/user.md"));
        assert!(should_exclude_from_restore("/home/user/.claude/memory/feedback.md"));
        assert!(should_exclude_from_restore("/home/user/.claude/plans/my-plan.md"));
        assert!(!should_exclude_from_restore("/home/user/src/main.rs"));
        assert!(!should_exclude_from_restore("/home/user/Cargo.toml"));
    }

    #[test]
    fn test_truncate_to_tokens_no_truncation() {
        let content = "short content";
        assert_eq!(truncate_to_tokens(content, 100), "short content");
    }

    #[test]
    fn test_truncate_to_tokens_truncates() {
        let content = "a".repeat(10_000);
        let truncated = truncate_to_tokens(&content, 10);
        assert!(truncated.contains(SKILL_TRUNCATION_MARKER));
        assert!(truncated.len() < content.len());
    }

    #[test]
    fn test_collect_read_tool_file_paths() {
        let messages = vec![Message {
            role: MessageRole::Assistant,
            content: "reading file".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: "t1".to_string(),
                r#type: "function".to_string(),
                name: "Read".to_string(),
                arguments: serde_json::json!({"file_path": "/foo/bar.txt"}),
            }]),
            is_error: None,
            is_meta: None,
        }];
        let paths = collect_read_tool_file_paths(&messages);
        assert!(paths.contains("/foo/bar.txt"));
    }

    #[test]
    fn test_collect_read_tool_file_paths_skips_non_read() {
        let messages = vec![Message {
            role: MessageRole::Assistant,
            content: "running bash".to_string(),
            attachments: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: "t1".to_string(),
                r#type: "function".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({"command": "ls"}),
            }]),
            is_error: None,
            is_meta: None,
        }];
        let paths = collect_read_tool_file_paths(&messages);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_create_post_compact_file_attachments() {
        let mut state = FileReadState::new();
        state.record("/a.txt".to_string(), "a".repeat(100).to_string());
        state.record("/b.txt".to_string(), "b".repeat(100).to_string());
        let attachments = create_post_compact_file_attachments(&state, &[], 5);
        assert_eq!(attachments.len(), 2);
        assert!(attachments[0].is_meta == Some(true));
        assert!(attachments[0].content.contains("post-compact-file-restore"));
    }

    #[test]
    fn test_create_post_compact_skill_attachments() {
        let skills = vec![("my-skill".to_string(), "skill content here".to_string())];
        let attachments = create_post_compact_skill_attachments(&skills);
        assert_eq!(attachments.len(), 1);
        assert!(attachments[0].content.contains("my-skill"));
        assert!(attachments[0].content.contains("post-compact-skill-restore"));
    }

    #[test]
    fn test_post_compact_restore_token_budget() {
        let mut state = FileReadState::new();
        // Create large files that exceed budget
        for i in 0..20 {
            state.record(
                format!("/file_{}.txt", i),
                "x".repeat(100_000), // Each file is large
            );
        }
        let attachments = create_post_compact_file_attachments(&state, &[], 5);
        // Should be limited by budget
        assert!(!attachments.is_empty());
        assert!(attachments.len() <= 5);
        // Total tokens should be within budget
        let total_tokens: usize = attachments
            .iter()
            .map(|a| rough_token_count_estimation_for_content(&serde_json::to_string(a).unwrap_or_default()))
            .sum();
        assert!(total_tokens <= POST_COMPACT_TOKEN_BUDGET as usize);
    }
}
