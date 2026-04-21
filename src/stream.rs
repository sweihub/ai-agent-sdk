// Source: Internal module — async stream interface for CLI/TUI users
// Provides futures::Stream-based event consumption for the AI Agent SDK.

use crate::types::{AgentEvent, QueryResult};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use tokio::sync::mpsc;

// Re-use the Stream trait from futures_util (already a dependency)
use futures_util::Stream;

/// A stream of [`AgentEvent`] from a single query, returned by [`crate::agent::Agent::query_stream`].
///
/// The engine loop runs on a spawned tokio task. Events flow through an internal
/// `mpsc` channel that this stream polls. The stream terminates when a
/// [`AgentEvent::Done`] event fires (both on normal completion and abort).
///
/// # Example
///
/// ```rust,ignore
/// let mut stream = agent.query_stream("hello").await?;
/// while let Some(ev) = stream.next().await {
///     match ev {
///         AgentEvent::ContentBlockDelta {
///             delta: AgentEvent::ContentDelta::Text { text },
///             ..
///         } => print!("{}", text),
///         AgentEvent::Done { result } => println!("\nDone!"),
///         _ => {}
///     }
/// }
/// ```
pub struct QueryStream {
    receiver: mpsc::Receiver<AgentEvent>,
    task: tokio::task::JoinHandle<()>,
    result: Arc<OnceLock<QueryResult>>,
}

impl QueryStream {
    pub(crate) fn new(
        receiver: mpsc::Receiver<AgentEvent>,
        task: tokio::task::JoinHandle<()>,
        result: Arc<OnceLock<QueryResult>>,
    ) -> Self {
        Self { receiver, task, result }
    }

    /// Get the final [`QueryResult`] after the stream has completed.
    ///
    /// Returns `None` if the stream is still running.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut stream = agent.query_stream("hello").await?;
    /// while let Some(ev) = stream.next().await {}
    /// let result = stream.result().expect("stream completed");
    /// println!("turns={}", result.num_turns);
    /// ```
    pub fn result(&self) -> Option<QueryResult> {
        self.result.get().cloned()
    }
}

impl Stream for QueryStream {
    type Item = AgentEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Try non-blocking recv first (avoids waker registration for completed streams)
        match self.receiver.try_recv() {
            Ok(event) => Poll::Ready(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => {
                // Register waker and await
                match Pin::new(&mut self.receiver).poll_recv(cx) {
                    Poll::Ready(Some(event)) => Poll::Ready(Some(event)),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
            Err(mpsc::error::TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }
}

impl Drop for QueryStream {
    fn drop(&mut self) {
        // Abort the spawned task when the stream is dropped
        self.task.abort();
    }
}

/// A subscriber to agent events across multiple queries.
///
/// Returned by [`crate::agent::Agent::subscribe`]. Events from the *current*
/// and *subsequent* queries flow to the subscriber until the associated
/// [`CancelGuard`] is dropped.
///
/// # Example
///
/// ```rust,ignore
/// let (mut sub, _guard) = agent.subscribe();
///
/// tokio::spawn(async move {
///     agent.query("hello").await;
/// });
///
/// while let Some(ev) = sub.next().await {
///     // render in TUI
/// }
/// ```
pub struct EventSubscriber {
    receiver: mpsc::Receiver<AgentEvent>,
}

impl EventSubscriber {
    pub(crate) fn new(receiver: mpsc::Receiver<AgentEvent>) -> Self {
        Self { receiver }
    }
}

impl Stream for EventSubscriber {
    type Item = AgentEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.receiver).poll_recv(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(Some(event)),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Token that unsubscribes the [`EventSubscriber`] when dropped.
///
/// Dropping this guard stops event delivery to the associated subscriber.
/// The subscriber's stream will return `None` on the next poll.
pub struct CancelGuard {
    _sender: Option<mpsc::Sender<AgentEvent>>,
}

impl CancelGuard {
    /// Create a new guard with the given sender.
    pub(crate) fn new(sender: mpsc::Sender<AgentEvent>) -> Self {
        Self {
            _sender: Some(sender),
        }
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        // Drop the sender to close the channel, which will cause the receiver to return None
        self._sender.take();
    }
}

