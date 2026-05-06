// Source: /data/home/swei/claudecode/openclaudecode/src/services/contextCollapse/index.ts
//! Context collapse module for managing context compression.
//!
//! Translated from TypeScript contextCollapse/index.ts

pub mod operations;
pub mod persist;

pub use operations::*;
pub use persist::*;

/// Statistics about context collapse operations
#[derive(Debug, Clone, Default)]
pub struct ContextCollapseStats {
    pub collapsed_spans: usize,
    pub staged_spans: usize,
    pub health: ContextCollapseHealth,
}

/// Health metrics for context collapse
#[derive(Debug, Clone, Default)]
pub struct ContextCollapseHealth {
    pub total_errors: usize,
    pub total_empty_spawns: usize,
    pub empty_spawn_warning_emitted: bool,
}

/// Module-level stats
static STATS: std::sync::LazyLock<std::sync::Mutex<ContextCollapseStats>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ContextCollapseStats::default()));

/// Check if context collapse is enabled
/// In TypeScript, this always returns false (feature gate)
/// In Rust, we check for the feature flag
pub fn is_context_collapse_enabled() -> bool {
    // Feature gate: context collapse is disabled by default
    // In full implementation, this would check the feature flag
    false
}

/// Reset context collapse state
pub fn reset_context_collapse() {
    let mut stats = STATS.lock().unwrap();
    *stats = ContextCollapseStats::default();
}

/// Apply collapses to messages if needed
/// Returns the messages and whether they were changed
pub fn apply_collapses_if_needed<T>(messages: T) -> ContextCollapseApplyResult<T>
where
    T: Clone,
{
    // In full implementation, this would apply context collapse
    // For now, just return the messages unchanged
    ContextCollapseApplyResult {
        messages,
        changed: false,
    }
}

/// Check if withheld prompt is too long
pub fn is_withheld_prompt_too_long() -> bool {
    false
}

/// Recover from overflow situation
pub fn recover_from_overflow<T>(messages: T) -> T
where
    T: Clone,
{
    // In full implementation, this would attempt recovery
    messages
}

/// Result of applying collapses
#[derive(Debug, Clone)]
pub struct ContextCollapseApplyResult<T> {
    pub messages: T,
    pub changed: bool,
}

/// Get context collapse statistics
pub fn get_stats() -> ContextCollapseStats {
    STATS.lock().unwrap().clone()
}

/// Increment collapsed spans counter
pub fn increment_collapsed_spans() {
    let mut stats = STATS.lock().unwrap();
    stats.collapsed_spans += 1;
}

/// Increment staged spans counter
pub fn increment_staged_spans() {
    let mut stats = STATS.lock().unwrap();
    stats.staged_spans += 1;
}

/// Record an error
pub fn record_error() {
    let mut stats = STATS.lock().unwrap();
    stats.health.total_errors += 1;
}

/// Record an empty spawn
pub fn record_empty_spawn() {
    let mut stats = STATS.lock().unwrap();
    stats.health.total_empty_spawns += 1;
}

/// Emit empty spawn warning
pub fn emit_empty_spawn_warning() {
    let mut stats = STATS.lock().unwrap();
    stats.health.empty_spawn_warning_emitted = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_context_collapse_enabled() {
        // Should return false by default
        assert!(!is_context_collapse_enabled());
    }

    #[test]
    fn test_get_stats_default() {
        let stats = get_stats();
        assert_eq!(stats.collapsed_spans, 0);
        assert_eq!(stats.staged_spans, 0);
        assert_eq!(stats.health.total_errors, 0);
        assert_eq!(stats.health.total_empty_spawns, 0);
        assert!(!stats.health.empty_spawn_warning_emitted);
    }

    #[test]
    #[serial_test::serial]
    fn test_increment_collapsed_spans() {
        increment_collapsed_spans();
        let stats = get_stats();
        assert_eq!(stats.collapsed_spans, 1);
        // Reset for other tests
        reset_context_collapse();
    }

    #[test]
    fn test_apply_collapses_if_needed_no_change() {
        let messages = vec![1, 2, 3];
        let result = apply_collapses_if_needed(messages.clone());
        assert!(!result.changed);
        assert_eq!(result.messages, messages);
    }

    #[test]
    fn test_recover_from_overflow() {
        let messages = vec![1, 2, 3];
        let result = recover_from_overflow(messages.clone());
        assert_eq!(result, messages);
    }
}
