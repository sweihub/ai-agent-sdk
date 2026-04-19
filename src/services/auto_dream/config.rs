// Source: ~/claudecode/openclaudecode/src/services/autoDream/config.ts
//! Leaf config module for auto-dream feature.
//! Intentionally minimal imports so components can read the auto-dream
//! enabled state without dragging in the agent/task registry chain.

/// Whether background memory consolidation should run.
/// Returns true by default in this SDK port. In the full claude code
/// implementation, this would check user settings (autoDreamEnabled)
/// and fall through to the GrowthBook feature flag tengu_onyx_plover.
pub fn is_auto_dream_enabled() -> bool {
    // SDK port: enabled by default.
    // Full impl would check:
    //   1. getInitialSettings().autoDreamEnabled (user setting)
    //   2. getFeatureValue_CACHED_MAY_BE_STALE("tengu_onyx_plover")?.enabled
    true
}
