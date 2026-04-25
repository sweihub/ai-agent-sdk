// Source: /data/home/swei/claudecode/openclaudecode/src/bootstrap/state.ts
#![allow(dead_code)]

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use uuid::Uuid;

pub type SessionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelEntry {
    Plugin {
        name: String,
        marketplace: String,
        dev: Option<bool>,
    },
    Server {
        name: String,
        dev: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSetting {
    pub value: Option<String>,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStrings {
    pub region_string: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLogEntry {
    pub error: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub created_at: u64,
    pub recurring: Option<bool>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeleportedSessionInfo {
    pub is_teleported: bool,
    pub has_logged_first_message: bool,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokedSkillInfo {
    pub skill_name: String,
    pub skill_path: String,
    pub content: String,
    pub invoked_at: u64,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowOperation {
    pub operation: String,
    pub duration_ms: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    pub name: Option<String>,
    pub matcher: Option<serde_json::Value>,
    pub plugin_root: Option<String>,
}

pub struct CostStateRestoreParams {
    pub total_cost_usd: f64,
    pub total_api_duration: f64,
    pub total_api_duration_without_retries: f64,
    pub total_tool_duration: f64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub last_duration: Option<u64>,
    pub model_usage: Option<HashMap<String, ModelUsage>>,
}

#[derive(Default)]
pub struct RegenerateOptions {
    pub set_current_as_parent: bool,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn get_session_id() -> String {
    STATE.lock().unwrap().session_id.clone()
}

pub fn regenerate_session_id(options: RegenerateOptions) -> String {
    let mut state = STATE.lock().unwrap();
    if options.set_current_as_parent {
        state.parent_session_id = Some(state.session_id.clone());
    }
    let old_session_id = state.session_id.clone();
    state.plan_slug_cache.remove(&old_session_id);
    let new_id = Uuid::new_v4().to_string();
    state.session_id = new_id.clone();
    state.session_project_dir = None;
    new_id
}

pub fn get_parent_session_id() -> Option<String> {
    STATE.lock().unwrap().parent_session_id.clone()
}

pub fn switch_session(session_id: String, project_dir: Option<String>) {
    let mut state = STATE.lock().unwrap();
    let old_session_id = state.session_id.clone();
    state.plan_slug_cache.remove(&old_session_id);
    state.session_id = session_id;
    state.session_project_dir = project_dir;
}

pub fn get_session_project_dir() -> Option<String> {
    STATE.lock().unwrap().session_project_dir.clone()
}

pub fn get_original_cwd() -> String {
    STATE.lock().unwrap().original_cwd.clone()
}

pub fn get_project_root() -> String {
    STATE.lock().unwrap().project_root.clone()
}

pub fn set_original_cwd(cwd: String) {
    STATE.lock().unwrap().original_cwd = cwd;
}

pub fn set_project_root(cwd: String) {
    STATE.lock().unwrap().project_root = cwd;
}

pub fn get_cwd_state() -> String {
    STATE.lock().unwrap().cwd.clone()
}

pub fn set_cwd_state(cwd: String) {
    STATE.lock().unwrap().cwd = cwd;
}

pub fn get_direct_connect_server_url() -> Option<String> {
    STATE.lock().unwrap().direct_connect_server_url.clone()
}

pub fn set_direct_connect_server_url(url: String) {
    STATE.lock().unwrap().direct_connect_server_url = Some(url);
}

pub fn add_to_total_duration_state(duration: f64, duration_without_retries: f64) {
    let mut state = STATE.lock().unwrap();
    state.total_api_duration += duration;
    state.total_api_duration_without_retries += duration_without_retries;
}

pub fn reset_total_duration_state_and_cost_for_tests_only() {
    let mut state = STATE.lock().unwrap();
    state.total_api_duration = 0.0;
    state.total_api_duration_without_retries = 0.0;
    state.total_cost_usd = 0.0;
}

pub fn add_to_total_cost_state(cost: f64, model_usage: ModelUsage, model: String) {
    let mut state = STATE.lock().unwrap();
    state.model_usage.insert(model, model_usage);
    state.total_cost_usd += cost;
}

pub fn get_total_cost_usd() -> f64 {
    STATE.lock().unwrap().total_cost_usd
}

pub fn get_total_api_duration() -> f64 {
    STATE.lock().unwrap().total_api_duration
}

pub fn get_total_duration() -> u64 {
    current_timestamp() - STATE.lock().unwrap().start_time
}

pub fn get_total_api_duration_without_retries() -> f64 {
    STATE.lock().unwrap().total_api_duration_without_retries
}

pub fn get_total_tool_duration() -> f64 {
    STATE.lock().unwrap().total_tool_duration
}

pub fn add_to_tool_duration(duration: f64) {
    let mut state = STATE.lock().unwrap();
    state.total_tool_duration += duration;
    state.turn_tool_duration_ms += duration;
    state.turn_tool_count += 1;
}

pub fn get_turn_hook_duration_ms() -> f64 {
    STATE.lock().unwrap().turn_hook_duration_ms
}

pub fn add_to_turn_hook_duration(duration: f64) {
    let mut state = STATE.lock().unwrap();
    state.turn_hook_duration_ms += duration;
    state.turn_hook_count += 1;
}

pub fn reset_turn_hook_duration() {
    let mut state = STATE.lock().unwrap();
    state.turn_hook_duration_ms = 0.0;
    state.turn_hook_count = 0;
}

pub fn get_turn_hook_count() -> u64 {
    STATE.lock().unwrap().turn_hook_count
}

pub fn get_turn_tool_duration_ms() -> f64 {
    STATE.lock().unwrap().turn_tool_duration_ms
}

pub fn reset_turn_tool_duration() {
    let mut state = STATE.lock().unwrap();
    state.turn_tool_duration_ms = 0.0;
    state.turn_tool_count = 0;
}

pub fn get_turn_tool_count() -> u64 {
    STATE.lock().unwrap().turn_tool_count
}

pub fn get_turn_classifier_duration_ms() -> f64 {
    STATE.lock().unwrap().turn_classifier_duration_ms
}

pub fn add_to_turn_classifier_duration(duration: f64) {
    let mut state = STATE.lock().unwrap();
    state.turn_classifier_duration_ms += duration;
    state.turn_classifier_count += 1;
}

pub fn reset_turn_classifier_duration() {
    let mut state = STATE.lock().unwrap();
    state.turn_classifier_duration_ms = 0.0;
    state.turn_classifier_count = 0;
}

pub fn get_turn_classifier_count() -> u64 {
    STATE.lock().unwrap().turn_classifier_count
}

pub fn get_last_interaction_time() -> u64 {
    STATE.lock().unwrap().last_interaction_time
}

pub fn add_to_total_lines_changed(added: u64, removed: u64) {
    let mut state = STATE.lock().unwrap();
    state.total_lines_added += added;
    state.total_lines_removed += removed;
}

pub fn get_total_lines_added() -> u64 {
    STATE.lock().unwrap().total_lines_added
}

pub fn get_total_lines_removed() -> u64 {
    STATE.lock().unwrap().total_lines_removed
}

pub fn get_total_input_tokens() -> u64 {
    STATE
        .lock()
        .unwrap()
        .model_usage
        .values()
        .map(|u| u.input_tokens)
        .sum()
}

pub fn get_total_output_tokens() -> u64 {
    STATE
        .lock()
        .unwrap()
        .model_usage
        .values()
        .map(|u| u.output_tokens)
        .sum()
}

pub fn get_total_cache_read_input_tokens() -> u64 {
    STATE
        .lock()
        .unwrap()
        .model_usage
        .values()
        .map(|u| u.cache_read_input_tokens)
        .sum()
}

pub fn get_total_cache_creation_input_tokens() -> u64 {
    STATE
        .lock()
        .unwrap()
        .model_usage
        .values()
        .map(|u| u.cache_creation_input_tokens)
        .sum()
}

pub fn get_total_web_search_requests() -> u64 {
    STATE
        .lock()
        .unwrap()
        .model_usage
        .values()
        .map(|u| u.web_search_requests)
        .sum()
}

pub fn get_turn_output_tokens() -> u64 {
    let state = STATE.lock().unwrap();
    let total = state.model_usage.values().map(|u| u.output_tokens).sum::<u64>();
    total.saturating_sub(state.output_tokens_at_turn_start)
}

/// Per-turn output token snapshotting (matches TypeScript bootstrap/state.ts)
pub fn snapshot_output_tokens_for_turn(budget: Option<f64>) {
    let mut state = STATE.lock().unwrap();
    state.output_tokens_at_turn_start = state.model_usage.values().map(|u| u.output_tokens).sum();
    state.current_turn_token_budget = budget;
    state.budget_continuation_count = 0;
}

pub fn get_current_turn_token_budget() -> Option<f64> {
    STATE.lock().unwrap().current_turn_token_budget
}

pub fn get_budget_continuation_count() -> u64 {
    STATE.lock().unwrap().budget_continuation_count
}

pub fn increment_budget_continuation_count() {
    let mut state = STATE.lock().unwrap();
    state.budget_continuation_count += 1;
}

pub fn set_has_unknown_model_cost() {
    STATE.lock().unwrap().has_unknown_model_cost = true;
}

pub fn has_unknown_model_cost() -> bool {
    STATE.lock().unwrap().has_unknown_model_cost
}

pub fn get_last_main_request_id() -> Option<String> {
    STATE.lock().unwrap().last_main_request_id.clone()
}

pub fn set_last_main_request_id(request_id: String) {
    STATE.lock().unwrap().last_main_request_id = Some(request_id);
}

pub fn get_last_api_completion_timestamp() -> Option<u64> {
    STATE.lock().unwrap().last_api_completion_timestamp
}

pub fn set_last_api_completion_timestamp(timestamp: u64) {
    STATE.lock().unwrap().last_api_completion_timestamp = Some(timestamp);
}

pub fn mark_post_compaction() {
    STATE.lock().unwrap().pending_post_compaction = true;
}

pub fn consume_post_compaction() -> bool {
    let mut state = STATE.lock().unwrap();
    let was = state.pending_post_compaction;
    state.pending_post_compaction = false;
    was
}

pub fn get_model_usage() -> HashMap<String, ModelUsage> {
    STATE.lock().unwrap().model_usage.clone()
}

pub fn get_usage_for_model(model: &str) -> Option<ModelUsage> {
    STATE.lock().unwrap().model_usage.get(model).cloned()
}

pub fn get_main_loop_model_override() -> Option<ModelSetting> {
    STATE.lock().unwrap().main_loop_model_override.clone()
}

pub fn get_initial_main_loop_model() -> Option<ModelSetting> {
    STATE.lock().unwrap().initial_main_loop_model.clone()
}

pub fn set_main_loop_model_override(model: Option<ModelSetting>) {
    STATE.lock().unwrap().main_loop_model_override = model;
}

pub fn set_initial_main_loop_model(model: ModelSetting) {
    STATE.lock().unwrap().initial_main_loop_model = Some(model);
}

pub fn get_sdk_betas() -> Option<Vec<String>> {
    STATE.lock().unwrap().sdk_betas.clone()
}

pub fn set_sdk_betas(betas: Option<Vec<String>>) {
    STATE.lock().unwrap().sdk_betas = betas;
}

pub fn reset_cost_state() {
    let mut state = STATE.lock().unwrap();
    state.total_cost_usd = 0.0;
    state.total_api_duration = 0.0;
    state.total_api_duration_without_retries = 0.0;
    state.total_tool_duration = 0.0;
    state.start_time = current_timestamp();
    state.total_lines_added = 0;
    state.total_lines_removed = 0;
    state.has_unknown_model_cost = false;
    state.model_usage.clear();
    state.prompt_id = None;
}

pub fn set_cost_state_for_restore(params: CostStateRestoreParams) {
    let mut state = STATE.lock().unwrap();
    state.total_cost_usd = params.total_cost_usd;
    state.total_api_duration = params.total_api_duration;
    state.total_api_duration_without_retries = params.total_api_duration_without_retries;
    state.total_tool_duration = params.total_tool_duration;
    state.total_lines_added = params.total_lines_added;
    state.total_lines_removed = params.total_lines_removed;

    if let Some(model_usage) = params.model_usage {
        state.model_usage = model_usage;
    }

    if let Some(last_duration) = params.last_duration {
        state.start_time = current_timestamp() - last_duration;
    }
}

pub fn get_model_strings() -> Option<ModelStrings> {
    STATE.lock().unwrap().model_strings.clone()
}

pub fn set_model_strings(model_strings: ModelStrings) {
    STATE.lock().unwrap().model_strings = Some(model_strings);
}

pub fn reset_model_strings_for_testing_only() {
    STATE.lock().unwrap().model_strings = None;
}

pub fn get_is_non_interactive_session() -> bool {
    !STATE.lock().unwrap().is_interactive
}

pub fn get_is_interactive() -> bool {
    STATE.lock().unwrap().is_interactive
}

pub fn set_is_interactive(value: bool) {
    STATE.lock().unwrap().is_interactive = value;
}

pub fn get_client_type() -> String {
    STATE.lock().unwrap().client_type.clone()
}

pub fn set_client_type(type_: String) {
    STATE.lock().unwrap().client_type = type_;
}

pub fn get_sdk_agent_progress_summaries_enabled() -> bool {
    STATE.lock().unwrap().sdk_agent_progress_summaries_enabled
}

pub fn set_sdk_agent_progress_summaries_enabled(value: bool) {
    STATE.lock().unwrap().sdk_agent_progress_summaries_enabled = value;
}

pub fn get_kairos_active() -> bool {
    STATE.lock().unwrap().kairos_active
}

pub fn set_kairos_active(value: bool) {
    STATE.lock().unwrap().kairos_active = value;
}

pub fn get_strict_tool_result_pairing() -> bool {
    STATE.lock().unwrap().strict_tool_result_pairing
}

pub fn set_strict_tool_result_pairing(value: bool) {
    STATE.lock().unwrap().strict_tool_result_pairing = value;
}

pub fn get_user_msg_opt_in() -> bool {
    STATE.lock().unwrap().user_msg_opt_in
}

pub fn set_user_msg_opt_in(value: bool) {
    STATE.lock().unwrap().user_msg_opt_in = value;
}

pub fn get_session_source() -> Option<String> {
    STATE.lock().unwrap().session_source.clone()
}

pub fn set_session_source(source: String) {
    STATE.lock().unwrap().session_source = Some(source);
}

pub fn get_question_preview_format() -> Option<String> {
    STATE.lock().unwrap().question_preview_format.clone()
}

pub fn set_question_preview_format(format: String) {
    STATE.lock().unwrap().question_preview_format = Some(format);
}

pub fn get_agent_color_map() -> HashMap<String, String> {
    STATE.lock().unwrap().agent_color_map.clone()
}

pub fn get_flag_settings_path() -> Option<String> {
    STATE.lock().unwrap().flag_settings_path.clone()
}

pub fn set_flag_settings_path(path: Option<String>) {
    STATE.lock().unwrap().flag_settings_path = path;
}

pub fn get_flag_settings_inline() -> Option<HashMap<String, serde_json::Value>> {
    STATE.lock().unwrap().flag_settings_inline.clone()
}

pub fn set_flag_settings_inline(settings: Option<HashMap<String, serde_json::Value>>) {
    STATE.lock().unwrap().flag_settings_inline = settings;
}

pub fn get_session_ingress_token() -> Option<String> {
    STATE.lock().unwrap().session_ingress_token.clone()
}

pub fn set_session_ingress_token(token: Option<String>) {
    STATE.lock().unwrap().session_ingress_token = token;
}

pub fn get_oauth_token_from_fd() -> Option<String> {
    STATE.lock().unwrap().oauth_token_from_fd.clone()
}

pub fn set_oauth_token_from_fd(token: Option<String>) {
    STATE.lock().unwrap().oauth_token_from_fd = token;
}

pub fn get_api_key_from_fd() -> Option<String> {
    STATE.lock().unwrap().api_key_from_fd.clone()
}

pub fn set_api_key_from_fd(key: Option<String>) {
    STATE.lock().unwrap().api_key_from_fd = key;
}

pub fn set_last_api_request(params: Option<serde_json::Value>) {
    STATE.lock().unwrap().last_api_request = params;
}

pub fn get_last_api_request() -> Option<serde_json::Value> {
    STATE.lock().unwrap().last_api_request.clone()
}

pub fn set_last_api_request_messages(messages: Option<serde_json::Value>) {
    STATE.lock().unwrap().last_api_request_messages = messages;
}

pub fn get_last_api_request_messages() -> Option<serde_json::Value> {
    STATE.lock().unwrap().last_api_request_messages.clone()
}

pub fn set_last_classifier_requests(requests: Option<Vec<serde_json::Value>>) {
    STATE.lock().unwrap().last_classifier_requests = requests;
}

pub fn get_last_classifier_requests() -> Option<Vec<serde_json::Value>> {
    STATE.lock().unwrap().last_classifier_requests.clone()
}

pub fn set_cached_claude_md_content(content: Option<String>) {
    STATE.lock().unwrap().cached_claude_md_content = content;
}

pub fn get_cached_claude_md_content() -> Option<String> {
    STATE.lock().unwrap().cached_claude_md_content.clone()
}

pub fn add_to_in_memory_error_log(error_info: ErrorLogEntry) {
    const MAX_IN_MEMORY_ERRORS: usize = 100;
    let mut state = STATE.lock().unwrap();
    if state.in_memory_error_log.len() >= MAX_IN_MEMORY_ERRORS {
        state.in_memory_error_log.remove(0);
    }
    state.in_memory_error_log.push(error_info);
}

pub fn get_allowed_setting_sources() -> Vec<String> {
    STATE.lock().unwrap().allowed_setting_sources.clone()
}

pub fn set_allowed_setting_sources(sources: Vec<String>) {
    STATE.lock().unwrap().allowed_setting_sources = sources;
}

pub fn prefer_third_party_authentication() -> bool {
    let state = STATE.lock().unwrap();
    !state.is_interactive && state.client_type != "claude-vscode"
}

pub fn set_inline_plugins(plugins: Vec<String>) {
    STATE.lock().unwrap().inline_plugins = plugins;
}

pub fn get_inline_plugins() -> Vec<String> {
    STATE.lock().unwrap().inline_plugins.clone()
}

pub fn set_chrome_flag_override(value: Option<bool>) {
    STATE.lock().unwrap().chrome_flag_override = value;
}

pub fn get_chrome_flag_override() -> Option<bool> {
    STATE.lock().unwrap().chrome_flag_override
}

pub fn set_use_cowork_plugins(value: bool) {
    STATE.lock().unwrap().use_cowork_plugins = value;
}

pub fn get_use_cowork_plugins() -> bool {
    STATE.lock().unwrap().use_cowork_plugins
}

pub fn set_session_bypass_permissions_mode(enabled: bool) {
    STATE.lock().unwrap().session_bypass_permissions_mode = enabled;
}

pub fn get_session_bypass_permissions_mode() -> bool {
    STATE.lock().unwrap().session_bypass_permissions_mode
}

pub fn set_scheduled_tasks_enabled(enabled: bool) {
    STATE.lock().unwrap().scheduled_tasks_enabled = enabled;
}

pub fn get_scheduled_tasks_enabled() -> bool {
    STATE.lock().unwrap().scheduled_tasks_enabled
}

pub fn get_session_cron_tasks() -> Vec<SessionCronTask> {
    STATE.lock().unwrap().session_cron_tasks.clone()
}

pub fn add_session_cron_task(task: SessionCronTask) {
    STATE.lock().unwrap().session_cron_tasks.push(task);
}

pub fn remove_session_cron_tasks(ids: &[String]) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let mut state = STATE.lock().unwrap();
    let id_set: HashSet<String> = ids.iter().cloned().collect();
    let initial_len = state.session_cron_tasks.len();
    state.session_cron_tasks.retain(|t| !id_set.contains(&t.id));
    let removed = initial_len - state.session_cron_tasks.len();
    if removed == 0 {
        return 0;
    }
    removed
}

pub fn set_session_trust_accepted(accepted: bool) {
    STATE.lock().unwrap().session_trust_accepted = accepted;
}

pub fn get_session_trust_accepted() -> bool {
    STATE.lock().unwrap().session_trust_accepted
}

pub fn set_session_persistence_disabled(disabled: bool) {
    STATE.lock().unwrap().session_persistence_disabled = disabled;
}

pub fn is_session_persistence_disabled() -> bool {
    STATE.lock().unwrap().session_persistence_disabled
}

pub fn has_exited_plan_mode_in_session() -> bool {
    STATE.lock().unwrap().has_exited_plan_mode
}

pub fn set_has_exited_plan_mode(value: bool) {
    STATE.lock().unwrap().has_exited_plan_mode = value;
}

pub fn needs_plan_mode_exit_attachment() -> bool {
    STATE.lock().unwrap().needs_plan_mode_exit_attachment
}

pub fn set_needs_plan_mode_exit_attachment(value: bool) {
    STATE.lock().unwrap().needs_plan_mode_exit_attachment = value;
}

pub fn handle_plan_mode_transition(from_mode: &str, to_mode: &str) {
    let mut state = STATE.lock().unwrap();
    if to_mode == "plan" && from_mode != "plan" {
        state.needs_plan_mode_exit_attachment = false;
    }
    if from_mode == "plan" && to_mode != "plan" {
        state.needs_plan_mode_exit_attachment = true;
    }
}

pub fn needs_auto_mode_exit_attachment() -> bool {
    STATE.lock().unwrap().needs_auto_mode_exit_attachment
}

pub fn set_needs_auto_mode_exit_attachment(value: bool) {
    STATE.lock().unwrap().needs_auto_mode_exit_attachment = value;
}

pub fn handle_auto_mode_transition(from_mode: &str, to_mode: &str) {
    let mut state = STATE.lock().unwrap();
    if (from_mode == "auto" && to_mode == "plan") || (from_mode == "plan" && to_mode == "auto") {
        return;
    }
    let from_is_auto = from_mode == "auto";
    let to_is_auto = to_mode == "auto";

    if to_is_auto && !from_is_auto {
        state.needs_auto_mode_exit_attachment = false;
    }
    if from_is_auto && !to_is_auto {
        state.needs_auto_mode_exit_attachment = true;
    }
}

pub fn has_shown_lsp_recommendation_this_session() -> bool {
    STATE.lock().unwrap().lsp_recommendation_shown_this_session
}

pub fn set_lsp_recommendation_shown_this_session(value: bool) {
    STATE.lock().unwrap().lsp_recommendation_shown_this_session = value;
}

pub fn set_init_json_schema(schema: HashMap<String, serde_json::Value>) {
    STATE.lock().unwrap().init_json_schema = Some(schema);
}

pub fn get_init_json_schema() -> Option<HashMap<String, serde_json::Value>> {
    STATE.lock().unwrap().init_json_schema.clone()
}

pub fn get_plan_slug_cache() -> HashMap<String, String> {
    STATE.lock().unwrap().plan_slug_cache.clone()
}

pub fn get_session_created_teams() -> HashSet<String> {
    STATE.lock().unwrap().session_created_teams.clone()
}

pub fn set_teleported_session_info(info: TeleportedSessionInfo) {
    STATE.lock().unwrap().teleported_session_info = Some(info);
}

pub fn get_teleported_session_info() -> Option<TeleportedSessionInfo> {
    STATE.lock().unwrap().teleported_session_info.clone()
}

pub fn mark_first_teleport_message_logged() {
    let mut state = STATE.lock().unwrap();
    if let Some(info) = state.teleported_session_info.as_mut() {
        info.has_logged_first_message = true;
    }
}

pub fn add_invoked_skill(
    skill_name: String,
    skill_path: String,
    content: String,
    agent_id: Option<String>,
) {
    let key = format!(
        "{}:{}",
        agent_id.as_ref().unwrap_or(&String::new()),
        skill_name
    );
    let mut state = STATE.lock().unwrap();
    state.invoked_skills.insert(
        key,
        InvokedSkillInfo {
            skill_name,
            skill_path,
            content,
            invoked_at: current_timestamp(),
            agent_id,
        },
    );
}

pub fn get_invoked_skills() -> HashMap<String, InvokedSkillInfo> {
    STATE.lock().unwrap().invoked_skills.clone()
}

pub fn get_invoked_skills_for_agent(agent_id: Option<&str>) -> HashMap<String, InvokedSkillInfo> {
    let normalized_id = agent_id.map(|s| s.to_string());
    STATE
        .lock()
        .unwrap()
        .invoked_skills
        .iter()
        .filter(|(_, skill)| skill.agent_id == normalized_id)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn clear_invoked_skills(preserved_agent_ids: Option<&HashSet<String>>) {
    let mut state = STATE.lock().unwrap();
    if let Some(ids) = preserved_agent_ids {
        if ids.is_empty() {
            state.invoked_skills.clear();
            return;
        }
        state.invoked_skills.retain(|_, skill| {
            skill.agent_id.is_none() || !ids.contains(skill.agent_id.as_ref().unwrap())
        });
    } else {
        state.invoked_skills.clear();
    }
}

pub fn clear_invoked_skills_for_agent(agent_id: &str) {
    let mut state = STATE.lock().unwrap();
    state
        .invoked_skills
        .retain(|_, skill| skill.agent_id.as_deref() != Some(agent_id));
}

const MAX_SLOW_OPERATIONS: usize = 10;
const SLOW_OPERATION_TTL_MS: u64 = 10000;

pub fn add_slow_operation(operation: String, duration_ms: f64) {
    let mut state = STATE.lock().unwrap();
    let now = current_timestamp();
    state
        .slow_operations
        .retain(|op| now - op.timestamp < SLOW_OPERATION_TTL_MS);
    state.slow_operations.push(SlowOperation {
        operation,
        duration_ms,
        timestamp: now,
    });
    if state.slow_operations.len() > MAX_SLOW_OPERATIONS {
        let len = state.slow_operations.len();
        state.slow_operations = state.slow_operations.split_off(len - MAX_SLOW_OPERATIONS);
    }
}

pub fn get_slow_operations() -> Vec<SlowOperation> {
    let state = STATE.lock().unwrap();
    if state.slow_operations.is_empty() {
        return vec![];
    }
    let now = current_timestamp();
    if state
        .slow_operations
        .iter()
        .any(|op| now - op.timestamp >= SLOW_OPERATION_TTL_MS)
    {
        return vec![];
    }
    state.slow_operations.clone()
}

pub fn get_main_thread_agent_type() -> Option<String> {
    STATE.lock().unwrap().main_thread_agent_type.clone()
}

pub fn set_main_thread_agent_type(agent_type: Option<String>) {
    STATE.lock().unwrap().main_thread_agent_type = agent_type;
}

pub fn get_is_remote_mode() -> bool {
    STATE.lock().unwrap().is_remote_mode
}

pub fn set_is_remote_mode(value: bool) {
    STATE.lock().unwrap().is_remote_mode = value;
}

pub fn get_system_prompt_section_cache() -> HashMap<String, Option<String>> {
    STATE.lock().unwrap().system_prompt_section_cache.clone()
}

pub fn set_system_prompt_section_cache_entry(name: String, value: Option<String>) {
    STATE
        .lock()
        .unwrap()
        .system_prompt_section_cache
        .insert(name, value);
}

pub fn clear_system_prompt_section_state() {
    STATE.lock().unwrap().system_prompt_section_cache.clear();
}

pub fn get_last_emitted_date() -> Option<String> {
    STATE.lock().unwrap().last_emitted_date.clone()
}

pub fn set_last_emitted_date(date: Option<String>) {
    STATE.lock().unwrap().last_emitted_date = date;
}

pub fn get_additional_directories_for_claude_md() -> Vec<String> {
    STATE
        .lock()
        .unwrap()
        .additional_directories_for_claude_md
        .clone()
}

pub fn set_additional_directories_for_claude_md(directories: Vec<String>) {
    STATE.lock().unwrap().additional_directories_for_claude_md = directories;
}

pub fn get_allowed_channels() -> Vec<ChannelEntry> {
    STATE.lock().unwrap().allowed_channels.clone()
}

pub fn set_allowed_channels(entries: Vec<ChannelEntry>) {
    STATE.lock().unwrap().allowed_channels = entries;
}

pub fn get_has_dev_channels() -> bool {
    STATE.lock().unwrap().has_dev_channels
}

pub fn set_has_dev_channels(value: bool) {
    STATE.lock().unwrap().has_dev_channels = value;
}

pub fn get_prompt_cache_1h_allowlist() -> Option<Vec<String>> {
    STATE.lock().unwrap().prompt_cache_1h_allowlist.clone()
}

pub fn set_prompt_cache_1h_allowlist(allowlist: Option<Vec<String>>) {
    STATE.lock().unwrap().prompt_cache_1h_allowlist = allowlist;
}

pub fn get_prompt_cache_1h_eligible() -> Option<bool> {
    STATE.lock().unwrap().prompt_cache_1h_eligible
}

pub fn set_prompt_cache_1h_eligible(eligible: Option<bool>) {
    STATE.lock().unwrap().prompt_cache_1h_eligible = eligible;
}

pub fn get_afk_mode_header_latched() -> Option<bool> {
    STATE.lock().unwrap().afk_mode_header_latched
}

pub fn set_afk_mode_header_latched(v: bool) {
    STATE.lock().unwrap().afk_mode_header_latched = Some(v);
}

pub fn get_fast_mode_header_latched() -> Option<bool> {
    STATE.lock().unwrap().fast_mode_header_latched
}

pub fn set_fast_mode_header_latched(v: bool) {
    STATE.lock().unwrap().fast_mode_header_latched = Some(v);
}

pub fn get_cache_editing_header_latched() -> Option<bool> {
    STATE.lock().unwrap().cache_editing_header_latched
}

pub fn set_cache_editing_header_latched(v: bool) {
    STATE.lock().unwrap().cache_editing_header_latched = Some(v);
}

pub fn get_thinking_clear_latched() -> Option<bool> {
    STATE.lock().unwrap().thinking_clear_latched
}

pub fn set_thinking_clear_latched(v: bool) {
    STATE.lock().unwrap().thinking_clear_latched = Some(v);
}

pub fn clear_beta_header_latches() {
    let mut state = STATE.lock().unwrap();
    state.afk_mode_header_latched = None;
    state.fast_mode_header_latched = None;
    state.cache_editing_header_latched = None;
    state.thinking_clear_latched = None;
}

pub fn get_prompt_id() -> Option<String> {
    STATE.lock().unwrap().prompt_id.clone()
}

pub fn set_prompt_id(id: Option<String>) {
    STATE.lock().unwrap().prompt_id = id;
}

struct State {
    pub original_cwd: String,
    pub project_root: String,
    pub total_cost_usd: f64,
    pub total_api_duration: f64,
    pub total_api_duration_without_retries: f64,
    pub total_tool_duration: f64,
    pub turn_hook_duration_ms: f64,
    pub turn_tool_duration_ms: f64,
    pub turn_classifier_duration_ms: f64,
    pub turn_tool_count: u64,
    pub turn_hook_count: u64,
    pub turn_classifier_count: u64,
    pub start_time: u64,
    pub last_interaction_time: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub has_unknown_model_cost: bool,
    pub cwd: String,
    pub model_usage: HashMap<String, ModelUsage>,
    pub main_loop_model_override: Option<ModelSetting>,
    pub initial_main_loop_model: Option<ModelSetting>,
    pub model_strings: Option<ModelStrings>,
    pub is_interactive: bool,
    pub kairos_active: bool,
    pub strict_tool_result_pairing: bool,
    pub sdk_agent_progress_summaries_enabled: bool,
    pub user_msg_opt_in: bool,
    pub client_type: String,
    pub session_source: Option<String>,
    pub question_preview_format: Option<String>,
    pub flag_settings_path: Option<String>,
    pub flag_settings_inline: Option<HashMap<String, serde_json::Value>>,
    pub allowed_setting_sources: Vec<String>,
    pub session_ingress_token: Option<String>,
    pub oauth_token_from_fd: Option<String>,
    pub api_key_from_fd: Option<String>,
    pub stats_store: Option<()>,
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub agent_color_map: HashMap<String, String>,
    pub agent_color_index: usize,
    pub last_api_request: Option<serde_json::Value>,
    pub last_api_request_messages: Option<serde_json::Value>,
    pub last_classifier_requests: Option<Vec<serde_json::Value>>,
    pub cached_claude_md_content: Option<String>,
    pub in_memory_error_log: Vec<ErrorLogEntry>,
    pub inline_plugins: Vec<String>,
    pub chrome_flag_override: Option<bool>,
    pub use_cowork_plugins: bool,
    pub session_bypass_permissions_mode: bool,
    pub scheduled_tasks_enabled: bool,
    pub session_cron_tasks: Vec<SessionCronTask>,
    pub session_created_teams: HashSet<String>,
    pub session_trust_accepted: bool,
    pub session_persistence_disabled: bool,
    pub has_exited_plan_mode: bool,
    pub needs_plan_mode_exit_attachment: bool,
    pub needs_auto_mode_exit_attachment: bool,
    pub lsp_recommendation_shown_this_session: bool,
    pub init_json_schema: Option<HashMap<String, serde_json::Value>>,
    pub registered_hooks: Option<HashMap<String, Vec<HookMatcher>>>,
    pub plan_slug_cache: HashMap<String, String>,
    pub teleported_session_info: Option<TeleportedSessionInfo>,
    pub invoked_skills: HashMap<String, InvokedSkillInfo>,
    pub slow_operations: Vec<SlowOperation>,
    pub sdk_betas: Option<Vec<String>>,
    pub main_thread_agent_type: Option<String>,
    pub is_remote_mode: bool,
    pub direct_connect_server_url: Option<String>,
    pub system_prompt_section_cache: HashMap<String, Option<String>>,
    pub last_emitted_date: Option<String>,
    pub additional_directories_for_claude_md: Vec<String>,
    pub allowed_channels: Vec<ChannelEntry>,
    pub has_dev_channels: bool,
    pub session_project_dir: Option<String>,
    pub prompt_cache_1h_allowlist: Option<Vec<String>>,
    pub prompt_cache_1h_eligible: Option<bool>,
    pub afk_mode_header_latched: Option<bool>,
    pub fast_mode_header_latched: Option<bool>,
    pub cache_editing_header_latched: Option<bool>,
    pub thinking_clear_latched: Option<bool>,
    pub prompt_id: Option<String>,
    pub last_main_request_id: Option<String>,
    pub last_api_completion_timestamp: Option<u64>,
    pub pending_post_compaction: bool,
    /// Per-turn token snapshotting state
    pub output_tokens_at_turn_start: u64,
    pub current_turn_token_budget: Option<f64>,
    pub budget_continuation_count: u64,
}

fn get_initial_state() -> State {
    let resolved_cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let session_id = Uuid::new_v4().to_string();

    State {
        original_cwd: resolved_cwd.clone(),
        project_root: resolved_cwd.clone(),
        total_cost_usd: 0.0,
        total_api_duration: 0.0,
        total_api_duration_without_retries: 0.0,
        total_tool_duration: 0.0,
        turn_hook_duration_ms: 0.0,
        turn_tool_duration_ms: 0.0,
        turn_classifier_duration_ms: 0.0,
        turn_tool_count: 0,
        turn_hook_count: 0,
        turn_classifier_count: 0,
        start_time: current_timestamp(),
        last_interaction_time: current_timestamp(),
        total_lines_added: 0,
        total_lines_removed: 0,
        has_unknown_model_cost: false,
        cwd: resolved_cwd,
        model_usage: HashMap::new(),
        main_loop_model_override: None,
        initial_main_loop_model: None,
        model_strings: None,
        is_interactive: false,
        kairos_active: false,
        strict_tool_result_pairing: false,
        sdk_agent_progress_summaries_enabled: false,
        user_msg_opt_in: false,
        client_type: "cli".to_string(),
        session_source: None,
        question_preview_format: None,
        session_ingress_token: None,
        oauth_token_from_fd: None,
        api_key_from_fd: None,
        flag_settings_path: None,
        flag_settings_inline: None,
        allowed_setting_sources: vec![
            "userSettings".to_string(),
            "projectSettings".to_string(),
            "localSettings".to_string(),
            "flagSettings".to_string(),
            "policySettings".to_string(),
        ],
        stats_store: None,
        session_id,
        parent_session_id: None,
        agent_color_map: HashMap::new(),
        agent_color_index: 0,
        last_api_request: None,
        last_api_request_messages: None,
        last_classifier_requests: None,
        cached_claude_md_content: None,
        in_memory_error_log: Vec::new(),
        inline_plugins: Vec::new(),
        chrome_flag_override: None,
        use_cowork_plugins: false,
        session_bypass_permissions_mode: false,
        scheduled_tasks_enabled: false,
        session_cron_tasks: Vec::new(),
        session_created_teams: HashSet::new(),
        session_trust_accepted: false,
        session_persistence_disabled: false,
        has_exited_plan_mode: false,
        needs_plan_mode_exit_attachment: false,
        needs_auto_mode_exit_attachment: false,
        lsp_recommendation_shown_this_session: false,
        init_json_schema: None,
        registered_hooks: None,
        plan_slug_cache: HashMap::new(),
        teleported_session_info: None,
        invoked_skills: HashMap::new(),
        slow_operations: Vec::new(),
        sdk_betas: None,
        main_thread_agent_type: None,
        is_remote_mode: false,
        direct_connect_server_url: None,
        system_prompt_section_cache: HashMap::new(),
        last_emitted_date: None,
        additional_directories_for_claude_md: Vec::new(),
        allowed_channels: Vec::new(),
        has_dev_channels: false,
        session_project_dir: None,
        prompt_cache_1h_allowlist: None,
        prompt_cache_1h_eligible: None,
        afk_mode_header_latched: None,
        fast_mode_header_latched: None,
        cache_editing_header_latched: None,
        thinking_clear_latched: None,
        prompt_id: None,
        last_main_request_id: None,
        last_api_completion_timestamp: None,
        pending_post_compaction: false,
        output_tokens_at_turn_start: 0,
        current_turn_token_budget: None,
        budget_continuation_count: 0,
    }
}

static STATE: Lazy<Mutex<State>> = Lazy::new(|| Mutex::new(get_initial_state()));
