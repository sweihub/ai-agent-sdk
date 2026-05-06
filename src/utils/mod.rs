//! Utility modules

pub mod abort_controller;
pub mod path;
pub mod circular_buffer;
pub mod combined_abort_signal;
pub mod commit_attribution;
pub mod concurrent;
pub mod config;
pub mod context;
pub mod cwd;
pub mod diff;
pub mod env_utils;
pub mod env_validation;
pub mod errors;
pub mod file_state_cache;
pub mod forked_agent;
pub mod git;
pub mod git_diff;
pub mod git_settings;
pub mod github_repo_path_mapping;
pub mod settings;
pub mod hooks;
pub mod messages;
pub mod model;
pub mod mtls;
pub mod pdf_utils;
pub mod permissions;
pub mod plugins;
pub mod process_user_input;
pub mod query_helpers;
pub mod shell;
pub mod side_query;
pub mod swarm;
pub mod system_theme;
pub mod task;
pub mod task_list;
pub mod tempfile;
pub mod theme;
pub mod thinking;
pub mod tool_errors;
pub mod ultraplan;
pub mod user;
pub mod user_agent;
pub mod uuid;
pub mod which;
pub mod words;
pub mod xdg;
pub mod yaml;
pub mod xml;
pub mod zod_to_json_schema;

pub use abort_controller::{
    AbortController, AbortSignal, create_abort_controller, create_abort_controller_default,
    create_child_abort_controller,
};
pub use config::{
    AccountInfo, AutoUpdaterDisabledReason, DiffTool, EditorMode, GlobalConfig, InstallMethod,
    McpServerConfig, NotificationChannel, ProjectConfig, ReleaseChannel, ThemeSetting,
    check_has_trust_dialog_accepted, get_current_project_config, get_global_config,
    get_global_config_path, get_or_create_user_id, is_auto_updater_disabled,
    save_current_project_config, save_global_config,
};
pub use cwd::{get_cwd, get_original_cwd, pwd, run_with_cwd_override, set_cwd};
pub use env_utils::{
    get_aws_region, get_claude_config_home_dir, get_default_vertex_region, get_teams_dir,
    get_user_type, get_vertex_region_for_model, has_node_option, is_ant_user, is_bare_mode,
    is_env_defined_falsy, is_env_truthy, is_in_protected_namespace, is_running_on_homespace,
    is_test_mode, parse_env_vars, should_maintain_project_working_dir,
};
pub use errors::error_message;
pub use messages::{
    Message, MessageContent, NormalizedMessage, extract_tag, get_last_assistant_message,
    get_progress_messages_from_lookup, get_sibling_tool_use_ids_from_lookup, get_tool_result_ids,
    is_classifier_denial, is_not_empty_message, is_tool_use_request_message,
    is_tool_use_result_message, reorder_attachments_for_api,
};
pub use thinking::{
    ThinkingConfig, find_thinking_trigger_positions, has_ultrathink_keyword, is_ultrathink_enabled,
    model_supports_adaptive_thinking, model_supports_thinking, should_enable_thinking_by_default,
};
pub use uuid::{create_agent_id, generate_uuid, validate_uuid};
pub use words::{generate_short_word_slug, generate_word_slug};

// Re-export shell utilities
pub use shell::{
    BashShellProvider, PowerShellProvider,
    shell_provider::{ShellError, ShellExecCommand},
    shell_tool_utils::{SHELL_TYPES, ShellType},
};

// Re-export side_query utilities
pub use side_query::{
    SideQueryMemorySelection, SideQueryOptions, side_query, side_query_simple,
    side_query_with_tools,
};

// Re-export tempfile utilities
pub use tempfile::{generate_temp_file_path, generate_temp_file_path_default};

// Re-export which utilities
pub use which::{which, which_sync};

// Re-export yaml utilities
pub use xdg::{get_user_bin_dir, get_xdg_cache_home, get_xdg_data_home, get_xdg_state_home};
pub use yaml::parse_yaml;

