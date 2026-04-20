use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Default minimum background time (5 minutes, matching away summary blur delay)
pub const DEFAULT_MIN_BACKGROUND_TIME_MS: u64 = 5 * 60 * 1000;

pub struct SessionBackgroundingState {
    is_backgrounded: AtomicBool,
    backgrounded_at: AtomicU64,
    last_activity: AtomicU64,
    min_background_time: Duration,
}

impl SessionBackgroundingState {
    pub fn new(min_background_time_ms: u64) -> Self {
        Self {
            is_backgrounded: AtomicBool::new(false),
            backgrounded_at: AtomicU64::new(0),
            last_activity: AtomicU64::new(now_timestamp()),
            min_background_time: Duration::from_millis(min_background_time_ms),
        }
    }

    pub fn set_backgrounded(&self, value: bool) {
        let now = now_timestamp();
        self.is_backgrounded.store(value, Ordering::SeqCst);
        if value {
            self.backgrounded_at.store(now, Ordering::SeqCst);
        } else {
            self.backgrounded_at.store(0, Ordering::SeqCst);
        }
    }

    pub fn is_backgrounded(&self) -> bool {
        self.is_backgrounded.load(Ordering::SeqCst)
    }

    pub fn update_activity(&self) {
        self.last_activity.store(now_timestamp(), Ordering::SeqCst);
    }

    pub fn get_last_activity(&self) -> u64 {
        self.last_activity.load(Ordering::SeqCst)
    }

    pub fn get_backgrounded_duration(&self) -> Option<Duration> {
        if self.is_backgrounded() {
            let backgrounded_at = self.backgrounded_at.load(Ordering::SeqCst);
            if backgrounded_at > 0 {
                let now = now_timestamp();
                let elapsed = now.saturating_sub(backgrounded_at);
                return Some(Duration::from_millis(elapsed));
            }
        }
        None
    }

    pub fn should_show_away_summary(&self) -> bool {
        if !self.is_backgrounded() {
            return false;
        }

        if let Some(duration) = self.get_backgrounded_duration() {
            duration >= self.min_background_time
        } else {
            false
        }
    }
}

fn now_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_backgrounding_state() {
        let state = SessionBackgroundingState::new(1000);

        assert!(!state.is_backgrounded());

        state.set_backgrounded(true);
        assert!(state.is_backgrounded());

        state.set_backgrounded(false);
        assert!(!state.is_backgrounded());
    }

    #[test]
    fn test_backgrounded_duration() {
        let state = SessionBackgroundingState::new(0);

        state.set_backgrounded(true);
        let duration = state.get_backgrounded_duration();
        assert!(duration.is_some());
    }
}
