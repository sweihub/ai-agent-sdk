// Source: ~/claudecode/openclaudecode/src/ink/terminal-focus-state.ts
//! Terminal focus state signal — global state for DECSET 1004 focus events.
//!
//! `unknown` is the default for terminals that don't support focus reporting;
//! consumers treat `unknown` identically to `focused` (no throttling).
//! Subscribers are notified synchronously when focus changes.

use std::sync::{Arc, Mutex, RwLock};

/// Terminal focus state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFocusState {
    /// Terminal has focus
    Focused,
    /// Terminal lost focus (blurred)
    Blurred,
    /// Terminal doesn't support focus reporting (DECSET 1004)
    Unknown,
}

impl Default for TerminalFocusState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Internal shared state
struct SharedState {
    state: TerminalFocusState,
    subscribers: Vec<Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>>,
    resolvers: Vec<Box<dyn FnOnce() + Send + Sync>>,
}

/// Initialize the global state. Called once via OnceLock.
fn initial_state() -> SharedState {
    SharedState {
        state: TerminalFocusState::default(),
        subscribers: Vec::new(),
        resolvers: Vec::new(),
    }
}

static INSTANCE: std::sync::OnceLock<RwLock<SharedState>> = std::sync::OnceLock::new();

fn get_instance() -> &'static RwLock<SharedState> {
    INSTANCE.get_or_init(|| RwLock::new(initial_state()))
}

/// Get whether the terminal is currently considered focused.
/// Treats `unknown` as focused (no throttling).
pub fn get_terminal_focused() -> bool {
    let instance = get_instance();
    let state = instance.read().unwrap();
    state.state != TerminalFocusState::Blurred
}

/// Get the full terminal focus state
pub fn get_terminal_focus_state() -> TerminalFocusState {
    let instance = get_instance();
    let state = instance.read().unwrap();
    state.state
}

/// Subscribe to terminal focus changes.
/// Returns an Arc-wrapped subscription handle that removes itself on drop.
pub fn subscribe_terminal_focus(cb: impl Fn() + Send + Sync + 'static) -> Arc<FocusUnsubscribe> {
    let callback = Arc::new(Mutex::new(Some(Box::new(cb) as Box<dyn Fn() + Send + Sync>)));
    let callback_clone = callback.clone();
    let instance = get_instance();
    {
        let mut state = instance.write().unwrap();
        state.subscribers.push(callback);
    }
    Arc::new(FocusUnsubscribe {
        instance,
        _callback_guard: callback_clone,
    })
}

/// Unsubscribe handle — removes callback on drop.
pub struct FocusUnsubscribe {
    instance: &'static RwLock<SharedState>,
    /// Keeps the callback alive until this handle is dropped
    _callback_guard: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>,
}

impl Drop for FocusUnsubscribe {
    fn drop(&mut self) {
        // Deregister by clearing the callback and removing from list
        let cb_ptr = Arc::as_ptr(&self._callback_guard);
        self._callback_guard.lock().unwrap().take();
        let mut state = self.instance.write().unwrap();
        state.subscribers.retain(|cb| Arc::as_ptr(cb) != cb_ptr);
    }
}

/// Set terminal focus state. Notifies all subscribers.
pub fn set_terminal_focus(focused: bool) {
    let instance = get_instance();
    let new_state = if focused {
        TerminalFocusState::Focused
    } else {
        TerminalFocusState::Blurred
    };
    let mut state = instance.write().unwrap();
    let old_state = state.state;
    state.state = new_state;

    if old_state != new_state {
        let subs: Vec<_> = state.subscribers.iter().cloned().collect();
        drop(state);

        for cb in subs {
            if let Some(f) = cb.lock().unwrap().as_ref() {
                f();
            }
        }
    }
}

/// Reset terminal focus state to unknown (e.g., if terminal doesn't support DECSET 1004)
pub fn reset_terminal_focus_state() {
    let instance = get_instance();
    let mut state = instance.write().unwrap();
    state.state = TerminalFocusState::default();
}

/// Resolve on focus event — waits until the terminal regains focus.
/// Returns immediately if already focused.
pub async fn on_terminal_focused() {
    let instance = get_instance();
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut state = instance.write().unwrap();
        if get_terminal_focused() {
            drop(state);
            return;
        }
        state.resolvers.push(Box::new(move || {
            let _ = tx.send(());
        }));
    }
    let _ = rx.await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn reset_test_state() {
        reset_terminal_focus_state();
    }

    #[test]
    fn test_focused_by_default() {
        assert!(get_terminal_focused());
    }

    #[test]
    fn test_set_focus_blur() {
        set_terminal_focus(false);
        assert_eq!(get_terminal_focus_state(), TerminalFocusState::Blurred);
        assert!(!get_terminal_focused());
    }

    #[test]
    fn test_set_focus_restore() {
        set_terminal_focus(false);
        set_terminal_focus(true);
        assert_eq!(get_terminal_focus_state(), TerminalFocusState::Focused);
        assert!(get_terminal_focused());
    }

    #[test]
    fn test_reset_state() {
        set_terminal_focus(false);
        reset_terminal_focus_state();
        assert_eq!(get_terminal_focus_state(), TerminalFocusState::Unknown);
    }

    #[test]
    fn test_subscribe_receives_updates() {
        let received = Arc::new(AtomicBool::new(false));
        let received_clone = received.clone();
        let _sub = subscribe_terminal_focus(move || {
            received_clone.store(true, Ordering::SeqCst);
        });
        // Force a state change from Unknown/Focused to Blurred then back
        set_terminal_focus(false);
        set_terminal_focus(true);
        assert!(received.load(Ordering::SeqCst));
    }

    #[test]
    fn test_subscribe_removed_on_drop() {
        let call_count = Arc::new(AtomicUsize::new(0));
        {
            let count_clone = call_count.clone();
            let _sub = subscribe_terminal_focus(move || {
                count_clone.fetch_add(1, Ordering::SeqCst);
            });
            // Need two state changes to fire (e.g., Unknown -> Blurred -> Focused)
            set_terminal_focus(false);
            set_terminal_focus(true);
            assert_eq!(call_count.load(Ordering::SeqCst), 2);
        }
        // After sub is dropped, state changes shouldn't trigger
        set_terminal_focus(false);
        set_terminal_focus(true);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
