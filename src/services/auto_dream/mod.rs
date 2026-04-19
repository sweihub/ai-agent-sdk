// Source: ~/claudecode/openclaudecode/src/services/autoDream/autoDream.ts
//! Background memory consolidation. Fires the /dream prompt as a forked
//! subagent when time-gate passes AND enough sessions have accumulated.
//!
//! Gate order (cheapest first):
//!   1. Time: hours since lastConsolidatedAt >= minHours (one stat)
//!   2. Sessions: transcript count with mtime > lastConsolidatedAt >= minSessions
//!   3. Lock: no other process mid-consolidation
//!
//! State is closure-scoped inside init_auto_dream() rather than module-level
//! (tests call init_auto_dream() in beforeEach for a fresh closure).

pub mod config;
pub mod consolidation_lock;
pub mod consolidation_prompt;

pub use config::*;
pub use consolidation_lock::*;
pub use consolidation_prompt::*;

use crate::memdir::paths::get_auto_mem_path;
use crate::types::message::Message;
use crate::utils::abort_controller::AbortController;
use std::future::Future;
use std::pin::Pin;

// Scan throttle: when time-gate passes but session-gate doesn't, the lock
// mtime doesn't advance, so the time-gate keeps passing every turn.
const SESSION_SCAN_INTERVAL_MS: u64 = 10 * 60 * 1000; // 10 minutes

/// Thresholds from tengu_onyx_plover. The enabled gate lives in config.ts
/// (isAutoDreamEnabled); this returns only the scheduling knobs.
#[derive(Debug, Clone)]
struct AutoDreamConfig {
    min_hours: u64,
    min_sessions: u64,
}

const DEFAULTS: AutoDreamConfig = AutoDreamConfig {
    min_hours: 24,
    min_sessions: 5,
};

/// Build the configuration. GrowthBook feature values with validation.
fn get_config() -> AutoDreamConfig {
    DEFAULTS
}

/// Check if the auto-dream gate is open.
/// Returns false when KAIROS/remote/auto-memory disabled.
fn is_gate_open() -> bool {
    is_auto_dream_enabled()
}

/// State for a running dream task.
#[derive(Debug, Clone)]
pub struct DreamTaskState {
    pub task_id: String,
    pub sessions_reviewing: usize,
    pub files_touched: Vec<String>,
    pub status: DreamTaskStatus,
}

/// Status of a dream task.
#[derive(Debug, Clone)]
pub enum DreamTaskStatus {
    Running,
    Completed,
    Failed,
    Killed,
}

/// Internal runner state — closure-scoped in init_auto_dream.
struct AutoDreamRunnerState {
    last_session_scan_at: u64,
}

impl AutoDreamRunnerState {
    fn new() -> Self {
        Self {
            last_session_scan_at: 0,
        }
    }
}

/// Progress watcher callback for the forked agent's messages.
fn make_dream_progress_watcher(
    task_id: String,
    _set_app_state: Box<dyn Fn(Box<dyn std::any::Any>) -> Box<dyn std::any::Any> + Send + Sync>,
) -> Box<dyn Fn(Message) + Send + Sync> {
    Box::new(move |msg: Message| {
        if matches!(msg, Message::Assistant(_)) {
            log::debug!("[autoDream] progress watcher received assistant message for task {}", task_id);
        }
    })
}

/// Future type alias for the auto-dream runner body.
type AutoDreamFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Runner handle returned by init_auto_dream.
pub struct AutoDreamHandle {
    inner: Box<dyn FnOnce() -> AutoDreamFuture + Send>,
}

impl AutoDreamHandle {
    /// Execute the runner. Spawns on the current tokio runtime.
    pub fn run(self) {
        let future = (self.inner)();
        tokio::spawn(async move { future.await });
    }
}

