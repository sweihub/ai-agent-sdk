//! Test utilities for integration tests.
//!
//! This module re-exported as `ai_agent::test_utils` provides
//! state reset functions for test isolation. It is intentionally
//! marked `#[doc(hidden)]` and should not be used in production code.

/// Reset all global mutable state to isolated defaults for testing.
///
/// Call this at the start of each integration test that modifies global state.
pub fn clear_all_test_state() {
    // Tool stores
    crate::tools::todo::reset_todos_for_testing();
    crate::tools::cron::reset_cron_jobs_for_testing();
    crate::tools::config::reset_config_for_testing();
    crate::tools::skill::reset_skills_for_testing();
    crate::tools::team::reset_teams_for_testing();
    crate::tools::plan::reset_plan_for_testing();
    crate::tools::agent::reset_agent_color_map_for_testing();
    crate::tools::worktree::reset_worktree_for_testing_sync();
    crate::tools::tasks::reset_task_store();
    crate::utils::task_list::reset_task_store();

    // Skills, caches, compact state
    crate::skills::bundled_skills::clear_bundled_skills();
    crate::services::compact::microcompact::reset_microcompact_state();
    crate::services::context_collapse::reset_context_collapse();
    crate::services::analytics::reset_for_testing();
    crate::session_memory::reset_session_memory_state();

    // Team memory, cleanup state, session globals
    crate::team_memory::reset_team_memory_for_testing();
    crate::services::compact::post_compact_cleanup::reset_cleanup_state_for_testing();
    crate::session::reset_session_globals_for_testing();
}
