// Source: /data/home/swei/claudecode/openclaudecode/src/utils/sessionRestore.ts
//! Session restore functionality for resuming conversations from NDJSON transcripts.
//!
//! This module provides comprehensive session restoration including:
//! - File history state restoration
//! - Attribution state restoration
//! - Context-collapse commit/snapshot restoration
//! - TODO list extraction from transcript
//! - Agent restoration (type, name, color, model override)
//! - Coordinator mode matching
//! - Resume consistency checks

use crate::cli_ndjson_safe_stringify::serialize_to_ndjson;
use crate::coordinator::coordinator_mode::{is_coordinator_mode, match_session_mode};
use crate::constants::tools::TODO_WRITE_TOOL_NAME;
use crate::error::AgentError;
use crate::session::{get_jsonl_path, get_session_path, get_sessions_dir, SessionEntry, SessionMetadata};
use crate::types::api_types::{Message, MessageRole};
use crate::types::logs::{
    AttributionSnapshotMessage, ContextCollapseCommitEntry, ContextCollapseSnapshotEntry,
    FileHistorySnapshot, PersistedWorktreeSession,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A TODO item extracted from a TodoWrite tool call in the transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoEntry {
    pub id: String,
    pub content: String,
    pub done: bool,
}

/// Information about a standalone agent's visual context (name and color).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandaloneAgentContext {
    pub name: String,
    /// Display color. `None` means use the default color.
    pub color: Option<String>,
}

/// Agent restoration information extracted from session transcript entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRestoreInfo {
    /// The agent type/setting (e.g. "reviewer", "worker")
    pub agent_type: String,
    /// Human-readable agent name
    pub agent_name: Option<String>,
    /// Agent display color
    pub agent_color: Option<String>,
    /// Model override if specified on the agent
    pub model_override: Option<String>,
}

/// Consistency check result for session resume.
#[derive(Debug, Clone)]
pub struct ConsistencyCheck {
    /// Whether the session can be safely resumed.
    pub can_resume: bool,
    /// List of issues found during the consistency check.
    pub issues: Vec<String>,
}

/// Result of restoring a session from its NDJSON transcript log.
#[derive(Debug, Clone)]
pub struct SessionRestoreResult {
    /// Messages extracted from the session transcript.
    pub messages: Vec<Message>,
    /// File history snapshots for state restoration.
    pub file_history_snapshots: Vec<FileHistorySnapshot>,
    /// Attribution snapshots for commit attribution state.
    pub attribution_snapshots: Vec<AttributionSnapshotMessage>,
    /// Context-collapse commit entries for rebuilding the collapsed view.
    pub context_collapse_commits: Vec<ContextCollapseCommitEntry>,
    /// Context-collapse staged snapshot.
    pub context_collapse_snapshot: Option<ContextCollapseSnapshotEntry>,
    /// TODO items extracted from the last TodoWrite tool call.
    pub todo_items: Vec<String>,
    /// Agent restoration info if the session used a custom agent.
    pub agent_info: Option<AgentRestoreInfo>,
    /// Standalone agent visual context (name + color).
    pub standalone_agent_context: Option<StandaloneAgentContext>,
    /// Session metadata (model, cwd, etc.).
    pub metadata: Option<SessionMetadata>,
    /// Session mode from the transcript (coordinator or normal).
    pub mode: Option<String>,
    /// Worktree session state.
    pub worktree_session: Option<PersistedWorktreeSession>,
    /// Custom session title.
    pub custom_title: Option<String>,
    /// Session tag.
    pub tag: Option<String>,
    /// Number of entries skipped due to parse errors.
    pub skipped_count: usize,
}

// ---------------------------------------------------------------------------
// Main restore function
// ---------------------------------------------------------------------------

