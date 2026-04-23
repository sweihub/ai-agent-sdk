//! Hook utilities for agent tool permission checking, hook execution, and related helpers.
//!
//! This module provides:
//! - `can_use_tool`: The CanUseToolFn type and related types for permission checking
//! - `helpers`: Helper functions for structured output and argument substitution
//! - `hook_helpers`: Additional hook helpers for structured output enforcement
//! - `api_query_hook_helper`: API query hook creation and execution
//! - `async_hook_registry`: Registry for async hooks with progress tracking
//! - `exec_agent_hook`: Execute agent-based hooks using multi-turn LLM queries
//! - `exec_http_hook`: Execute HTTP hooks by POSTing to configured URLs
//! - `exec_prompt_hook`: Execute prompt-based hooks using single LLM queries
//! - `file_changed_watcher`: File system watcher for CwdChanged/FileChanged hooks
//! - `hook_events`: Event system for broadcasting hook execution events
//! - `hooks_config_manager`: Hook configuration management with event metadata
//! - `hooks_config_snapshot`: Hooks configuration snapshot with policy enforcement
//! - `hooks_settings`: Hook settings parsing and source management
//! - `post_sampling_hooks`: Post-sampling hook registration and execution
//! - `register_frontmatter_hooks`: Register hooks from agent/skill frontmatter
//! - `register_skill_hooks`: Register hooks from skill frontmatter with once support
//! - `session_hooks`: Session-scoped hook storage and management
//! - `skill_improvement`: Automatic skill improvement detection and application
//! - `ssrf_guard`: SSRF guard for HTTP hook DNS resolution

pub mod api_query_hook_helper;
pub mod async_hook_registry;
pub mod can_use_tool;
pub mod exec_agent_hook;
pub mod exec_http_hook;
pub mod exec_prompt_hook;
pub mod file_changed_watcher;
pub mod helpers;
pub mod hook_events;
pub mod hook_helpers;
pub mod hooks_config_manager;
pub mod hooks_config_snapshot;
pub mod hooks_settings;
pub mod post_sampling_hooks;
pub mod register_frontmatter_hooks;
pub mod register_skill_hooks;
pub mod session_hooks;
pub mod skill_improvement;
pub mod ssrf_guard;

// Re-export core types from existing modules
pub use can_use_tool::*;
pub use helpers::*;

// Re-export from new modules with explicit exports to avoid ambiguity
pub use api_query_hook_helper::{
    ApiQueryHookConfig, ApiQueryResult, ReplHookContext, SystemPrompt, create_api_query_hook,
};
pub use async_hook_registry::{
    HookEvent as AsyncHookEvent, HookExecutionEvent, HookOutcome, PendingAsyncHook,
    check_for_async_hook_responses, clear_hook_event_state, emit_hook_progress, emit_hook_response,
    emit_hook_started, get_pending_async_hooks, register_hook_event_handler,
    register_pending_async_hook, set_all_hook_events_enabled, start_hook_progress_interval,
};
pub use exec_agent_hook::{HookResult as ExecAgentHookResult, exec_agent_hook};
pub use exec_http_hook::{HttpHook, HttpHookResult, exec_http_hook};
pub use exec_prompt_hook::{HookResult as ExecPromptHookResult, PromptHook, exec_prompt_hook};
pub use file_changed_watcher::{
    FileEvent, HookOutsideReplResult, initialize_file_changed_watcher, on_cwd_changed_for_hooks,
    set_env_hook_notifier, update_watch_paths,
};
pub use hook_events::{
    EmitHookResponseParams, HookEventHandler, HookExecutionEvent as HookEventsExecution,
    HookOutcome as HookEventsOutcome, HookProgressEvent, HookResponseEvent, HookStartedEvent,
    ProgressOutput, StartHookProgressParams as HookEventsStartHookProgressParams,
};
pub use hook_helpers::{
    HookResponse, SYNTHETIC_OUTPUT_TOOL_NAME, create_structured_output_tool,
    has_successful_tool_call, hook_response_schema, register_structured_output_enforcement,
};
pub use hooks_config_manager::{
    HOOK_EVENTS, HookCommand as HookConfigCommand, HookEvent, HookEventMetadata,
    HookSource as HookConfigSource, IndividualHookConfig, MatcherMetadata, get_hook_display_text,
    get_hook_event_metadata, get_hooks_for_matcher as get_hooks_config, get_matcher_metadata,
    group_hooks_by_event_and_matcher, hook_source_description_display_string,
    hook_source_header_display_string, hook_source_inline_display_string,
    is_hook_equal as is_hook_equal_config, sort_matchers_by_priority as sort_matchers_config,
};
pub use hooks_config_snapshot::{
    HookMatcher, HooksSettings, capture_hooks_config_snapshot, get_hooks_config_from_snapshot,
    reset_hooks_config_snapshot, should_allow_managed_hooks_only,
    should_disable_all_hooks_including_managed, update_hooks_config_snapshot,
};
pub use hooks_settings::{
    DEFAULT_HOOK_SHELL, EditableSettingSource, HOOK_EVENTS as HOOK_EVENTS_SETTINGS,
    HookCommand as HooksSettingsHookCommand, HookEvent as HooksSettingsEvent,
    HookSource as HooksSettingsSource, IndividualHookConfig as HooksSettingsIndividualHookConfig,
    SOURCES, get_all_hooks, get_hook_display_text as get_hook_display_text_settings,
    get_hooks_for_event,
    hook_source_description_display_string as hook_source_description_display_string_settings,
    hook_source_header_display_string as hook_source_header_display_string_settings,
    hook_source_inline_display_string as hook_source_inline_display_string_settings, is_hook_equal,
    sort_matchers_by_priority as sort_matchers_by_priority_settings,
};
pub use post_sampling_hooks::{
    PostSamplingHook, ReplHookContext as PostSamplingReplHookContext, clear_post_sampling_hooks,
    execute_post_sampling_hooks, register_post_sampling_hook,
};
pub use register_frontmatter_hooks::{
    HookMatcher as FrontmatterHookMatcher, HooksSettings as FrontmatterHooksSettings,
    register_frontmatter_hooks,
};
pub use register_skill_hooks::{
    HookMatcher as SkillHookMatcher, HooksSettings as SkillHooksSettings, register_skill_hooks,
    register_hooks_from_skills,
};
pub use session_hooks::{
    AggregatedHookResult, FunctionHook, FunctionHookCallback, FunctionHookMatcher, OnHookSuccess,
    SessionDerivedHookMatcher, SessionHookCommand, SessionHookEntry, SessionHookMatcher,
    SessionStore, add_function_hook, add_session_hook, clear_session_hooks,
    get_session_function_hooks, get_session_hook_callback, get_session_hooks, remove_function_hook,
    remove_session_hook,
};
pub use skill_improvement::{
    SkillImprovementSuggestion, SkillUpdate, apply_skill_improvement, init_skill_improvement,
};
pub use ssrf_guard::{
    DnsLookupResult, LookupAddress, SsrfError, create_ssrf_protected_connector, is_blocked_address,
    ssrf_guarded_lookup, ssrf_guarded_lookup_async,
};
