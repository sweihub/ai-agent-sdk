//! Analytics service - public API for event logging
//!
//! This module serves as the main entry point for analytics events.
//!
//! DESIGN: This module has NO dependencies to avoid import cycles.
//! Events are queued until attach_analytics_sink() is called during app initialization.
//! The sink handles routing to Datadog and 1P event logging.

pub mod config;
pub mod datadog;
pub mod first_party_event_logger;
pub mod first_party_event_logging_exporter;
pub mod growthbook;
pub mod metadata;
pub mod sink;
pub mod sink_killswitch;

// Re-export all submodules
pub use config::*;
pub use datadog::*;
pub use first_party_event_logger::*;
pub use first_party_event_logging_exporter::*;
pub use growthbook::*;
pub use metadata::*;
pub use sink::*;
pub use sink_killswitch::*;

/// Marker type for verifying analytics metadata doesn't contain sensitive data
/// This type forces explicit verification that string values being logged
/// don't contain code snippets, file paths, or other sensitive information.
/// Usage: `my_string as AnalyticsMetadataVerified`
pub type AnalyticsMetadataVerified = ();

/// Marker type for values routed to PII-tagged proto columns
pub type AnalyticsMetadataPiiTagged = ();

/// Log event metadata type
pub type LogEventMetadata = std::collections::HashMap<String, serde_json::Value>;

/// Queued event structure
#[derive(Debug, Clone)]
struct QueuedEvent {
    event_name: String,
    metadata: LogEventMetadata,
    is_async: bool,
}

/// Sink interface for the analytics backend
pub trait AnalyticsSink: Send + Sync {
    fn log_event(&self, event_name: &str, metadata: &LogEventMetadata);
    fn log_event_async(
        &self,
        event_name: &str,
        metadata: &LogEventMetadata,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

/// Strip `_PROTO_*` keys from a payload destined for general-access storage.
/// Used by sink.rs before Datadog fanout and first_party_event_logging_exporter
/// for defensive stripping after hoisting known _PROTO_* keys.
pub fn strip_proto_fields<V: Clone>(
    metadata: &std::collections::HashMap<String, V>,
) -> std::collections::HashMap<String, V> {
    let mut result: Option<std::collections::HashMap<String, V>> = None;

    for key in metadata.keys() {
        if key.starts_with("_PROTO_") {
            if result.is_none() {
                result = Some(metadata.clone());
            }
            if let Some(ref mut r) = result {
                r.remove(key);
            }
        }
    }

    result.unwrap_or_else(|| metadata.clone())
}

/// Internal event queue for events logged before sink is attached
static EVENT_QUEUE: std::sync::OnceLock<std::sync::Mutex<Vec<QueuedEvent>>> =
    std::sync::OnceLock::new();

fn get_event_queue() -> &'static std::sync::Mutex<Vec<QueuedEvent>> {
    EVENT_QUEUE.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Sink - initialized during app startup
static ANALYTICS_SINK: std::sync::Mutex<Option<std::sync::Arc<dyn AnalyticsSink>>> =
    std::sync::Mutex::new(None);

fn get_sink() -> Option<std::sync::Arc<dyn AnalyticsSink>> {
    ANALYTICS_SINK.lock().unwrap().clone()
}

/// Attach the analytics sink that will receive all events.
/// Queued events are drained asynchronously via microtask to avoid
/// adding latency to the startup path.
///
/// Idempotent: if a sink is already attached, this is a no-op.
pub fn attach_analytics_sink(new_sink: std::sync::Arc<dyn AnalyticsSink>) -> bool {
    let mut guard = ANALYTICS_SINK.lock().unwrap();
    if guard.is_some() {
        return false; // Already attached
    }

    *guard = Some(new_sink);

    // Drain the queue asynchronously
    let queue = get_event_queue();
    let mut queued_events = queue.lock().unwrap();

    if !queued_events.is_empty() {
        let events: Vec<QueuedEvent> = std::mem::take(&mut *queued_events);

        // Log queue size for debugging analytics initialization timing
        if let Some(sink) = get_sink() {
            let mut metadata = LogEventMetadata::new();
            metadata.insert(
                "queued_event_count".to_string(),
                serde_json::json!(events.len()),
            );
            sink.log_event("analytics_sink_attached", &metadata);
        }

        // Schedule async drain
        let sink = ANALYTICS_SINK.lock().unwrap().clone().expect("sink just set");

        // Use spawn to simulate queueMicrotask behavior
        std::thread::spawn(move || {
            for event in events {
                if event.is_async {
                    // For async events, we need to handle differently
                    let metadata = &event.metadata;
                    sink.log_event(&event.event_name, metadata);
                } else {
                    sink.log_event(&event.event_name, &event.metadata);
                }
            }
        });
    }

    true
}

/// Log an event to analytics backends (synchronous)
///
/// Events may be sampled based on the 'tengu_event_sampling_config' dynamic config.
/// When sampled, the sample_rate is added to the event metadata.
///
/// If no sink is attached, events are queued and drained when the sink attaches.
pub fn log_event(event_name: &str, metadata: LogEventMetadata) {
    if let Some(sink) = get_sink() {
        sink.log_event(event_name, &metadata);
    } else {
        let mut queue = get_event_queue().lock().unwrap();
        queue.push(QueuedEvent {
            event_name: event_name.to_string(),
            metadata,
            is_async: false,
        });
    }
}

/// Log an event to analytics backends (asynchronous)
///
/// Events may be sampled based on the 'tengu_event_sampling_config' dynamic config.
/// When sampled, the sample_rate is added to the event metadata.
///
/// If no sink is attached, events are queued and drained when the sink attaches.
pub async fn log_event_async(event_name: &str, metadata: LogEventMetadata) {
    if let Some(sink) = get_sink() {
        sink.log_event_async(event_name, &metadata).await;
    } else {
        let mut queue = get_event_queue().lock().unwrap();
        queue.push(QueuedEvent {
            event_name: event_name.to_string(),
            metadata,
            is_async: true,
        });
    }
}

/// Reset analytics state for testing purposes only.
pub fn reset_for_testing() {
    let mut queue = get_event_queue().lock().unwrap();
    queue.clear();
    *ANALYTICS_SINK.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_proto_fields_no_change() {
        let mut metadata: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        metadata.insert("event_name".to_string(), serde_json::json!("test"));
        metadata.insert("count".to_string(), serde_json::json!(42));

        let result = strip_proto_fields(&metadata);

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("event_name"));
        assert!(result.contains_key("count"));
    }

    #[test]
    fn test_strip_proto_fields_removes_proto() {
        let mut metadata: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        metadata.insert("event_name".to_string(), serde_json::json!("test"));
        metadata.insert("_PROTO_PII".to_string(), serde_json::json!("sensitive"));

        let result = strip_proto_fields(&metadata);

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("event_name"));
        assert!(!result.contains_key("_PROTO_PII"));
    }

    #[test]
    fn test_attach_analytics_sink_idempotent() {
        struct TestSink;
        impl AnalyticsSink for TestSink {
            fn log_event(&self, _event_name: &str, _metadata: &LogEventMetadata) {}
            fn log_event_async(
                &self,
                _event_name: &str,
                _metadata: &LogEventMetadata,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
                Box::pin(async {})
            }
        }

        let sink1 = std::sync::Arc::new(TestSink);
        let sink2 = std::sync::Arc::new(TestSink);

        let result1 = attach_analytics_sink(sink1);
        let result2 = attach_analytics_sink(sink2);

        assert!(result1);
        assert!(!result2); // Second attach should fail
    }
}