// Re-export commit attribution
pub use commit_attribution::{
    AttributionData, AttributionSnapshotMessage, AttributionState, AttributionSummary,
    FileAttribution, FileAttributionState, FileChange, FileChangeType, SessionBaseline,
    SurfaceBreakdown, attribution_restore_state_from_log, build_surface_key,
    calculate_commit_attribution, compute_content_hash, create_empty_attribution_state,
    expand_file_path, get_attribution_repo_root, get_client_surface, get_file_mtime,
    get_repo_class_cached, get_staged_files, increment_prompt_count, is_internal_model_repo,
    is_internal_model_repo_cached, normalize_file_path, restore_attribution_state_from_snapshots,
    sanitize_model_name, sanitize_surface_key, state_to_snapshot_message, track_bulk_file_changes,
    track_file_creation, track_file_deletion, track_file_modification,
};

// Re-export plugin utilities
pub use plugins::{
    KnownMarketplace, KnownMarketplacesFile, PluginId, PluginMarketplace, PluginMarketplaceEntry,
    PluginMarketplaceMetadata, PluginMarketplaceOwner, PluginSource, get_known_marketplace_names,
    get_marketplace_cache_only, get_plugin_by_id_cache_only, parse_plugin_identifier,
};

// Re-export file state cache utilities
pub use file_state_cache::{
    DEFAULT_MAX_CACHE_SIZE_BYTES, FileState, FileStateCache, READ_FILE_STATE_CACHE_SIZE,
    cache_keys, cache_to_object, clone_file_state_cache, create_file_state_cache_with_size_limit,
    merge_file_state_caches,
};

// Re-export process_user_input utilities
pub use process_user_input::{
    AgentDefinitions, ContentBlockParam, CursorPosition, EffortValue, IdeSelection,
    ImageDimensions, ImageSource, PastedContent, ProcessUserInput, ProcessUserInputBaseResult,
    ProcessUserInputContext, ProcessUserInputContextOptions, ProcessUserInputOptions,
    PromptInputMode, QuerySource, QueryTracking, process_user_input,
};

// Re-export model utilities
pub use model::{
    ModelName, ModelSetting, ModelShortName, ModelValidationResult, check_opus_1m_access,
    check_sonnet_1m_access, first_party_name_to_canonical, get_best_model, get_canonical_name,
    get_claude_ai_user_default_model_description, get_default_haiku_model,
    get_default_main_loop_model, get_default_opus_model, get_default_sonnet_model,
    get_main_loop_model, get_public_model_display_name, get_public_model_name,
    get_small_fast_model, get_user_specified_model_setting, is_legacy_model_remap_enabled,
    is_opus_1m_merge_enabled, model_display_string, normalize_model_string_for_api,
    parse_user_specified_model, render_default_model_setting, render_model_name,
    render_model_setting, resolve_skill_model_override, validate_model,
};

// Re-export swarm utilities
pub use swarm::{
    AgentColorName, BackendType, CreatePaneResult, HIDDEN_SESSION_NAME, PLAN_MODE_REQUIRED_ENV_VAR,
    PaneBackendType, PaneId, SWARM_SESSION_NAME, SWARM_VIEW_WINDOW_NAME, SystemPromptMode,
    TEAM_LEAD_NAME, TEAMMATE_COLOR_ENV_VAR, TEAMMATE_COMMAND_ENV_VAR, TMUX_COMMAND,
    TeammateIdentity, TeammateMessage, TeammateSpawnConfig, TeammateSpawnResult,
    get_swarm_socket_name, is_pane_backend,
};

// Re-export theme utilities
pub use system_theme::{
    SystemTheme, get_system_theme_name, resolve_theme_setting, set_cached_system_theme,
    theme_from_osc_color,
};

// Re-export theme (colors)
pub use theme::{
    AnsiColor, DARK_ANSI_THEME, DARK_DALTONIZED_THEME, DARK_THEME, LIGHT_ANSI_THEME,
    LIGHT_DALTONIZED_THEME, LIGHT_THEME, THEME_NAMES, Theme, ThemeColor, get_theme,
    theme_color_to_ansi,
};

// Re-export user utilities
pub use user::{
    CoreUserData, GitHubActionsMetadata, Platform, get_core_user_data, get_git_email,
    get_user_for_analytics, reset_user_cache, set_cached_email,
};