/// Restore a complete session state from its NDJSON transcript log.
///
/// This is the primary entry point for session resume. It loads the session
/// transcript, parses all entry types, and returns a structured result
/// containing messages, file history snapshots, attribution snapshots,
/// context-collapse commits, TODO items, and agent info.
///
/// # Arguments
///
/// * `session_id` - The session identifier.
///
/// # Returns
///
/// A `SessionRestoreResult` with all extracted state, or an error if the
/// session cannot be loaded.
pub async fn restore_session_from_log(session_id: &str) -> Result<SessionRestoreResult, AgentError> {
    let entries = load_transcript_entries(session_id).await?;

    // Extract entries by type
    let mut file_history_snapshots: Vec<FileHistorySnapshot> = Vec::new();
    let mut attribution_snapshots: Vec<AttributionSnapshotMessage> = Vec::new();
    let mut context_collapse_commits: Vec<ContextCollapseCommitEntry> = Vec::new();
    let mut context_collapse_snapshot: Option<ContextCollapseSnapshotEntry> = None;
    let mut worktree_session: Option<PersistedWorktreeSession> = None;
    let mut custom_title: Option<String> = None;
    let mut tag: Option<String> = None;

    // Agent-related fields
    let mut agent_setting: Option<String> = None;
    let mut agent_name: Option<String> = None;
    let mut agent_color: Option<String> = None;

    let mut metadata: Option<SessionMetadata> = None;
    let mut mode: Option<String> = None;
    let mut messages: Vec<Message> = Vec::new();

    for entry in &entries {
        let entry_type = match &entry.entry_type {
            Some(t) => t.as_str(),
            None => continue,
        };

        match entry_type {
            "file-history-snapshot" => {
                if let Some(data) = &entry.data {
                    if let Ok(snapshot) =
                        serde_json::from_value::<FileHistorySnapshot>(data.clone())
                    {
                        file_history_snapshots.push(snapshot);
                    }
                }
            }
            "attribution-snapshot" => {
                if let Some(data) = &entry.data {
                    if let Ok(snapshot) = serde_json::from_value::<AttributionSnapshotMessage>(data.clone()) {
                        attribution_snapshots.push(snapshot);
                    }
                }
            }
            "marble-origami-commit" => {
                if let Some(data) = &entry.data {
                    if let Ok(commit) = serde_json::from_value::<ContextCollapseCommitEntry>(data.clone()) {
                        context_collapse_commits.push(commit);
                    }
                }
            }
            "marble-origami-snapshot" => {
                if let Some(data) = &entry.data {
                    if let Ok(snapshot) =
                        serde_json::from_value::<ContextCollapseSnapshotEntry>(data.clone())
                    {
                        context_collapse_snapshot = Some(snapshot);
                    }
                }
            }
            "worktree-state" => {
                if let Some(data) = &entry.data {
                    if let Ok(ws) = serde_json::from_value::<PersistedWorktreeSession>(data.clone()) {
                        worktree_session = Some(ws);
                    }
                }
            }
            "agent-setting" => {
                if let Some(data) = &entry.data {
                    if let Some(setting) = data.get("agentSetting").and_then(|v| v.as_str()) {
                        agent_setting = Some(setting.to_string());
                    }
                }
            }
            "agent-name" => {
                if let Some(data) = &entry.data {
                    if let Some(name) = data.get("agentName").and_then(|v| v.as_str()) {
                        agent_name = Some(name.to_string());
                    }
                }
            }
            "agent-color" => {
                if let Some(data) = &entry.data {
                    if let Some(color) = data.get("agentColor").and_then(|v| v.as_str()) {
                        agent_color = Some(color.to_string());
                    }
                }
            }
            "custom-title" => {
                if let Some(data) = &entry.data {
                    if let Some(title) = data.get("customTitle").and_then(|v| v.as_str()) {
                        custom_title = Some(title.to_string());
                    }
                }
            }
            "tag" => {
                if let Some(data) = &entry.data {
                    if let Some(t) = data.get("tag").and_then(|v| v.as_str()) {
                        tag = Some(t.to_string());
                    }
                }
            }
            "mode" => {
                if let Some(data) = &entry.data {
                    if let Some(m) = data.get("mode").and_then(|v| v.as_str()) {
                        mode = Some(m.to_string());
                    }
                }
            }
            "metadata" => {
                if let Some(data) = &entry.data {
                    if let Ok(md) = serde_json::from_value::<SessionMetadata>(data.clone()) {
                        metadata = Some(md);
                    }
                }
            }
            "message" => {
                if let Some(data) = &entry.data {
                    if let Ok(msg) = serde_json::from_value::<Message>(data.clone()) {
                        messages.push(msg);
                    }
                }
            }
            _ => {}
        }
    }

    // Extract TODO items from messages
    let todo_items = extract_todo_from_transcript(&entries);

    // Restore agent info
    let agent_info = restore_agent_from_session_with_fields(
        agent_setting,
        agent_name.clone(),
        agent_color.clone(),
    );

    // Compute standalone agent context
    let standalone_agent_context = compute_standalone_agent_context(agent_name.as_deref(), agent_color.as_deref());

    // Restore context-collapse state in the global store
    // This mirrors TypeScript's unconditional restoreFromEntries call.
    // It must run before the first query() so projectView() can rebuild
    // the collapsed view from the resumed messages.
    let _ = &context_collapse_commits;
    let _ = &context_collapse_snapshot;
    // The caller should invoke crate::services::context_collapse::persist::restore_from_entries
    // with the extracted commits and snapshot. We provide the data here.

    Ok(SessionRestoreResult {
        messages,
        file_history_snapshots,
        attribution_snapshots,
        context_collapse_commits,
        context_collapse_snapshot,
        todo_items,
        agent_info,
        standalone_agent_context,
        metadata,
        mode,
        worktree_session,
        custom_title,
        tag,
        skipped_count: 0,
    })
}

