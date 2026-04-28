//! Service modules for agent functionality.
//!
//! This module provides various services similar to claude code's services:
//! - Analytics and event logging
//! - API error handling and retry logic
//! - Rate limiting
//! - Token estimation
//! - Model cost calculation

// Main service modules
pub mod agent_summary;
pub mod analytics;
pub mod api;
pub mod auto_dream;
pub mod away_summary;
pub mod claude_ai_limits;
pub mod compact;
pub mod context_collapse;
pub mod diagnostic_tracking;
pub mod extract_memories;
pub mod internal_logging;
pub mod lsp;
pub mod magic_docs;
pub mod mcp;
pub mod mock_rate_limits;
pub mod model_cost;
pub mod notifier;
pub mod oauth;
pub mod plugin_operations;
pub mod plugins;
pub mod policy_limits;
pub mod prevent_sleep;
pub mod prompt_suggestion;
pub mod rate_limit;
pub mod rate_limit_messages;
pub mod rate_limit_mocking;
pub mod remote_managed_settings;
pub mod retry;
pub mod session_memory;
pub mod settings_sync;
pub mod skill_search;
pub mod streaming;
pub mod team_memory_sync;
pub mod tips;
pub mod token_estimation;
pub mod tool_execution;
pub mod tool_use_summary;
pub mod vcr;
pub mod voice;
pub mod voice_keyterms;

// Re-export commonly used items
pub use api::errors::*;
pub use api::retry_helpers::*;
pub use api::usage::*;
pub use api::with_retry::*;

pub use compact::{
    CompactDirection, CompactOptions, CompactResult, CompactWarningInfo, WarningLevel,
    calculate_messages_to_remove, compact_messages, create_compact_warning_info,
    get_recommended_direction,
};

pub use model_cost::{
    CostSummary, ModelCostRegistry, ModelCosts, ModelInfo, TokenUsage, calculate_cost, format_cost,
    get_available_models,
};

pub use rate_limit::{
    RateLimit as RateLimitInfo, RateLimitConfig, RateLimitStatus, RateLimiter, RateLimiterBuilder,
};

pub use retry::{
    DEFAULT_MAX_RETRIES, RetryConfig, RetryError, is_rate_limit_error, is_retryable_error,
    is_service_unavailable_error, retry_async, retry_with_retry_after,
};

pub use token_estimation::{
    bytes_per_token_for_file_type, count_messages_tokens_with_api, count_tokens_via_haiku_fallback,
    count_tokens_with_api, count_tokens_with_fallback, estimate_conversation, estimate_tokens,
    estimate_tokens_characters, estimate_tokens_words, estimate_tool_definitions,
    get_default_file_read_max_tokens, fits_in_context, rough_token_count_estimation,
    rough_token_count_estimation_for_content, rough_token_count_estimation_for_file_type,
    rough_token_count_estimation_for_message, rough_token_count_estimation_for_messages,
    calculate_padding, EstimationMethod, TokenEstimate, MaxFileReadTokenExceededError,
    TOKEN_COUNT_MAX_TOKENS, TOKEN_COUNT_THINKING_BUDGET, DEFAULT_FILE_READ_MAX_TOKENS,
    validate_content_tokens,
};

pub use streaming::{
    STALL_THRESHOLD_MS, StallStats, StreamWatchdog, StreamingResult, StreamingToolExecutor,
    ToolStatus, TrackedTool, calculate_streaming_cost, cleanup_stream,
    get_nonstreaming_fallback_timeout_ms, is_404_stream_creation_error, is_api_timeout_error,
    is_nonstreaming_fallback_disabled, is_user_abort_error, release_stream_resources,
    validate_stream_completion,
};
