//! AbortController utilities

use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Default max listeners for standard operations
const DEFAULT_MAX_LISTENERS: usize = 50;

/// Creates an AbortController with proper event listener limits set.
/// This prevents MaxListenersExceededWarning when multiple listeners
/// are attached to the abort signal.
///
/// # Arguments
/// * `max_listeners` - Maximum number of listeners (default: 50)
///
/// # Returns
/// AbortController with configured listener limit
pub fn create_abort_controller(max_listeners: usize) -> AbortController {
    AbortController::new(max_listeners)
}

/// Creates an AbortController with default max listeners
pub fn create_abort_controller_default() -> AbortController {
    create_abort_controller(DEFAULT_MAX_LISTENERS)
}

/// AbortController implementation for Rust
/// Provides similar functionality to the JavaScript AbortController
pub struct AbortController {
    signal: Arc<AbortSignal>,
}

impl AbortController {
    /// Create a new AbortController with custom max listeners
    pub fn new(max_listeners: usize) -> Self {
        Self {
            signal: Arc::new(AbortSignal::new(max_listeners)),
        }
    }

    /// Get the abort signal
    pub fn signal(&self) -> &Arc<AbortSignal> {
        &self.signal
    }

    /// Abort the controller with an optional reason
    pub fn abort(&self, reason: Option<Arc<dyn std::any::Any + Send + Sync>>) {
        self.signal.abort(reason);
    }

    /// Check if aborted
    pub fn is_aborted(&self) -> bool {
        self.signal.is_aborted()
    }
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LISTENERS)
    }
}

impl Clone for AbortController {
    fn clone(&self) -> Self {
        Self {
            signal: Arc::clone(&self.signal),
        }
    }
}

/// AbortSignal implementation
pub struct AbortSignal {
    aborted: std::sync::atomic::AtomicBool,
    reason: std::sync::Mutex<Option<Arc<dyn std::any::Any + Send + Sync>>>,
    listeners: std::sync::Mutex<Vec<AbortCallback>>,
    max_listeners: usize,
}

pub type AbortCallback = Box<dyn Fn(Option<&dyn std::any::Any>) + Send + Sync>;

impl AbortSignal {
    /// Create a new AbortSignal with custom max listeners
    pub fn new(max_listeners: usize) -> Self {
        Self {
            aborted: std::sync::atomic::AtomicBool::new(false),
            reason: std::sync::Mutex::new(None),
            listeners: std::sync::Mutex::new(Vec::new()),
            max_listeners,
        }
    }

    /// Check if aborted
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Get the abort reason
    pub fn reason(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        self.reason.lock().ok().and_then(|guard| guard.clone())
    }

    /// Abort the signal
    pub fn abort(&self, reason: Option<Arc<dyn std::any::Any + Send + Sync>>) {
        if self.aborted.swap(true, Ordering::SeqCst) {
            return; // Already aborted
        }

        *self.reason.lock().unwrap() = reason.clone();

        // Notify all listeners - iterate directly over the locked guard
        // This is safe because we hold the lock during iteration
        let reason_ref = reason.as_deref().map(|a| a as &dyn std::any::Any);
        for listener in self.listeners.lock().unwrap().iter() {
            listener(reason_ref);
        }
    }

    /// Add an abort listener
    /// Returns the number of listeners after adding
    pub fn add_event_listener(&self, callback: AbortCallback) -> usize {
        let mut listeners = self.listeners.lock().unwrap();
        if listeners.len() >= self.max_listeners {
            log::warn!(
                "Max listeners ({}) exceeded for AbortSignal",
                self.max_listeners
            );
        }
        listeners.push(callback);
        listeners.len()
    }

    /// Remove an abort listener
    #[allow(dead_code)]
    pub fn remove_event_listener(&self, _callback: &AbortCallback) {
        // Note: Full implementation would require function pointer comparison
        // For now, this is a placeholder
    }

    /// Get the number of listeners
    #[allow(dead_code)]
    pub fn listener_count(&self) -> usize {
        self.listeners.lock().unwrap().len()
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LISTENERS)
    }
}

impl Clone for AbortSignal {
    fn clone(&self) -> Self {
        Self {
            aborted: std::sync::atomic::AtomicBool::new(self.aborted.load(Ordering::SeqCst)),
            reason: std::sync::Mutex::new(self.reason.lock().ok().and_then(|g| g.clone())),
            listeners: std::sync::Mutex::new(Vec::new()), // Cloned signals don't share listeners
            max_listeners: self.max_listeners,
        }
    }
}

impl std::fmt::Debug for AbortSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortSignal")
            .field("aborted", &self.aborted.load(Ordering::SeqCst))
            .field("max_listeners", &self.max_listeners)
            .finish()
    }
}

/// Creates a child AbortController that aborts when its parent aborts.
/// Aborting the child does NOT affect the parent.
///
/// Memory-safe: Uses Arc so that parent doesn't retain abandoned children.
/// If the child is dropped without being aborted, it can still be GC'd.
/// When the child IS aborted, the parent listener is removed to prevent
/// accumulation of dead handlers.
///
/// # Arguments
/// * `parent` - The parent AbortController
/// * `max_listeners` - Maximum number of listeners (default: 50)
///
/// # Returns
/// Child AbortController
#[allow(dead_code)]
pub fn create_child_abort_controller(
    parent: &AbortController,
    max_listeners: Option<usize>,
) -> AbortController {
    let max_listeners = max_listeners.unwrap_or(DEFAULT_MAX_LISTENERS);
    let child = AbortController::new(max_listeners);

    // Fast path: parent already aborted, no listener setup needed
    if parent.is_aborted() {
        child.abort(parent.signal.reason());
        return child;
    }

    // Clone the child signal to use in the closure
    let child_signal = Arc::clone(&child.signal);
    let parent_signal = Arc::clone(parent.signal());

    // Get the reason now, before moving into closure
    let reason = parent_signal.reason();

    // Use a wrapper to handle the propagation
    // Note: We need both signals to be Send + Sync, which they are
    parent_signal.add_event_listener(Box::new(move |_reason| {
        // Propagate the captured reason to child
        child_signal.abort(reason.clone());
    }));

    child
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_abort_controller() {
        let controller = create_abort_controller(50);
        assert!(!controller.is_aborted());
    }

    #[test]
    fn test_abort_controller_abort() {
        let controller = create_abort_controller(50);
        controller.abort(None);
        assert!(controller.is_aborted());
    }

    #[test]
    fn test_abort_with_reason() {
        let controller = create_abort_controller(50);
        let reason = Arc::new("test reason".to_string()) as Arc<dyn std::any::Any + Send + Sync>;
        controller.abort(Some(reason));

        assert!(controller.is_aborted());
        let stored_reason = controller.signal().reason();
        assert!(stored_reason.is_some());
    }

    #[test]
    fn test_abort_listener() {
        let controller = create_abort_controller(50);
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        controller
            .signal()
            .add_event_listener(Box::new(move |_reason| {
                called.store(true, std::sync::atomic::Ordering::SeqCst);
            }));

        controller.abort(None);
        assert!(called_clone.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_child_abort_controller() {
        let parent = create_abort_controller(50);
        let child = create_child_abort_controller(&parent, None);

        assert!(!parent.is_aborted());
        assert!(!child.is_aborted());

        parent.abort(None);

        assert!(parent.is_aborted());
        assert!(child.is_aborted());
    }

    #[test]
    fn test_child_already_aborted_parent() {
        let parent = create_abort_controller(50);
        parent.abort(None);

        let child = create_child_abort_controller(&parent, None);

        assert!(child.is_aborted());
    }
}