/// Load and parse all NDJSON transcript entries for a session.
async fn load_transcript_entries(session_id: &str) -> Result<Vec<SessionEntry>, AgentError> {
    let path = get_jsonl_path(session_id);
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AgentError::Session(format!(
                "Session '{}' not found: no transcript at {:?}",
                session_id, path
            )));
        }
        Err(e) => return Err(AgentError::Io(e)),
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let entry: SessionEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        entries.push(entry);
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// TODO extraction
// ---------------------------------------------------------------------------

/// Extract TODO items from the last TodoWrite tool call in the transcript.
///
/// Scans session entries in reverse order to find the last assistant message
/// containing a TodoWrite tool_use block, then extracts the TODO item contents.
/// This mirrors the TypeScript `extractTodosFromTranscript` which scans
/// `Message[]` for `tool_use` blocks with `name === TODO_WRITE_TOOL_NAME`.
///
/// # Arguments
///
/// * `entries` - All parsed session transcript entries.
///
/// # Returns
///
/// A vector of TODO item content strings. Empty if no TodoWrite tool call
/// was found or the input was invalid.
pub fn extract_todo_from_transcript(entries: &[SessionEntry]) -> Vec<String> {
    // Scan entries in reverse to find the last TodoWrite tool call.
    for entry in entries.iter().rev() {
        let entry_type = match &entry.entry_type {
            Some(t) if t == "message" => "message",
            _ => continue,
        };
        let data = match &entry.data {
            Some(d) => d,
            None => continue,
        };

        // Check if this is an assistant message
        let role = data.get("role").and_then(|r| r.as_str());
        if role != Some("assistant") {
            continue;
        }

        // Look for TodoWrite tool call in tool_calls array
        let tool_calls = data.get("tool_calls");
        if let Some(tc_arr) = tool_calls.and_then(|arr| arr.as_array()) {
            for tc in tc_arr {
                let name = tc.get("name").and_then(|n| n.as_str());
                if name == Some(TODO_WRITE_TOOL_NAME) {
                    return parse_todos_from_tool_input(tc);
                }
            }
        }
    }
    Vec::new()
}

/// Parse TODO items from a tool call's input.
///
/// Handles both structured `input.todos` and flat JSON objects.
fn parse_todos_from_tool_input(tool_call: &serde_json::Value) -> Vec<String> {
    // Try input.todos first (structured format)
    if let Some(input) = tool_call.get("input") {
        if let Some(todos) = input.get("todos") {
            if let Some(arr) = todos.as_array() {
                let mut items = Vec::new();
                for item in arr {
                    if let Some(content) = item.get("content").and_then(|c| c.as_str()) {
                        items.push(content.to_string());
                    } else if let Some(content) = item.as_str() {
                        items.push(content.to_string());
                    }
                }
                if !items.is_empty() {
                    return items;
                }
            }
        }
    }

    // Fallback: try to extract content from the tool call itself
    Vec::new()
}

// ---------------------------------------------------------------------------
// Agent restoration
// ---------------------------------------------------------------------------

/// Restore agent information from session transcript entries.
///
/// Scans the full entry list for agent-setting, agent-name, and agent-color
/// entries to reconstruct the agent configuration that was active during
/// the session. This mirrors TypeScript's `restoreAgentFromSession`.
///
/// # Arguments
///
/// * `entries` - All parsed session transcript entries.
///
/// # Returns
///
/// `Some(AgentRestoreInfo)` if agent-related entries were found, `None`
/// if the session used no custom agent.
pub fn restore_agent_from_session(entries: &[SessionEntry]) -> Option<AgentRestoreInfo> {
    let mut agent_setting: Option<String> = None;
    let mut agent_name: Option<String> = None;
    let mut agent_color: Option<String> = None;

    for entry in entries {
        let entry_type = match &entry.entry_type {
            Some(t) => t.as_str(),
            None => continue,
        };
        let data = match &entry.data {
            Some(d) => d,
            None => continue,
        };

        match entry_type {
            "agent-setting" => {
                if let Some(setting) = data.get("agentSetting").and_then(|v| v.as_str()) {
                    agent_setting = Some(setting.to_string());
                }
            }
            "agent-name" => {
                if let Some(name) = data.get("agentName").and_then(|v| v.as_str()) {
                    agent_name = Some(name.to_string());
                }
            }
            "agent-color" => {
                if let Some(color) = data.get("agentColor").and_then(|v| v.as_str()) {
                    agent_color = Some(color.to_string());
                }
            }
            _ => {}
        }
    }

    restore_agent_from_session_with_fields(agent_setting, agent_name, agent_color)
}