// Re-export task utilities
pub use task::{
    AppState, CircularBuffer, MAX_TASK_OUTPUT_BYTES, MAX_TASK_OUTPUT_BYTES_DISPLAY,
    OUTPUT_FILE_TAG, PANEL_GRACE_MS, POLL_INTERVAL_MS, STATUS_TAG, STOPPED_DISPLAY_MS, SUMMARY_TAG,
    SetAppState, TASK_ID_TAG, TASK_NOTIFICATION_TAG, TASK_TYPE_TAG, TOOL_USE_ID_TAG,
    TaskAttachment, TaskOutput, TaskStateBase, TaskStatus, TaskType, append_task_output,
    apply_task_offsets_and_evictions, cleanup_task_output, evict_task_output, evict_terminal_task,
    flush_task_output, format_task_notification, generate_task_attachments, get_running_tasks,
    get_task_output, get_task_output_delta, get_task_output_path, get_task_output_size,
    init_task_output, init_task_output_as_symlink, is_terminal_task_status, poll_tasks,
    register_task,
};

// Re-export ultraplan utilities
pub use ultraplan::{
    TriggerPosition, find_ultraplan_trigger_positions, find_ultrareview_trigger_positions,
    has_ultraplan_keyword, has_ultrareview_keyword, replace_ultraplan_keyword,
};

// New module exports
pub mod billing;
pub mod completion_cache;
pub mod content_array;
pub mod cursor;
pub mod debug;
pub mod debug_filter;

pub use billing::{
    has_claude_ai_billing_access, has_console_billing_access, set_mock_billing_access_override,
};
pub use completion_cache::{ShellInfo, detect_shell, get_completion_cache_dir};
pub use content_array::insert_block_after_tool_results;
pub use cursor::{
    YankPopResult, can_yank_pop, clear_kill_ring, get_kill_ring_item, get_kill_ring_size,
    get_last_kill, is_vim_punctuation, is_vim_whitespace, is_vim_word_char, push_to_kill_ring,
    record_yank, reset_kill_accumulation, reset_yank_state, update_yank_length, yank_pop,
};
pub use debug::{
    DebugLogLevel, enable_debug_logging, get_debug_file_path, get_debug_filter, get_debug_log_path,
    get_min_debug_log_level, is_debug_mode, is_debug_to_stderr, log_ant_error, log_for_debugging,
};
pub use debug_filter::{
    DebugFilter, extract_debug_categories, parse_debug_filter, should_show_debug_categories,
    should_show_debug_message,
};
pub mod gh_pr_status;
pub mod heatmap;
pub mod horizontal_scroll;
pub mod http;
pub mod hyperlink;
pub mod ide_path_conversion;
pub mod idle_timeout;
pub mod image_store;
pub mod image_validation;
pub mod immediate_command;
// Source: from ink (~/claudecode/openclaudecode/src/ink)
// ui_event renamed to interact

// New modules from TypeScript translation
pub mod inspector;
pub mod managed_env_constants;
pub mod memoize;
pub mod memory_file_detection;
pub mod modifiers;
pub mod native_installer;
pub mod notebook;
pub mod paste_store;
pub mod plan_mode_v2;
pub mod plans;
pub mod powershell;
pub mod process;
pub mod prompt_editor;
pub mod prompt_shell_execution;
pub mod proxy;
pub mod query_context;
pub mod query_guard;
pub mod query_profiler;
pub mod queue_processor;
pub mod read_edit_context;
pub mod read_file_in_range;
pub mod release_notes;
pub mod render_options;
pub mod ripgrep;
pub mod sandbox;
pub mod sanitization;
pub mod screenshot_clipboard;
pub mod sdk_event_queue;
pub mod secure_storage;
pub mod semantic_boolean;
pub mod semantic_number;
pub mod semver;
pub mod sequential;
pub mod session_activity;
pub mod session_env_vars;
pub mod session_environment;
pub mod session_file_access_hooks;
pub mod session_ingress_auth;
pub mod session_restore;
pub mod session_start;
pub mod session_state;
pub mod session_storage;
pub mod session_storage_portable;
pub mod session_title;
pub mod session_url;

