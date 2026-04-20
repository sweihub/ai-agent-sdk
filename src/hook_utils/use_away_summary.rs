//! Away summary hook — manages "while you were away" session recap generation.
//!
//! Translates useAwaySummary.ts from claude code.
//! Integrates:
//! - Terminal focus state (from `utils::terminal_focus`)
//! - Session backgrounding state (from `use_session_backgrounding`)
//! - Away summary generation (from `services::away_summary`)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::hook_utils::use_session_backgrounding::{
    SessionBackgroundingState, DEFAULT_MIN_BACKGROUND_TIME_MS,
};
use crate::types::Message;

/// Default blur delay: 5 minutes
pub const DEFAULT_BLUR_DELAY_MS: u64 = 5 * 60 * 1000;

/// Manager for the "while you were away" away summary feature.
///
/// Tracks whether the terminal has been blurred (backgrounded) for the
/// configured delay, and determines when an away summary should be generated.
pub struct AwaySummaryManager {
    /// Whether the away summary feature is enabled
    enabled: bool,
    /// Session backgrounding state (tracks blur/fg focus transitions)
    backgrounding: SessionBackgroundingState,
    /// Whether a summary has been generated since the last user turn
    has_summary_since_last_user: bool,
    /// Whether a summary generation is currently in progress
    is_generating: bool,
    /// Whether a summary generation is pending (timer fired during a turn)
    is_pending: bool,
    /// The last time the terminal was set to blurred
    blurred_at: Option<Instant>,
    /// The blur delay before triggering summary generation
    blur_delay_ms: u64,
}

impl AwaySummaryManager {
    /// Create a new away summary manager with default settings.
    pub fn new() -> Self {
        Self {
            enabled: true,
            backgrounding: SessionBackgroundingState::new(DEFAULT_MIN_BACKGROUND_TIME_MS),
            has_summary_since_last_user: false,
            is_generating: false,
            is_pending: false,
            blurred_at: None,
            blur_delay_ms: DEFAULT_BLUR_DELAY_MS,
        }
    }

    /// Create with custom settings.
    pub fn with_settings(
        enabled: bool,
        blur_delay_ms: u64,
        min_background_time_ms: u64,
    ) -> Self {
        Self {
            enabled,
            backgrounding: SessionBackgroundingState::new(min_background_time_ms),
            has_summary_since_last_user: false,
            is_generating: false,
            is_pending: false,
            blurred_at: None,
            blur_delay_ms,
        }
    }

    /// Enable or disable the away summary feature.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // Reset state when disabled
            self.has_summary_since_last_user = false;
            self.is_generating = false;
            self.is_pending = false;
            self.blurred_at = None;
            self.backgrounding.set_backgrounded(false);
        }
    }

    /// Check if the feature is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Called when the terminal loses focus (blurred).
    /// Starts the backgrounding timer.
    pub fn on_terminal_blurred(&mut self) {
        if !self.enabled {
            return;
        }
        self.backgrounding.set_backgrounded(true);
        self.blurred_at = Some(Instant::now());
    }

    /// Called when the terminal gains focus (focused).
    /// Cancels any pending summary generation.
    pub fn on_terminal_focused(&mut self) {
        if !self.enabled {
            return;
        }
        self.backgrounding.set_backgrounded(false);
        self.is_pending = false;
        self.blurred_at = None;
    }

    /// Check if the away summary should be shown (i.e., blur delay has elapsed).
    /// The summary is ready when:
    /// - Feature is enabled
    /// - Session has been backgrounded for at least the blur delay
    /// - No summary has been generated since the last user turn
    /// - Not currently generating
    /// - Not currently loading (user is idle)
    pub fn should_generate_summary(
        &self,
        is_loading: bool,
        has_summary_since_last_user: bool,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if is_loading {
            return false;
        }
        if has_summary_since_last_user || self.has_summary_since_last_user {
            return false;
        }
        if self.is_generating {
            return false;
        }
        // Check if enough time has passed since blurred
        if let Some(blurred_at) = self.blurred_at {
            let elapsed = blurred_at.elapsed().as_millis() as u64;
            return elapsed >= self.blur_delay_ms;
        }
        false
    }

    /// Mark that a summary has been generated since the last user turn.
    pub fn mark_summary_generated(&mut self) {
        self.has_summary_since_last_user = true;
        self.is_generating = false;
        self.is_pending = false;
    }

    /// Mark that a summary generation failed or returned empty.
    pub fn mark_summary_failed(&mut self) {
        self.is_generating = false;
        self.is_pending = false;
    }

    /// Mark that generation is in progress.
    pub fn mark_generating(&mut self) {
        self.is_generating = true;
        self.is_pending = false;
    }

    /// Check if a summary generation is pending (timer fired during a turn).
    pub fn is_pending_summary(&self) -> bool {
        self.is_pending
    }

    /// Called when a turn completes and a summary generation was pending.
    /// If still blurred, triggers generation.
    pub fn on_turn_complete(&self) -> bool {
        // Returns true if a summary should be generated now
        !self.is_generating && !self.has_summary_since_last_user
    }

    /// Get the time since the terminal was blurred.
    pub fn time_since_blurred(&self) -> Option<std::time::Duration> {
        self.blurred_at.map(|t| t.elapsed())
    }

    /// Check if the session is currently backgrounded.
    pub fn is_backgrounded(&self) -> bool {
        self.backgrounding.is_backgrounded()
    }

    /// Reset the "summary generated" flag (called when a new user message arrives).
    pub fn reset_summary_flag(&mut self) {
        self.has_summary_since_last_user = false;
    }
}