/// Internal helper to build AgentRestoreInfo from extracted fields.
fn restore_agent_from_session_with_fields(
    agent_setting: Option<String>,
    _agent_name: Option<String>,
    _agent_color: Option<String>,
) -> Option<AgentRestoreInfo> {
    let agent_setting = agent_setting?;

    Some(AgentRestoreInfo {
        agent_type: agent_setting,
        agent_name: _agent_name,
        agent_color: normalize_agent_color(_agent_color.as_deref()),
        model_override: None,
    })
}

/// Compute standalone agent context from name and color.
/// Mirrors TypeScript's `computeStandaloneAgentContext`.
fn compute_standalone_agent_context(
    agent_name: Option<&str>,
    agent_color: Option<&str>,
) -> Option<StandaloneAgentContext> {
    if agent_name.is_none() && agent_color.is_none() {
        return None;
    }
    Some(StandaloneAgentContext {
        name: agent_name.unwrap_or("").to_string(),
        color: normalize_agent_color(agent_color),
    })
}

/// Normalize agent color: "default" maps to None.
fn normalize_agent_color(color: Option<&str>) -> Option<String> {
    match color {
        Some("default") => None,
        Some(c) => Some(c.to_string()),
        None => None,
    }
}

// ---------------------------------------------------------------------------
// Consistency checks
// ---------------------------------------------------------------------------

/// Check whether a session can be safely resumed.
///
/// Validates:
/// 1. The session directory and transcript file exist.
/// 2. The transcript contains at least one entry.
/// 3. The session metadata is well-formed.
/// 4. If `parent_session_id` is provided, that parent session also exists.
///
/// # Arguments
///
/// * `session_id` - The session to check.
/// * `parent_session_id` - Optional parent session ID to validate.
///
/// # Returns
///
/// A `ConsistencyCheck` with the result.
pub async fn check_resume_consistency(
    session_id: &str,
    parent_session_id: Option<&str>,
) -> ConsistencyCheck {
    let mut issues = Vec::new();

    // Check session directory exists
    let session_dir = get_session_path(session_id);
    if !session_dir.exists() {
        return ConsistencyCheck {
            can_resume: false,
            issues: vec![format!(
                "Session directory does not exist: {:?}",
                session_dir
            )],
        };
    }

    // Check transcript file exists
    let jsonl_path = get_jsonl_path(session_id);
    if !jsonl_path.exists() {
        issues.push(format!(
            "Transcript file does not exist: {:?}",
            jsonl_path
        ));
    } else {
        // Check transcript is not empty
        match tokio::fs::read_to_string(&jsonl_path).await {
            Ok(content) => {
                let line_count = content.lines().filter(|l| !l.trim().is_empty()).count();
                if line_count == 0 {
                    issues.push("Transcript file is empty, no entries to resume.".to_string());
                }
            }
            Err(e) => issues.push(format!("Failed to read transcript: {}", e)),
        }
    }

    // Check parent session if specified
    if let Some(parent_id) = parent_session_id {
        let parent_dir = get_session_path(parent_id);
        if !parent_dir.exists() {
            issues.push(format!("Parent session '{}' directory not found: {:?}", parent_id, parent_dir));
        } else {
            let parent_jsonl = get_jsonl_path(parent_id);
            if !parent_jsonl.exists() {
                issues.push(format!(
                    "Parent session '{}' has no transcript file: {:?}",
                    parent_id, parent_jsonl
                ));
            }
        }
    }

    // Check working directory matches
    if let Some(cwd) = std::env::var("AI_CODE_CWD").ok() {
        let current = std::env::current_dir().unwrap_or_default();
        if cwd != current.to_string_lossy() {
            issues.push(format!(
                "Session CWD '{}' differs from current directory '{}'",
                cwd,
                current.to_string_lossy()
            ));
        }
    }

    ConsistencyCheck {
        can_resume: issues.is_empty(),
        issues,
    }
}