pub use mtls::{
    clear_mtls_cache, configure_mtls, get_ca_cert, get_client_cert, get_client_key, is_mtls_enabled,
};
pub use proxy::{
    clear_proxy_cache, configure_global_agents, get_http_proxy, get_https_proxy, get_proxy_config,
    should_bypass_proxy,
};
pub use semantic_boolean::{is_falsy, is_truthy, parse_env_bool, to_bool};
pub use semantic_number::{format_with_suffix, parse_byte_size, parse_semantic_number};
pub use session_env_vars::{
    clear_session_environment, get_session_environment, set_session_environment,
};

pub use managed_env_constants::{
    DANGEROUS_SHELL_SETTINGS, SAFE_ENV_VARS, is_provider_managed_env_var, is_safe_env_var,
};
pub use memoize::{memoize_with_lru, memoize_with_ttl, memoize_with_ttl_async};
pub use memory_file_detection::{
    detect_session_file_type, detect_session_pattern_type, is_auto_managed_memory_file,
    is_memory_directory, is_shell_command_targeting_memory,
};
pub use pdf_utils::{extension_for_mime_type, is_binary_content_type, is_likely_pdf};
pub use powershell::{escape_powershell_string, is_powershell_available};

pub use modifiers::{Modifier, Shortcut};
pub use native_installer::{install_package, is_native_installer_available};
pub use notebook::{Notebook, extract_code_cells, is_notebook_file, parse_notebook};
pub use paste_store::{PasteItem, PasteStore};
pub use plan_mode_v2::{
    get_plan_mode_v2_agent_count, get_plan_mode_v2_explore_agent_count,
    is_plan_mode_interview_phase_enabled, is_plan_mode_v2_enabled,
};
pub use plans::{Plan, PlanStatus, PlanStep, StepStatus};
pub use process::{get_process_id, get_process_info, is_running_in_container};
pub use prompt_editor::{PromptEditorConfig, PromptTemplate};
pub use prompt_shell_execution::{
    FrontmatterShell, build_shell_command, execute_prompt_shell,
    execute_shell_commands_in_prompt,
};
pub use query_context::{QueryContext, QueryMatch, QueryResult as QueryContextResult};
pub use query_guard::{QueryGuard, QueryGuardError};
pub use query_helpers::{parse_rg_output, search_with_rg};
pub use query_profiler::QueryProfiler;
pub use queue_processor::QueueProcessor;
pub use read_file_in_range::{get_line_count, read_bytes_in_range, read_file_in_range};
pub use render_options::RenderOptions;
pub use ripgrep::{is_ripgrep_available, ripgrep_files};
pub use sandbox::{get_sandbox_dir, is_path_in_sandbox, is_sandbox_enabled};
pub use sanitization::escape_shell_arg;
pub use sanitization::{escape_html, sanitize_filename, sanitize_path, truncate};
pub use screenshot_clipboard::{copy_screenshot_to_clipboard, take_screenshot};
pub use sdk_event_queue::{
    DrainedSdkEvent, SdkEvent, SdkEventQueue, SdkEventType, SdkEventUsage, TaskProgressParams,
    drain_sdk_events, emit_session_state_changed, emit_task_progress, emit_task_started,
    emit_task_terminated_sdk, enqueue_sdk_event,
};
pub use secure_storage::SecureStorage;
pub use semver::{Semver, parse_semver};
pub use session_activity::{ActivityType, SessionActivity, SessionActivityTracker};
pub use session_restore::{can_restore_session, restore_session};
pub use session_start::{SessionStartConfig, create_session};
pub use session_state::SessionState;
pub use session_title::{clean_title_for_filename, generate_session_title};
pub use session_url::{build_session_url, extract_session_id, is_valid_session_url};

// New modules from TypeScript translation (April 2026)
pub mod conversation_recovery;
pub mod exec_file_no_throw;
pub mod platform;
pub mod set;
pub mod subprocess_env;

pub use exec_file_no_throw::{
    ExecResult, exec_file_no_throw, exec_file_no_throw_sync, exec_file_no_throw_with_cwd,
};
pub use platform::{SUPPORTED_PLATFORMS, detect_platform, get_platform};
pub use set::{difference, every, intersects, union};
pub use subprocess_env::{GHA_SUBPROCESS_SCRUB, register_upstream_proxy_env_fn, subprocess_env};

// Token budget / token counting utilities
pub mod token_budget;
pub mod tokens;
pub mod analyze_context;
pub mod collapse_read_search;
