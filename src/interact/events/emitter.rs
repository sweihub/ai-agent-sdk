// Source: ~/claudecode/openclaudecode/src/ink/events/emitter.ts

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::Waker;

/// Thread-safe EventEmitter similar to Node's EventEmitter but aware of our Event class
/// and respects `stop_immediate_propagation()`.
pub struct EventEmitter {
    listeners: Arc<Mutex<HashMap<String, Vec<Arc<Mutex<Listener>>>>>>,
}

pub(crate) struct Listener {
    callback: Box<dyn Fn(&crate::interact::events::event::Event) + Send + 'static>,
    waker: Option<Waker>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Emit an event to all listeners. If the event is an Event and
    /// did_stop_immediate_propagation() returns true, stop calling remaining listeners.
    pub fn emit(&self, event_type: &str, event: crate::interact::events::event::Event) -> bool {
        let listeners = {
            let guard = self.listeners.lock().unwrap();
            guard.get(event_type).cloned().unwrap_or_default()
        };

        if listeners.is_empty() {
            return false;
        }

        for listener in listeners {
            let listener_guard = listener.lock().unwrap();
            (listener_guard.callback)(&event);

            if event.did_stop_immediate_propagation() {
                break;
            }

            // Wake any waker if present
            if let Some(ref waker) = listener_guard.waker {
                waker.wake_by_ref();
            }
        }

        true
    }

    /// Add a listener for the given event type.
    pub(crate) fn on(
        &self,
        event_type: &str,
        callback: Box<dyn Fn(&crate::interact::events::event::Event) + Send + 'static>,
    ) -> Arc<Mutex<Listener>> {
        let listener = Arc::new(Mutex::new(Listener {
            callback,
            waker: None,
        }));

        let mut guard = self.listeners.lock().unwrap();
        guard.entry(event_type.to_string()).or_default().push(listener.clone());

        listener
    }

    /// Add a listener that will be removed after the first invocation.
    pub fn once(
        &self,
        event_type: &str,
        callback: Box<dyn Fn(&crate::interact::events::event::Event) + Send + 'static>,
    ) {
        let emitter = self.clone();
        let event_type_owned = event_type.to_string();
        let callback = Box::new(move |event: &crate::interact::events::event::Event| {
            callback(event);
            emitter.remove_all_listeners(&event_type_owned);
        });

        let _ = self.on(event_type, callback);
    }

    /// Remove all listeners for a given event type.
    pub fn remove_all_listeners(&self, event_type: &str) {
        let mut guard = self.listeners.lock().unwrap();
        guard.remove(event_type);
    }

    /// Remove a specific listener.
    pub(crate) fn remove_listener(&self, listener: &Arc<Mutex<Listener>>) {
        let mut guard = self.listeners.lock().unwrap();
        for listeners in guard.values_mut() {
            listeners.retain(|l| !Arc::ptr_eq(l, listener));
        }
    }

    /// Get the number of listeners for a given event type.
    pub fn listener_count(&self, event_type: &str) -> usize {
        let guard = self.listeners.lock().unwrap();
        guard.get(event_type).map(|l| l.len()).unwrap_or(0)
    }

    /// Remove all listeners for all event types.
    pub fn clear(&self) {
        let mut guard = self.listeners.lock().unwrap();
        guard.clear();
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventEmitter {
    fn clone(&self) -> Self {
        Self {
            listeners: self.listeners.clone(),
        }
    }
}