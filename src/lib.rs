#![allow(dead_code)]
#![allow(unused)]
#![allow(unused_mut)]
#![allow(unused_variables)]
#![allow(unexpected_cfgs)]
#![allow(unreachable_patterns)]
#![allow(irrefutable_let_patterns)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(async_fn_in_trait)]

pub mod agent;
#[cfg(test)]
mod agent_tests;
pub mod query_engine;
pub mod error;
pub mod hooks;
pub mod interact;
pub mod http_client;
pub mod mcp;
pub mod message_queue_types;
pub mod session;
pub mod state;
pub mod task;
pub mod tasks;
pub mod tool;
pub mod tool_helper;
pub mod tools;
pub mod types;
pub mod utils;

pub mod env;
pub mod extract_memories;
pub mod memdir;
pub mod permission;
pub mod plugin;

pub use utils::{
    create_abort_controller, create_abort_controller_default, create_agent_id,
    create_child_abort_controller, get_aws_region, get_claude_config_home_dir, get_cwd,
    get_default_vertex_region, get_original_cwd, get_teams_dir, get_vertex_region_for_model,
    has_node_option, is_bare_mode, is_env_defined_falsy, is_env_truthy, is_in_protected_namespace,
    is_running_on_homespace, parse_env_vars, pwd, run_with_cwd_override, set_cwd,
    should_maintain_project_working_dir, validate_uuid, AbortController, AbortSignal,
};
pub use types::ids::AgentId;
pub mod ai_md;
pub mod analytics;
pub mod bridge;
pub mod bridge_enabled;
pub mod commands;
pub mod compact;
pub mod constants;
pub mod coordinator;
pub mod cost;
pub mod memory_types;
pub mod prompts;
pub mod review;
pub mod services;
pub mod session_discovery;
pub mod session_history;
pub mod session_memory;
pub mod skills;
pub mod team_memory;
pub mod user_agent;

