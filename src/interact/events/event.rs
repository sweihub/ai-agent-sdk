// Source: ~/claudecode/openclaudecode/src/ink/events/event.ts

use std::sync::atomic::{AtomicBool, Ordering};

/// Base event class for all ink events.
/// Supports stop_immediate_propagation() to prevent downstream listeners from firing.
#[derive(Debug)]
pub struct Event {
    did_stop_immediate_propagation: AtomicBool,
}

impl Event {
    pub fn new() -> Self {
        Self {
            did_stop_immediate_propagation: AtomicBool::new(false),
        }
    }

    pub fn did_stop_immediate_propagation(&self) -> bool {
        self.did_stop_immediate_propagation.load(Ordering::SeqCst)
    }

    pub fn stop_immediate_propagation(&self) {
        self.did_stop_immediate_propagation.store(true, Ordering::SeqCst);
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Event {
    fn clone(&self) -> Self {
        Self {
            did_stop_immediate_propagation: AtomicBool::new(
                self.did_stop_immediate_propagation.load(Ordering::SeqCst)
            ),
        }
    }
}