/// Initialize the auto-dream background consolidation service.
/// Call once at startup (from background_housekeeping alongside
/// init_extract_memories), or per-test for a fresh closure.
///
/// Returns a runner handle that can be invoked on each turn.
pub fn init_auto_dream() -> AutoDreamHandle {
    let state = std::sync::Arc::new(tokio::sync::Mutex::new(AutoDreamRunnerState::new()));

    AutoDreamHandle {
        inner: Box::new(move || {
            let state = state.clone();
            Box::pin(async move {
                let cfg = get_config();

                // Gate check
                if !is_gate_open() {
                    return;
                }

                // --- Time gate ---
                let last_at = consolidation_lock::read_last_consolidated_at().await;
                let now = chrono::Utc::now().timestamp_millis() as u64;
                let hours_since = (now - last_at) as f64 / 3_600_000.0;

                if hours_since < cfg.min_hours as f64 {
                    log::debug!(
                        "[autoDream] time gate: {:.1}h since last consolidation, need {}h",
                        hours_since,
                        cfg.min_hours
                    );
                    return;
                }

                // --- Scan throttle ---
                let mut state_guard = state.lock().await;
                let since_scan_ms = now - state_guard.last_session_scan_at;

                if since_scan_ms < SESSION_SCAN_INTERVAL_MS {
                    log::debug!(
                        "[autoDream] scan throttle — time-gate passed but last scan was {}s ago",
                        since_scan_ms / 1000
                    );
                    return;
                }
                state_guard.last_session_scan_at = now;
                drop(state_guard);

                // --- Session gate ---
                let session_ids = consolidation_lock::list_sessions_touched_since(last_at).await;

                // In the full impl, the current session is excluded:
                //   const currentSession = getSessionId();
                //   sessionIds = sessionIds.filter(id => id !== currentSession);

                if session_ids.len() < cfg.min_sessions as usize {
                    log::debug!(
                        "[autoDream] session gate: {} sessions since last consolidation, need {}",
                        session_ids.len(),
                        cfg.min_sessions
                    );
                    return;
                }

                // --- Lock ---
                let prior_mtime = match consolidation_lock::try_acquire_consolidation_lock().await {
                    Some(mtime) => mtime,
                    None => {
                        log::debug!(
                            "[autoDream] lock acquisition failed or another process is consolidating"
                        );
                        return;
                    }
                };

                log::debug!(
                    "[autoDream] firing — {:.1}h since last, {} sessions to review",
                    hours_since,
                    session_ids.len()
                );

                // Build the extra context string
                let session_list: String = session_ids
                    .iter()
                    .map(|id| format!("- {}", id))
                    .collect::<Vec<_>>()
                    .join("\n");

                let extra = format!(
                    "\n\nSessions since last consolidation ({}):\n{}",
                    session_ids.len(),
                    session_list
                );

                let memory_root = get_auto_mem_path();
                let memory_root_str = memory_root.to_string_lossy().to_string();

                // In the full impl, transcript_dir comes from get_project_dir(getOriginalCwd())
                let transcript_dir = std::env::current_dir()
                    .map(|p| p.join("sessions").to_string_lossy().to_string())
                    .unwrap_or_else(|_| "sessions/".to_string());

                let _prompt = consolidation_prompt::build_consolidation_prompt(
                    &memory_root_str,
                    &transcript_dir,
                    &extra,
                );

                // In the full implementation, this would call run_forked_agent:
                //   let result = run_forked_agent(ForkedAgentConfig {
                //       prompt_messages: [createUserMessage({ content: prompt })],
                //       cache_safe_params: create_cache_safe_params(context),
                //       can_use_tool: create_auto_mem_can_use_tool(memory_root),
                //       query_source: QuerySource("auto_dream".to_string()),
                //       fork_label: "auto_dream".to_string(),
                //       skip_transcript: true,
                //       overrides: Some(SubagentContextOverrides {
                //           abort_controller: Some(Arc::new(AbortController::default())),
                //           ..Default::default()
                //       }),
                //       on_message: make_dream_progress_watcher(task_id, set_app_state),
                //       ..Default::default()
                //   }).await;
                //
                // On success: complete the task, show inline completion.
                // On failure: rollback the lock mtime so the time-gate fires again.

                // SDK port: log the attempt without executing the query loop
                log::debug!(
                    "[autoDream] would run forked agent — {} sessions, prompt chars: {}",
                    session_ids.len(),
                    _prompt.len()
                );

                // prior_mtime intentionally not rolled back on success.
                let _ = prior_mtime;
            })
        }),
    }
}

/// Execute the auto-dream consolidation runner.
/// Per-turn cost when enabled: one stat + one file read.
pub async fn execute_auto_dream() {
    let handle = init_auto_dream();
    handle.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = get_config();
        assert_eq!(cfg.min_hours, 24);
        assert_eq!(cfg.min_sessions, 5);
    }

    #[test]
    fn test_is_gate_open_returns_true_when_enabled() {
        assert!(is_gate_open());
    }

    #[test]
    fn test_scan_interval() {
        assert_eq!(SESSION_SCAN_INTERVAL_MS, 10 * 60 * 1000);
    }

    #[test]
    fn test_read_last_consolidated_at_no_lock() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(consolidation_lock::read_last_consolidated_at());
        assert_eq!(result, 0);
    }

    #[test]
    fn test_lock_path_is_inside_memory_dir() {
        let lock = consolidation_lock::lock_path();
        let mem = get_auto_mem_path();
        let lock_str = lock.to_string_lossy();
        let mem_str = mem.to_string_lossy();
        assert!(lock_str.starts_with(mem_str.as_ref()));
    }

    #[test]
    fn test_init_auto_dream_returns_handle() {
        let handle = init_auto_dream();
        assert!(std::mem::size_of_val(&handle) > 0);
    }

    #[test]
    fn test_dream_task_state() {
        let state = DreamTaskState {
            task_id: "task-1".to_string(),
            sessions_reviewing: 5,
            files_touched: vec!["mem1.md".to_string(), "mem2.md".to_string()],
            status: DreamTaskStatus::Running,
        };
        assert_eq!(state.sessions_reviewing, 5);
        assert_eq!(state.files_touched.len(), 2);
        assert!(matches!(state.status, DreamTaskStatus::Running));
    }
}