pub use agent::Agent;
pub use ai_md::{
    get_ai_md_files, load_ai_md, process_ai_md_file, AiMdContent, AiMdFile, AiMdType,
    AI_MD_INSTRUCTION_PROMPT, MAX_AI_MD_CHARACTER_COUNT,
};
pub use bridge_enabled::{
    check_bridge_min_version, get_bridge_disabled_reason, get_ccr_auto_connect_default,
    is_bridge_enabled, is_bridge_enabled_blocking, is_ccr_mirror_enabled, is_cse_shim_enabled,
    is_env_less_bridge_enabled, OauthAccountInfo,
};
pub use compact::{
    compact_errors, estimate_token_count, get_compact_command, get_effective_context_window_size,
    get_auto_compact_threshold, should_compact, calculate_token_warning_state, CompactCommand,
    TokenWarningState, AUTOCOMPACT_BUFFER_TOKENS, WARNING_THRESHOLD_BUFFER_TOKENS,
    ERROR_THRESHOLD_BUFFER_TOKENS, MANUAL_COMPACT_BUFFER_TOKENS, MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES,
};
pub use services::compact::auto_compact::{
    is_auto_compact_enabled, should_auto_compact, AutoCompactTrackingState, AutoCompactResult,
    RecompactionInfo,
};
pub use query_engine::QueryEngine;
pub use env::{is_assistant_mode, is_assistant_mode_enabled, EnvConfig};
pub use error::AgentError;
pub use extract_memories::{
    build_extract_auto_only_prompt, build_extract_combined_prompt,
    count_model_visible_messages_since, count_tool_calls, create_auto_mem_can_use_tool,
    drain_pending_extractions, execute_extract_memories, parse_extracted_content,
    should_extract_memories, ExtractMemoriesConfig, ExtractMemoriesResult, ExtractMemoriesState,
    MemoryEntry, MemoryEntryType, PendingExtraction,
};
pub use hooks::{
    create_hook_registry, HookConfig, HookDefinition, HookInput, HookOutput, HookRegistry,
    CONFIG_CHANGE_SOURCES, EXIT_REASONS, HOOK_EVENTS, INSTRUCTIONS_LOAD_REASONS,
    INSTRUCTIONS_MEMORY_TYPES,
};
pub use memdir::{
    ensure_memory_dir_exists, get_auto_mem_path, get_memory_base_dir, get_memory_entrypoint,
    is_auto_mem_path, is_auto_memory_enabled, load_memory_prompt_sync,
    EntrypointTruncation, MemoryFrontmatter, MemoryType, MAX_ENTRYPOINT_LINES,
};
pub use message_queue_types::MessageQueueEntry;
pub use permission::{
    PermissionAllowDecision, PermissionAskDecision, PermissionBehavior, PermissionContext,
    PermissionDecision, PermissionDecisionReason, PermissionDenyDecision, PermissionHandler,
    PermissionMetadata, PermissionMode, PermissionResult, PermissionRule, PermissionRuleSource,
};
pub use plugin::{
    get_plugin_error_message, load_plugin, load_plugin_skills, load_plugins_from_dir,
    load_plugins_from_sources, register_plugin_skills, CommandAvailability, CommandMetadata,
    CommandRegistry, CommandResult, CommandResultDisplay, CommandSource, LoadedPlugin,
    PluginAuthor, PluginCommand, PluginComponent, PluginConfig, PluginError, PluginLoadResult,
    PluginManifest, PluginMcpServer, PluginMcpServerManager, PluginRepository, PluginSkill,
};
pub use services::{
    api::{
        categorize_retryable_api_error, classify_api_error, create_assistant_api_error_message,
        create_assistant_api_error_message_with_options, extract_unknown_error_format,
        get_error_message_if_refusal, get_image_too_large_error_message,
        get_oauth_org_not_allowed_error_message, get_pdf_invalid_error_message,
        get_pdf_password_protected_error_message, get_pdf_too_large_error_message,
        get_prompt_too_long_token_gap, get_request_too_large_error_message,
        get_token_revoked_error_message, is_ccr_mode, is_media_size_error,
        is_media_size_error_message, is_prompt_too_long_message, is_valid_api_message,
        parse_prompt_too_long_token_counts, starts_with_api_error_prefix, ApiErrorMessage,
        ApiErrorType, SDKAssistantMessageError, API_ERROR_MESSAGE_PREFIX,
        API_TIMEOUT_ERROR_MESSAGE, CCR_AUTH_ERROR_MESSAGE, CREDIT_BALANCE_TOO_LOW_ERROR_MESSAGE,
        CUSTOM_OFF_SWITCH_MESSAGE, INVALID_API_KEY_ERROR_MESSAGE,
        INVALID_API_KEY_ERROR_MESSAGE_EXTERNAL, NO_RESPONSE_REQUESTED,
        OAUTH_ORG_NOT_ALLOWED_ERROR_MESSAGE, ORG_DISABLED_ERROR_MESSAGE_ENV_KEY,
        ORG_DISABLED_ERROR_MESSAGE_ENV_KEY_WITH_OAUTH, PROMPT_TOO_LONG_ERROR_MESSAGE,
        REPEATED_529_ERROR_MESSAGE, TOKEN_REVOKED_ERROR_MESSAGE,
    },
    model_cost::{
        calculate_cost, format_cost, get_available_models, CostSummary, ModelCostRegistry,
        ModelCosts, ModelInfo, TokenUsage, COST_HAIKU_35, COST_HAIKU_45, COST_TIER_15_75,
        COST_TIER_3_15, COST_TIER_5_25,
    },
    rate_limit::{
        RateLimit as RateLimitInfo, RateLimitConfig, RateLimitStatus, RateLimiter,
        RateLimiterBuilder,
    },
    retry::{
        is_rate_limit_error, is_retryable_error, is_service_unavailable_error, retry_async,
        retry_with_retry_after, RetryConfig, RetryError, DEFAULT_MAX_RETRIES,
    },
    token_estimation::{
        calculate_padding, estimate_conversation, estimate_tokens, estimate_tokens_characters,
        estimate_tokens_words, estimate_tool_definitions, fits_in_context, EstimationMethod,
        TokenEstimate,
    },
};
pub use session::{
    append_to_session, delete_session, fork_session, get_session_info, get_session_messages,
    list_sessions, load_session, rename_session, save_session, tag_session, SessionData,
    SessionMetadata,
};
pub use session_discovery::discover_assistant_sessions;
pub use session_history::{
    create_history_auth_ctx, fetch_latest_events, fetch_older_events, get_bridge_access_token,
    get_bridge_base_url, get_bridge_base_url_override, get_bridge_headers,
    get_bridge_token_override, get_oauth_headers, prepare_api_request, HistoryAuthCtx, HistoryPage,
    OAuthTokens, OauthConfig, SDKMessage, HISTORY_PAGE_SIZE,
};
pub use session_memory::{
    get_last_summarized_message_id, get_session_memory_config, get_session_memory_content,
    get_session_memory_dir, get_session_memory_path, get_session_memory_state,
    get_tool_calls_between_updates, has_met_initialization_threshold, has_met_update_threshold,
    init_session_memory_file, is_session_memory_initialized, mark_session_memory_initialized,
    record_extraction_token_count, reset_session_memory_state, set_last_summarized_message_id,
    set_session_memory_config, should_extract_memory, wait_for_session_memory_extraction,
    ManualExtractionResult, SessionMemoryConfig, SessionMemoryState, DEFAULT_SESSION_MEMORY_CONFIG,
};
pub use skills::{
    get_bundled_skills, init_bundled_skills, load_skill_from_dir, load_skills_from_dir,
    BundledSkill, LoadedSkill, SkillMetadata,
};
pub use state::Store;
pub use task::{
    create_task_state_base, generate_task_id, get_task_id_prefix, get_task_output_path,
    is_terminal_task_status, LocalShellSpawnInput, ShellKind, TaskHandle, TaskStateBase,
    TaskStatus, TaskType, TASK_ID_ALPHABET, TASK_ID_PREFIXES,
};
pub use team_memory::{
    batch_delta_by_bytes, compute_delta, create_sync_state, delete_local_team_memory_entry,
    disable_team_memory, enable_team_memory, get_last_sync_error, get_team_memory_dir,
    get_team_memory_path, hash_content, is_team_memory_enabled, is_team_memory_sync_available,
    pull_team_memory, push_team_memory, read_local_team_memory, scan_entries_for_secrets,
    scan_for_secrets, set_last_sync_error, sync_team_memory, validate_team_memory_key,
    write_local_team_memory, SkippedSecretFile, SyncState, TeamMemoryContent, TeamMemoryData,
    TeamMemoryHashesResult, TeamMemorySyncFetchResult, TeamMemorySyncPushResult,
    TeamMemorySyncUploadResult, TeamMemoryTooManyEntries, MAX_CONFLICT_RETRIES,
    MAX_FILE_SIZE_BYTES, MAX_PUT_BODY_BYTES, MAX_RETRIES, TEAM_MEMORY_SYNC_TIMEOUT_MS,
};
pub use tool_helper::{
    create_tool, create_tool_with_annotations, sdk_tool_to_tool_definition, SdkToolDefinition,
    ToolAnnotations,
};
pub use tools::{filter_tools, get_all_base_tools, Tool, ToolDefinition, ToolInputSchema, ToolFuture};
pub use types::*;

/// Create a new agent with the given model and max turns.
/// This is a convenience function that matches the TypeScript SDK API.
/// Create a new agent (alias for Agent::new)
pub fn create_agent(model: &str, max_turns: u32) -> Agent {
    Agent::new(model, max_turns)
}

/// Alias for get_all_base_tools to match TypeScript API
pub fn get_all_tools() -> Vec<ToolDefinition> {
    get_all_base_tools()
}

/// Build-time version constant (matches TypeScript MACRO.VERSION)
pub const MACRO_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use user_agent::get_claude_code_user_agent;

// Re-export coordinator utilities
pub use coordinator::{
    apply_coordinator_tool_filter, get_coordinator_system_prompt, get_coordinator_user_context,
    is_coordinator_mode, is_pr_activity_subscription_tool, match_session_mode, WORKER_AGENT,
};