impl Default for AwaySummaryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if there's an away summary message in the messages since the last user turn.
/// A summary "since last user turn" means there's an away_summary system message
/// after the most recent non-meta user message.
pub fn has_away_summary_since_last_user(messages: &[Message]) -> bool {
    // Iterate from the end, looking for either:
    // 1. A user message (non-meta) → no summary found since this point
    // 2. An away_summary system message → summary found
    for msg in messages.iter().rev() {
        match msg {
            Message::User(user_msg) => {
                // Non-meta user message resets the search
                if !user_msg.is_meta.unwrap_or(false) {
                    return false;
                }
            }
            Message::System(sys_msg) => {
                if sys_msg.subtype.as_deref() == Some("away_summary") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if there's an away summary message in the messages since the last user turn.
pub fn has_away_summary_since_last_user_for(messages: &[Message]) -> bool {
    for msg in messages.iter().rev() {
        match msg {
            Message::User(user_msg) => {
                if !user_msg.is_meta.unwrap_or(false) && !user_msg.is_compact_summary.unwrap_or(false)
                {
                    return false;
                }
            }
            Message::System(sys_msg) => {
                if sys_msg.subtype.as_deref() == Some("away_summary") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Generate an away summary with the given parameters.
/// This is the core function that wraps `generate_away_summary` from the service.
pub async fn generate_away_summary_with_state(
    messages: &[Message],
    api_key: &str,
    abort_signal: &Arc<AtomicBool>,
) -> Option<String> {
    if messages.is_empty() {
        return None;
    }

    // Check abort signal
    if abort_signal.load(Ordering::SeqCst) {
        return None;
    }

    // Check if we already have a summary since the last user turn
    if has_away_summary_since_last_user(messages) {
        return None;
    }

    // Call the service-level generate function
    let result = crate::services::away_summary::generate_away_summary(
        messages,
        api_key,
        abort_signal,
    ).await;

    result.summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_enabled_by_default() {
        let manager = AwaySummaryManager::new();
        assert!(manager.is_enabled());
        assert!(!manager.is_backgrounded());
    }

    #[test]
    fn test_on_blur_sets_backgrounded() {
        let mut manager = AwaySummaryManager::new();
        manager.on_terminal_blurred();
        assert!(manager.is_backgrounded());
        assert!(manager.time_since_blurred().is_some());
    }

    #[test]
    fn test_on_focus_clears_backgrounded() {
        let mut manager = AwaySummaryManager::new();
        manager.on_terminal_blurred();
        manager.on_terminal_focused();
        assert!(!manager.is_backgrounded());
        assert!(manager.time_since_blurred().is_none());
    }

    #[test]
    fn test_should_not_generate_when_loading() {
        let manager = AwaySummaryManager::new();
        assert!(!manager.should_generate_summary(true, false));
    }

    #[test]
    fn test_should_not_generate_when_has_summary() {
        let manager = AwaySummaryManager::new();
        assert!(!manager.should_generate_summary(false, true));
    }

    #[test]
    fn test_disable_resets_state() {
        let mut manager = AwaySummaryManager::new();
        manager.on_terminal_blurred();
        manager.has_summary_since_last_user = true;
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        assert!(!manager.is_backgrounded());
        assert!(!manager.has_summary_since_last_user);
    }

    #[test]
    fn test_blur_delay_checks() {
        let manager = AwaySummaryManager::new();
        // With blur_at = None, should not generate (no elapsed time)
        assert!(!manager.should_generate_summary(false, false));
    }

    #[test]
    fn test_reset_summary_flag() {
        let mut manager = AwaySummaryManager::new();
        manager.has_summary_since_last_user = true;
        manager.reset_summary_flag();
        assert!(!manager.has_summary_since_last_user);
    }
}
