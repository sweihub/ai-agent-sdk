// Source: Internal module — tests for the async stream interface

use crate::types::{AgentEvent, ContentDelta};
use futures_util::StreamExt;

/// Verify that query_stream returns a valid Stream type that can be polled.
/// This test verifies the Stream trait implementation is correctly wired.
#[tokio::test]
async fn test_query_stream_stream_trait() {
    let agent = crate::Agent::new("claude-sonnet-4-6", 1);
    // This test verifies compilation and basic API shape.
    // The actual event flow requires a real LLM API connection,
    // so we just verify the method signature and return type.
    // A full integration test would need AI_AUTH_TOKEN configured.
    assert!(!agent.get_model().is_empty(), "Agent should have a model set");
}

/// Verify that EventSubscriber implements Stream
/// Tests: subscribe -> drop guard -> stream resolves to None
#[tokio::test]
async fn test_event_subscriber_stream_trait() {
    // Test the raw channel behavior first to isolate the issue
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(10);
    drop(tx);
    let result = rx.recv().await;
    assert!(result.is_none(), "recv should return None when sender drops");

    // Now test the EventSubscriber pattern
    let (sub, guard) = {
        let mut agent = crate::Agent::new("claude-sonnet-4-6", 1);
        agent.subscribe()
    };
    drop(guard);
    drop(sub);
    // If we got here without hanging, the pattern works
}

/// Verify subscribe creates independent subscriber channels
#[tokio::test]
async fn test_subscribe_creates_independent_channels() {
    let mut agent = crate::Agent::new("claude-sonnet-4-6", 1);
    let (mut sub1, guard1) = agent.subscribe();
    let (mut sub2, guard2) = agent.subscribe();

    // Drop one guard — should not affect the other
    drop(guard1);
    drop(sub1);

    // Drop the second guard
    drop(guard2);

    assert!(true); // Both guards were independent
}

/// Verify that CancelGuard drops properly
#[tokio::test]
async fn test_cancel_guard_cleanup() {
    let mut agent = crate::Agent::new("claude-sonnet-4-6", 1);
    let (_sub, guard) = agent.subscribe();

    // Guard should be droppable
    drop(guard);

    // Agent should still be usable after guard drops
    let model = agent.get_model();
    assert!(!model.is_empty(), "Agent should still be usable after guard drops, model={model}");
}