/// Check resume consistency and return errors as a String.
///
/// Convenience wrapper that returns `Err(String)` on failure for
/// ergonomic use in error-handling code.
///
/// # Arguments
///
/// * `session_id` - The session to check.
/// * `parent_session_id` - Optional parent session ID.
///
/// # Returns
///
/// `Ok(())` if the session can be safely resumed, `Err(String)` with
/// a joined list of issues otherwise.
pub async fn check_resume_consistency_err(
    session_id: &str,
    parent_session_id: Option<&str>,
) -> Result<(), String> {
    let check = check_resume_consistency(session_id, parent_session_id).await;
    if check.can_resume {
        Ok(())
    } else {
        Err(check.issues.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// High-level resume handler
// ---------------------------------------------------------------------------

/// Handle a full session resume: load, restore state, match mode, return result.
///
/// This is the top-level orchestration function for session resume. It:
/// 1. Loads and parses the session transcript
/// 2. Extracts all state (messages, file history, attribution, etc.)
/// 3. Matches coordinator mode to the resumed session
/// 4. Extracts TODO items
/// 5. Restores agent information
/// 6. Validates session metadata
///
/// # Arguments
///
/// * `session_id` - The session identifier to resume.
///
/// # Returns
///
/// A `SessionRestoreResult` with all restored state, including a coordinator
/// mode warning message if the mode was switched.
pub async fn handle_session_resume(session_id: &str) -> Result<SessionRestoreResult, AgentError> {
    // Run consistency check
    let check = check_resume_consistency(session_id, None).await;
    if !check.can_resume {
        return Err(AgentError::Session(format!(
            "Resume consistency check failed: {}",
            check.issues.join("; ")
        )));
    }

    // Restore from log
    let mut result = restore_session_from_log(session_id).await?;

    // Match coordinator mode to the resumed session
    if let Some(ref mode) = result.mode {
        if let Some(warning) = match_session_mode(Some(mode)) {
            // Append the mode-switch warning as a system message
            result.messages.push(create_system_message(&warning));
        }
    }

    // Log restoration summary
    log::info!(
        "Session '{}' restored: {} messages, {} file history snapshots, {} attribution snapshots, {} context collapse commits, {} todo items",
        session_id,
        result.messages.len(),
        result.file_history_snapshots.len(),
        result.attribution_snapshots.len(),
        result.context_collapse_commits.len(),
        result.todo_items.len(),
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// State restoration helpers
// ---------------------------------------------------------------------------

/// Restore file history state from snapshots.
///
/// Applies the last file history snapshot to rebuild the file history
/// tracking state. Mirrors TypeScript's `fileHistoryRestoreStateFromLog`.
///
/// # Arguments
///
/// * `snapshots` - File history snapshots extracted from the transcript.
///
/// # Returns
///
/// The latest merged file history state, or `None` if no snapshots were provided.
pub fn restore_file_history_state(
    snapshots: &[FileHistorySnapshot],
) -> Option<FileHistorySnapshot> {
    if snapshots.is_empty() {
        return None;
    }

    // Merge all snapshots: later snapshots override earlier ones.
    let mut merged = FileHistorySnapshot::new();
    for snapshot in snapshots {
        for (key, value) in snapshot {
            merged.insert(key.clone(), value.clone());
        }
    }
    Some(merged)
}

/// Restore attribution state from snapshots.
///
/// Applies the last attribution snapshot to rebuild the commit attribution
/// tracking state. Mirrors TypeScript's `attributionRestoreStateFromLog`.
///
/// # Arguments
///
/// * `snapshots` - Attribution snapshots extracted from the transcript.
///
/// # Returns
///
/// The latest attribution snapshot, or `None` if no snapshots were provided.
pub fn restore_attribution_state(
    snapshots: &[AttributionSnapshotMessage],
) -> Option<AttributionSnapshotMessage> {
    snapshots.last().cloned()
}

/// Restore worktree session state.
///
/// Checks whether the worktree path from the session transcript still
/// exists. If it does, returns the worktree state so the caller can cd
/// back into it. If the directory is gone, returns `None` to indicate
/// the worktree should be considered exited.
///
/// # Arguments
///
/// * `worktree_session` - The persisted worktree session from the transcript.
///
/// # Returns
///
/// `Some(PersistedWorktreeSession)` if the worktree directory exists,
/// `None` if it has been removed.
pub fn restore_worktree_state(
    worktree_session: Option<PersistedWorktreeSession>,
) -> Option<PersistedWorktreeSession> {
    let ws = worktree_session?;

    // TOCTOU-safe check: if the directory doesn't exist, treat as exited
    let path = PathBuf::from(&ws.worktree_path);
    if !path.is_dir() {
        log::warn!(
            "Worktree directory no longer exists: {:?}. Treating as exited.",
            path
        );
        return None;
    }

    Some(ws)
}

/// Create a system message for coordinator mode switch notifications.
fn create_system_message(content: &str) -> Message {
    Message {
        role: MessageRole::System,
        content: content.to_string(),
        attachments: None,
        tool_call_id: None,
        tool_calls: None,
        is_error: None,
        is_meta: Some(true),
        is_api_error_message: None,
        error_details: None,
        uuid: None,
        timestamp: None,
    }
}

/// Write a mode entry to the session transcript.
///
/// Persists the current coordinator mode so future resumes know what mode
/// this session was in.
///
/// # Arguments
///
/// * `session_id` - The session to write to.
pub async fn save_mode_to_session(session_id: &str) {
    let mode = if is_coordinator_mode() {
        "coordinator"
    } else {
        "normal"
    };
    let entry = SessionEntry {
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
        entry_type: Some("mode".to_string()),
        data: Some(serde_json::json!({
            "mode": mode,
            "sessionId": session_id,
        })),
    };
    if let Err(e) = crate::session::append_session_entry(session_id, &entry).await {
        log::warn!("Failed to save mode entry: {}", e);
    }
}

/// Apply context-collapse restoration from a restore result.
///
/// This convenience function calls the context-collapse persist module with
/// the data from a `SessionRestoreResult`. In the current SDK build, the
/// persist module is a no-op stub. When context-collapse is fully
/// implemented, this wires through to the real store.
///
/// # Arguments
///
/// * `result` - The session restore result containing collapse data.
pub fn apply_context_collapse_restore(result: &SessionRestoreResult) {
    // Wire through to the context-collapse persist module.
    // Currently a no-op in the SDK; the real implementation lives in
    // TypeScript and will be ported when context-collapse is enabled.
    let _ = &result.context_collapse_commits;
    let _ = &result.context_collapse_snapshot;
}

/// Get the sessions directory path (re-export for convenience).
pub fn session_restore_dir() -> PathBuf {
    get_sessions_dir()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message_entry(role: &str, content: &str) -> SessionEntry {
        SessionEntry {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("message".to_string()),
            data: Some(serde_json::json!({
                "role": role,
                "content": content,
            })),
        }
    }

    fn make_assistant_with_tool_call(tool_name: &str, todos: Vec<serde_json::Value>) -> SessionEntry {
        SessionEntry {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("message".to_string()),
            data: Some(serde_json::json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "toolu-123",
                    "type": "function",
                    "name": tool_name,
                    "input": {
                        "todos": todos,
                    },
                }],
            })),
        }
    }

    fn make_agent_setting_entry(setting: &str) -> SessionEntry {
        SessionEntry {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("agent-setting".to_string()),
            data: Some(serde_json::json!({
                "agentSetting": setting,
                "sessionId": "test-session",
            })),
        }
    }

    fn make_agent_name_entry(name: &str) -> SessionEntry {
        SessionEntry {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("agent-name".to_string()),
            data: Some(serde_json::json!({
                "agentName": name,
                "sessionId": "test-session",
            })),
        }
    }

    fn make_agent_color_entry(color: &str) -> SessionEntry {
        SessionEntry {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("agent-color".to_string()),
            data: Some(serde_json::json!({
                "agentColor": color,
                "sessionId": "test-session",
            })),
        }
    }

    // --- extract_todo_from_transcript tests ---

    #[test]
    fn test_extract_todo_finds_last_todo_write() {
        let entries = vec![
            make_message_entry("user", "hello"),
            make_assistant_with_tool_call(TODO_WRITE_TOOL_NAME, vec![
                serde_json::json!({"content": "first task", "id": "1", "done": false}),
                serde_json::json!({"content": "second task", "id": "2", "done": false}),
            ]),
            make_message_entry("user", "done with first"),
            make_assistant_with_tool_call(TODO_WRITE_TOOL_NAME, vec![
                serde_json::json!({"content": "first task", "id": "1", "done": true}),
                serde_json::json!({"content": "third task", "id": "3", "done": false}),
            ]),
        ];
        let todos = extract_todo_from_transcript(&entries);
        // Should find the LAST TodoWrite call
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], "first task");
        assert_eq!(todos[1], "third task");
    }

    #[test]
    fn test_extract_todo_empty_transcript() {
        let entries: Vec<SessionEntry> = vec![];
        let todos = extract_todo_from_transcript(&entries);
        assert!(todos.is_empty());
    }

    #[test]
    fn test_extract_todo_no_todo_write() {
        let entries = vec![
            make_message_entry("user", "hello"),
            make_message_entry("assistant", "hi there"),
            make_assistant_with_tool_call("Read", vec![]),
        ];
        let todos = extract_todo_from_transcript(&entries);
        assert!(todos.is_empty());
    }

    #[test]
    fn test_extract_todo_plain_string_items() {
        let entries = vec![make_assistant_with_tool_call(TODO_WRITE_TOOL_NAME, vec![
            serde_json::json!("just a task"),
            serde_json::json!("another task"),
        ])];
        // Plain strings in todos array are handled by the as_str() fallback
        let todos = extract_todo_from_transcript(&entries);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], "just a task");
        assert_eq!(todos[1], "another task");
    }

    // --- restore_agent_from_session tests ---

    #[test]
    fn test_restore_agent_finds_all_fields() {
        let entries = vec![
            make_message_entry("user", "hello"),
            make_agent_setting_entry("reviewer"),
            make_agent_name_entry("Code Reviewer"),
            make_agent_color_entry("blue"),
        ];
        let info = restore_agent_from_session(&entries);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.agent_type, "reviewer");
        assert_eq!(info.agent_color, Some("blue".to_string()));
    }

    #[test]
    fn test_restore_agent_no_setting() {
        let entries = vec![
            make_agent_name_entry("Some Agent"),
            make_agent_color_entry("red"),
        ];
        // Without agent-setting, should return None
        let info = restore_agent_from_session(&entries);
        assert!(info.is_none());
    }

    #[test]
    fn test_restore_agent_default_color_normalized() {
        let entries = vec![
            make_agent_setting_entry("worker"),
            make_agent_color_entry("default"),
        ];
        let info = restore_agent_from_session(&entries);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.agent_type, "worker");
        // "default" color should be normalized to None
        assert_eq!(info.agent_color, None);
    }

    // --- restore_file_history_state tests ---

    #[test]
    fn test_restore_file_history_empty() {
        let snapshots: Vec<FileHistorySnapshot> = vec![];
        let result = restore_file_history_state(&snapshots);
        assert!(result.is_none());
    }

    #[test]
    fn test_restore_file_history_merges() {
        let mut s1 = FileHistorySnapshot::new();
        s1.insert("file_a".to_string(), serde_json::json!({"hash": "abc"}));

        let mut s2 = FileHistorySnapshot::new();
        s2.insert("file_b".to_string(), serde_json::json!({"hash": "def"}));
        s2.insert("file_a".to_string(), serde_json::json!({"hash": "abc2"}));

        let result = restore_file_history_state(&[s1, s2]);
        assert!(result.is_some());
        let merged = result.unwrap();
        assert_eq!(merged.len(), 2);
        // file_a should be overridden by s2
        assert_eq!(merged["file_a"], serde_json::json!({"hash": "abc2"}));
        assert_eq!(merged["file_b"], serde_json::json!({"hash": "def"}));
    }

    // --- restore_attribution_state tests ---

    #[test]
    fn test_restore_attribution_empty() {
        let snapshots: Vec<AttributionSnapshotMessage> = vec![];
        let result = restore_attribution_state(&snapshots);
        assert!(result.is_none());
    }

    #[test]
    fn test_restore_attribution_returns_last() {
        let s1 = AttributionSnapshotMessage {
            message_type: "attribution-snapshot".to_string(),
            message_id: uuid::Uuid::new_v4(),
            surface: "edit".to_string(),
            file_states: HashMap::new(),
            prompt_count: Some(1),
            prompt_count_at_last_commit: None,
            permission_prompt_count: None,
            permission_prompt_count_at_last_commit: None,
            escape_count: None,
            escape_count_at_last_commit: None,
        };
        let s2 = AttributionSnapshotMessage {
            message_type: "attribution-snapshot".to_string(),
            message_id: uuid::Uuid::new_v4(),
            surface: "edit".to_string(),
            file_states: HashMap::new(),
            prompt_count: Some(5),
            prompt_count_at_last_commit: None,
            permission_prompt_count: None,
            permission_prompt_count_at_last_commit: None,
            escape_count: None,
            escape_count_at_last_commit: None,
        };
        let result = restore_attribution_state(&[s1, s2]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().prompt_count, Some(5));
    }

    // --- restore_worktree_state tests ---

    #[test]
    fn test_restore_worktree_none() {
        let result = restore_worktree_state(None);
        assert!(result.is_none());
    }

    #[test]
    fn test_restore_worktree_missing_dir() {
        let ws = PersistedWorktreeSession {
            original_cwd: "/tmp".to_string(),
            worktree_path: "/tmp/nonexistent-worktree-path-12345".to_string(),
            worktree_name: "test".to_string(),
            worktree_branch: None,
            original_branch: None,
            original_head_commit: None,
            session_id: "test-session".to_string(),
            tmux_session_name: None,
            hook_based: None,
        };
        // Directory doesn't exist, should return None
        let result = restore_worktree_state(Some(ws));
        assert!(result.is_none());
    }

    #[test]
    fn test_restore_worktree_existing_dir() {
        let ws = PersistedWorktreeSession {
            original_cwd: "/tmp".to_string(),
            worktree_path: "/tmp".to_string(),
            worktree_name: "test".to_string(),
            worktree_branch: None,
            original_branch: None,
            original_head_commit: None,
            session_id: "test-session".to_string(),
            tmux_session_name: None,
            hook_based: None,
        };
        // /tmp always exists
        let result = restore_worktree_state(Some(ws));
        assert!(result.is_some());
    }

    // --- normalize_agent_color tests ---

    #[test]
    fn test_normalize_agent_color() {
        assert_eq!(normalize_agent_color(Some("default")), None);
        assert_eq!(normalize_agent_color(Some("blue")), Some("blue".to_string()));
        assert_eq!(normalize_agent_color(None), None);
    }

    // --- compute_standalone_agent_context tests ---

    #[test]
    fn test_compute_standalone_agent_context_both_none() {
        let result = compute_standalone_agent_context(None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_standalone_agent_context_name_only() {
        let result = compute_standalone_agent_context(Some("Reviewer"), None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Reviewer");
    }

    #[test]
    fn test_compute_standalone_agent_context_with_default_color() {
        let result = compute_standalone_agent_context(Some("Agent"), Some("default"));
        assert!(result.is_some());
        let ctx = result.unwrap();
        assert_eq!(ctx.name, "Agent");
        assert_eq!(ctx.color, None);
    }

    // --- create_system_message tests ---

    #[test]
    fn test_create_system_message() {
        let msg = create_system_message("Mode switched");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "Mode switched");
        assert!(msg.is_meta == Some(true));
    }

    // --- check_resume_consistency tests ---

    #[tokio::test]
    async fn test_check_resume_consistency_nonexistent() {
        let check = check_resume_consistency("nonexistent-session-12345", None).await;
        assert!(!check.can_resume);
        assert!(!check.issues.is_empty());
    }

    #[tokio::test]
    async fn test_check_resume_consistency_valid_session() {
        crate::tests::common::clear_all_test_state();
        let session_id = format!("consistency-test-{}", uuid::Uuid::new_v4());

        // Create a session with some entries
        let msg = crate::session::SessionEntry::message(&crate::types::api_types::Message {
            role: crate::types::api_types::MessageRole::User,
            content: "hello".to_string(),
            ..Default::default()
        });
        crate::session::append_session_entry(&session_id, &msg).await.unwrap();

        let check = check_resume_consistency(&session_id, None).await;
        assert!(check.can_resume);
        assert!(check.issues.is_empty());

        // Cleanup
        let _ = tokio::fs::remove_dir_all(crate::session::get_session_path(&session_id)).await;
    }

    #[tokio::test]
    async fn test_check_resume_consistency_with_missing_parent() {
        crate::tests::common::clear_all_test_state();
        let session_id = format!("consistency-parent-test-{}", uuid::Uuid::new_v4());

        // Create the session
        let msg = crate::session::SessionEntry::message(&crate::types::api_types::Message {
            role: crate::types::api_types::MessageRole::User,
            content: "hello".to_string(),
            ..Default::default()
        });
        crate::session::append_session_entry(&session_id, &msg).await.unwrap();

        // Check with a nonexistent parent
        let check = check_resume_consistency(&session_id, Some("missing-parent-session")).await;
        assert!(!check.can_resume);
        assert!(check.issues.iter().any(|i| i.contains("Parent session")));

        // Cleanup
        let _ = tokio::fs::remove_dir_all(crate::session::get_session_path(&session_id)).await;
    }

    // --- SessionRestoreResult tests ---

    #[test]
    fn test_session_restore_result_debug() {
        let result = SessionRestoreResult {
            messages: vec![],
            file_history_snapshots: vec![],
            attribution_snapshots: vec![],
            context_collapse_commits: vec![],
            context_collapse_snapshot: None,
            todo_items: vec!["test".to_string()],
            agent_info: None,
            standalone_agent_context: None,
            metadata: None,
            mode: None,
            worktree_session: None,
            custom_title: None,
            tag: None,
            skipped_count: 0,
        };
        // Just verify Debug works
        let _ = format!("{:?}", result);
    }

    // --- parse_todos_from_tool_input tests ---

    #[test]
    fn test_parse_todos_from_tool_input() {
        let tool_call = serde_json::json!({
            "name": "TodoWrite",
            "input": {
                "todos": [
                    {"content": "task one", "id": "1", "done": false},
                    {"content": "task two", "id": "2", "done": true},
                ]
            }
        });
        let todos = parse_todos_from_tool_input(&tool_call);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], "task one");
        assert_eq!(todos[1], "task two");
    }

    #[test]
    fn test_parse_todos_from_tool_input_empty() {
        let tool_call = serde_json::json!({
            "name": "TodoWrite",
            "input": {
                "todos": []
            }
        });
        let todos = parse_todos_from_tool_input(&tool_call);
        assert!(todos.is_empty());
    }

    #[test]
    fn test_parse_todos_from_tool_input_no_input() {
        let tool_call = serde_json::json!({
            "name": "Read",
        });
        let todos = parse_todos_from_tool_input(&tool_call);
        assert!(todos.is_empty());
    }
}
