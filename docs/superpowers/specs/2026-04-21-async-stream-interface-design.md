# Async Stream Interface for CLI/TUI

**Date:** 2026-04-21
**Status:** Approved
**Goal:** Provide a convenient `futures::Stream` interface for CLI/TUI users to consume agent events incrementally.

## Problem

The current `Agent::query()` blocks the caller until the entire agent loop finishes. For real-time chat UIs that need to render text chunks as they stream in, show tool progress inline, and respond to user interrupt mid-stream, the callback mechanism (`set_event_callback`) is awkward — it cannot be composed with `tokio::select!`, `StreamExt`, or channel pipes.

The TypeScript SDK supports `AsyncIterable<SDKUserMessage>` input for streaming conversations, but the Rust SDK has no equivalent output streaming interface.

## Solution

Add two methods to `Agent`:

1. **`query_stream()`** — primary TUI interface. Returns a `QueryStream` that implements `futures::Stream<Item = AgentEvent>`.
2. **`subscribe()`** — pub/sub for multi-listener scenarios. Returns an `EventSubscriber` + `CancelGuard`.

Both wrap the same internal `tokio::sync::mpsc` channel dispatch. The existing callback mechanism (`set_event_callback`) and `query()` remain unchanged.

## Public API

### `query_stream()`

```rust
pub async fn query_stream(&mut self, prompt: &str) -> Result<QueryStream, AgentError>
```

Returns a `QueryStream` implementing `futures::Stream<Item = AgentEvent>`. The engine loop runs on a spawned tokio task. Events flow through an mpsc channel. The stream completes with `AgentEvent::Done { result }` on both normal completion and abort.

**Usage:**

```rust
let mut stream = agent.query_stream("write a hello world program").await?;
tokio::pin!(stream);

loop {
    tokio::select! {
        ev = stream.next() => match ev {
            Some(AgentEvent::ContentBlockDelta { delta: ContentDelta::Text { text }, .. }) => print!("{}", text),
            Some(AgentEvent::Done { result }) => { println!("\nDone! Turns: {}", result.num_turns); break; }
            Some(AgentEvent::ToolStart { tool_name, .. }) => eprintln!("\n[{}] Running: {}", tool_name, chrono::Utc::now().format("%H:%M:%S")),
            Some(_) => {}
            None => break,
        },
        _ = tokio::signal::ctrl_c() => {
            agent.interrupt();
        }
    }
}
```

### `subscribe()`

```rust
pub fn subscribe(&mut self) -> (EventSubscriber, CancelGuard)
```

Returns an `EventSubscriber` (implements `Stream<Item = AgentEvent>`) and a `CancelGuard`. Events from the current and subsequent queries flow to the subscriber until the guard is dropped.

### TUI Usage Pattern

```rust
let (mut sub, _guard) = agent.subscribe();
tokio::pin!(sub);

// Run query without blocking
let query_handle = tokio::spawn(async move {
    agent.query("hello").await
});

// Consume events
while let Some(ev) = sub.next().await {
    // render in TUI
}

query_handle.await??;
```

## Architecture

```
┌──────────────────────────────────────────────────┐
│                  Caller (TUI)                     │
│                                                   │
│  let mut stream = agent.query_stream("hello")    │
│      .await?;                                     │
│  while let Some(ev) = stream.next().await {       │
│      // render incrementally                      │
│  }                                                │
└──────────────┬───────────────────────────────────┘
               │ .next().await
       ┌───────▼────────┐
       │  mpsc channel  │  ← Stream polls this
       └───────▲────────┘
               │
    ┌──────────┴──────────┐
    │   Tokio Task (spawn) │
    │                      │
    │  QueryEngine.submit_ │
    │  message(prompt)     │
    │      │               │
    │      ▼               │
    │  dispatch events     │
    │  to channel          │
    └──────────────────────┘
```

## Internal Components

### `QueryStream`

```rust
pub struct QueryStream {
    receiver: tokio::sync::mpsc::Receiver<AgentEvent>,
    task: JoinHandle<()>,
    abort_flag: Arc<AtomicBool>,
}
```

- `receiver` — pulls events as they're dispatched
- `task` — handle to the spawned engine loop (can be aborted on drop)
- `abort_flag` — signal for `agent.interrupt()`

`Stream` impl uses `try_recv` first to avoid deadlocks on completed streams, then falls back to async `poll_recv`.

### `EventSubscriber` + `CancelGuard`

```rust
pub struct EventSubscriber {
    receiver: tokio::sync::mpsc::Receiver<AgentEvent>,
}

pub struct CancelGuard; // drop to unsubscribe
```

### Event Fan-Out

The `on_event` dispatch wraps the channel sender. Multiple destinations receive the same event:

1. All active `subscribe()` senders (fan-out to a list)
2. The current `query_stream()` receiver
3. The optional user callback from `set_event_callback`

### `query_stream()` Flow

1. Snapshot engine config (model, api_key, base_url, cwd, max_turns)
2. Clone `on_event` to wrap mpsc sender + existing callback
3. Clone abort controller
4. Build engine via existing `get_or_create_engine()` config logic
5. Spawn tokio task: run `submit_message(prompt)` → dispatch `Done` event → complete
6. Return `QueryStream`

### Error Handling

| Scenario | Stream Behavior |
|----------|----------------|
| Normal completion | Yields `Done { result, exit_reason: Completed }` as last event, then `None` |
| `agent.interrupt()` | Yields `Done { result, exit_reason: AbortedStreaming }` as last event, then `None` |
| API error | Yields `Done { result, exit_reason: ModelError }` as last event, then `None` |
| Stream dropped early | Task aborts via `JoinHandle` |
| `subscribe()` dropped | Subscriber stops receiving. Other queries/subscribers unaffected. |

**Key:** `Done` always fires. The caller never needs to distinguish abort from normal completion by checking for missing events — the `exit_reason` field tells the story.

## Files Changed

- `src/agent.rs` — add `query_stream()`, `subscribe()`, `QueryStream`, `EventSubscriber`, `CancelGuard`, fan-out infrastructure
- `src/lib.rs` — re-export `QueryStream`, `EventSubscriber`, `CancelGuard`
- `src/query_engine.rs` — ensure `Done` always fires (even on abort path)
- `README.md` / `READCN.md` — document new API
- `examples/28_query_stream.rs` — runnable TUI pattern example
- `src/tests/stream_interface.rs` — unit tests

## Testing

1. `test_query_stream_delivers_events` — verify stream yields ContentBlockStart, ContentBlockDelta, ContentBlockStop, MessageStop, Done in order
2. `test_query_stream_done_always_fires` — verify Done fires on both normal completion and abort
3. `test_query_stream_interrupt_cancels_task` — verify `interrupt()` aborts the spawned task
4. `test_subscribe_fans_out_events` — verify multiple subscribers receive same events
5. `test_subscribe_cancel_stops_receiving` — verify dropping CancelGuard stops delivery
6. `test_query_stream_composable_with_select` — verify tokio::select! works with stream

## Version

Bump to 0.30.0 (new feature, additive API — but query_stream is a new method, not a breakage of existing surface. Still, it's a significant new capability).

Actually: this is additive (new methods, no removals). Follow semver: minor version bump → 0.30.0.
