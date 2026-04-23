// Source: /data/home/swei/claudecode/openclaudecode/src/commands/session/session.tsx
use crate::constants::env::system;
use crate::types::Message;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub cwd: String,
    pub model: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "messageCount")]
    pub message_count: u32,
    pub summary: Option<String>,
    pub tag: Option<String>,
}

/// Session data on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub metadata: SessionMetadata,
    pub messages: Vec<Message>,
}

/// Get the sessions directory path.
pub fn get_sessions_dir() -> PathBuf {
    let home = std::env::var(system::HOME)
        .or_else(|_| std::env::var(system::USERPROFILE))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".open-agent-sdk").join("sessions")
}

/// Get the path for a specific session.
pub fn get_session_path(session_id: &str) -> PathBuf {
    get_sessions_dir().join(session_id)
}

/// Save session to disk.
pub async fn save_session(
    session_id: &str,
    messages: Vec<Message>,
    metadata: Option<SessionMetadata>,
) -> Result<(), crate::error::AgentError> {
    let dir = get_session_path(session_id);
    fs::create_dir_all(&dir)
        .await
        .map_err(crate::error::AgentError::Io)?;

    let cwd = metadata
        .as_ref()
        .and_then(|m| Some(m.cwd.clone()))
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let model = metadata
        .as_ref()
        .and_then(|m| Some(m.model.clone()))
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    let created_at = metadata
        .as_ref()
        .and_then(|m| Some(m.created_at.clone()))
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let summary = metadata.as_ref().and_then(|m| m.summary.clone());
    let tag = metadata.as_ref().and_then(|m| m.tag.clone());

    let data = SessionData {
        metadata: SessionMetadata {
            id: session_id.to_string(),
            cwd,
            model,
            created_at: created_at.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: messages.len() as u32,
            summary,
            tag,
        },
        messages,
    };

    let path = dir.join("transcript.json");
    let json = serde_json::to_string_pretty(&data).map_err(crate::error::AgentError::Json)?;
    fs::write(&path, json)
        .await
        .map_err(crate::error::AgentError::Io)?;

    Ok(())
}

/// Load session from disk.
pub async fn load_session(
    session_id: &str,
) -> Result<Option<SessionData>, crate::error::AgentError> {
    let path = get_session_path(session_id).join("transcript.json");

    match fs::read_to_string(&path).await {
        Ok(content) => {
            let data: SessionData =
                serde_json::from_str(&content).map_err(crate::error::AgentError::Json)?;
            Ok(Some(data))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::error::AgentError::Io(e)),
    }
}

/// List all sessions.
pub async fn list_sessions() -> Result<Vec<SessionMetadata>, crate::error::AgentError> {
    let dir = get_sessions_dir();

    let mut entries = match fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(crate::error::AgentError::Io(e)),
    };

    let mut sessions = Vec::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(crate::error::AgentError::Io)?
    {
        let entry_id = entry.file_name().to_string_lossy().to_string();
        if let Ok(Some(data)) = load_session(&entry_id).await {
            if let Some(metadata) = Some(data.metadata) {
                sessions.push(metadata);
            }
        }
    }

    // Sort by updatedAt descending
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// Fork a session (create a copy with a new ID).
pub async fn fork_session(
    source_session_id: &str,
    new_session_id: Option<String>,
) -> Result<Option<String>, crate::error::AgentError> {
    let data = match load_session(source_session_id).await? {
        Some(d) => d,
        None => return Ok(None),
    };

    let fork_id = new_session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    save_session(
        &fork_id,
        data.messages,
        Some(SessionMetadata {
            id: fork_id.clone(),
            cwd: data.metadata.cwd,
            model: data.metadata.model,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: data.metadata.message_count,
            summary: Some(format!("Forked from session {}", source_session_id)),
            tag: None,
        }),
    )
    .await?;

    Ok(Some(fork_id))
}

/// Get session messages.
pub async fn get_session_messages(
    session_id: &str,
) -> Result<Vec<Message>, crate::error::AgentError> {
    match load_session(session_id).await? {
        Some(data) => Ok(data.messages),
        None => Ok(vec![]),
    }
}

/// Append a message to a session transcript.
pub async fn append_to_session(
    session_id: &str,
    message: Message,
) -> Result<(), crate::error::AgentError> {
    let mut data = match load_session(session_id).await? {
        Some(d) => d,
        None => return Ok(()),
    };

    data.messages.push(message);
    data.metadata.updated_at = chrono::Utc::now().to_rfc3339();
    data.metadata.message_count = data.messages.len() as u32;

    save_session(session_id, data.messages, Some(data.metadata)).await
}

/// Delete a session.
pub async fn delete_session(session_id: &str) -> Result<bool, crate::error::AgentError> {
    let path = get_session_path(session_id);

    match fs::remove_dir_all(&path).await {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(crate::error::AgentError::Io(e)),
    }
}

/// Get info about a specific session.
pub async fn get_session_info(
    session_id: &str,
) -> Result<Option<SessionMetadata>, crate::error::AgentError> {
    match load_session(session_id).await? {
        Some(data) => Ok(Some(data.metadata)),
        None => Ok(None),
    }
}

/// Rename a session.
pub async fn rename_session(session_id: &str, title: &str) -> Result<(), crate::error::AgentError> {
    let mut data = match load_session(session_id).await? {
        Some(d) => d,
        None => return Ok(()),
    };

    data.metadata.summary = Some(title.to_string());
    data.metadata.updated_at = chrono::Utc::now().to_rfc3339();

    save_session(session_id, data.messages, Some(data.metadata)).await
}

/// Tag a session.
pub async fn tag_session(
    session_id: &str,
    tag: Option<&str>,
) -> Result<(), crate::error::AgentError> {
    let mut data = match load_session(session_id).await? {
        Some(d) => d,
        None => return Ok(()),
    };

    data.metadata.tag = tag.map(|s| s.to_string());
    data.metadata.updated_at = chrono::Utc::now().to_rfc3339();

    save_session(session_id, data.messages, Some(data.metadata)).await
}

/// Configuration for session resume.
#[derive(Debug, Clone, Default)]
pub struct ResumeConfig {
    /// Maximum number of tail messages to load (default: all messages)
    pub max_tail_messages: Option<usize>,
    /// Session ID to resume from. Messages after this point will be loaded.
    /// When None, loads the full session.
    pub tail_uuid: Option<String>,
}

/// Result of resuming a session.
#[derive(Debug, Clone)]
pub struct ResumeResult {
    /// Messages to inject into the QueryEngine (deduplicated, tail segment)
    pub messages: Vec<Message>,
    /// Session metadata (model, cwd, etc.)
    pub metadata: Option<SessionMetadata>,
    /// Number of messages dropped (deduplicated or outside tail window)
    pub dropped_count: usize,
}

/// Resume a session by loading its messages from disk.
///
/// This implements the core resume logic:
/// 1. Load session from disk
/// 2. Apply tail segment (load only messages after tail_uuid)
/// 3. Deduplicate messages by UUID/content
/// 4. Return messages ready to set on QueryEngine
///
/// Matches TypeScript's resume flow: load transcript → preserved segment → dedup → continue.
pub async fn resume_session(
    session_id: &str,
    config: &ResumeConfig,
) -> Result<ResumeResult, crate::error::AgentError> {
    let data = match load_session(session_id).await? {
        Some(d) => d,
        None => {
            return Ok(ResumeResult {
                messages: vec![],
                metadata: None,
                dropped_count: 0,
            })
        }
    };

    let mut messages = data.messages;
    let mut dropped = 0;

    // Apply tail segment: skip messages before tail_uuid
    if let Some(ref tail_uuid) = config.tail_uuid {
        // Find the index of the message matching tail_uuid, take everything after
        if let Some(idx) = messages.iter().position(|m| is_message_uuid(m, tail_uuid)) {
            let after_tail = messages.drain(idx + 1..).collect::<Vec<_>>();
            dropped += messages.len();
            messages = after_tail;
        }
        // tail_uuid not found — keep all messages (fallback)
    }

    // Apply tail limit: keep only the last N messages
    if let Some(max_tail) = config.max_tail_messages {
        if messages.len() > max_tail {
            let dropped_tail = messages.len() - max_tail;
            messages.drain(..dropped_tail);
            dropped += dropped_tail;
        }
    }

    // Deduplicate messages by content
    let before_dedup = messages.len();
    messages = deduplicate_messages(messages);
    dropped += before_dedup - messages.len();

    Ok(ResumeResult {
        messages,
        metadata: Some(data.metadata),
        dropped_count: dropped,
    })
}

/// Check if a message matches a UUID (for tail segment loading).
/// Since our simplified Message type doesn't have a UUID field,
/// we match by tool_call_id or content hash.
fn is_message_uuid(msg: &Message, uuid: &str) -> bool {
    // Match by tool_call_id if present
    if let Some(ref tool_call_id) = msg.tool_call_id {
        if tool_call_id == uuid {
            return true;
        }
    }
    // Fallback: match by content hash (for messages without tool_call_id)
    let content_hash = format!("{:x}", md5_hash(&msg.content));
    content_hash == uuid
}

/// Simple hash for content matching.
fn md5_hash(content: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in content.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

/// Deduplicate messages by content.
/// Keeps the first occurrence of each unique message.
fn deduplicate_messages(messages: Vec<Message>) -> Vec<Message> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        let key = (msg.role.clone(), msg.content.clone());
        if seen.insert(key) {
            result.push(msg);
        }
    }
    result
}

/// Create a preserved segment from the last N messages.
///
/// Preserved segments are kept during compaction to maintain context.
/// This mirrors the TypeScript `preservedSegment` used in `getAppStateForCompact`.
pub fn create_preserved_segment(
    messages: &[Message],
    max_tokens: u32,
    tail_count: usize,
) -> Vec<Message> {
    let tail = &messages[messages.len().saturating_sub(tail_count)..];
    let mut tokens = 0;
    let mut result = Vec::new();

    for msg in tail.iter().rev() {
        let msg_tokens = crate::compact::rough_token_count_estimation_for_content(&msg.content);
        if tokens + msg_tokens > max_tokens as usize {
            break;
        }
        tokens += msg_tokens;
        result.push(msg.clone());
    }

    // Reverse to maintain chronological order
    result.reverse();
    result
}

// --------------------------------------------------------------------------
// NDJSON Streaming Session Writes
// Source: ~/claudecode/openclaudecode/src/utils/sessionStorage.ts
//
// Replaces monolithic transcript.json with incremental .jsonl writes.
// Each session entry (message, metadata) is serialized as one NDJSON line
// and appended to {session_id}.jsonl. A global write queue with 100ms drain
// timer batches writes for efficiency.
// --------------------------------------------------------------------------

use crate::cli_ndjson_safe_stringify::serialize_to_ndjson;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use tokio::io::AsyncWriteExt;
use std::sync::LazyLock;
use tokio::time;

/// One line in the NDJSON transcript file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "t")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "d")]
    pub data: Option<serde_json::Value>,
}

impl SessionEntry {
    pub fn message(message: &Message) -> Self {
        Self {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("message".to_string()),
            data: Some(serde_json::to_value(message).unwrap_or(serde_json::Value::Null)),
        }
    }

    pub fn metadata(metadata: &SessionMetadata) -> Self {
        Self {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            entry_type: Some("metadata".to_string()),
            data: Some(
                serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null),
            ),
        }
    }
}

/// Get the .jsonl path for a session's NDJSON transcript.
pub fn get_jsonl_path(session_id: &str) -> PathBuf {
    get_session_path(session_id).join(format!("{session_id}.jsonl"))
}

/// Append one NDJSON session entry to the transcript file.
///
/// Creates the session directory if needed, opens the file in append mode,
/// and writes one NDJSON-safe line. This is O(1) per message.
pub async fn append_session_entry(
    session_id: &str,
    entry: &SessionEntry,
) -> Result<(), crate::error::AgentError> {
    let path = get_jsonl_path(session_id);
    fs::create_dir_all(path.parent().unwrap())
        .await
        .map_err(crate::error::AgentError::Io)?;

    let line = serialize_to_ndjson(entry).map_err(crate::error::AgentError::Json)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(crate::error::AgentError::Io)?;
    file.write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(crate::error::AgentError::Io)?;
    Ok(())
}

/// Append a single message to the session as an NDJSON entry.
///
/// Convenience wrapper around `append_session_entry`.
pub async fn append_session_message(
    session_id: &str,
    message: &Message,
) -> Result<(), crate::error::AgentError> {
    let entry = SessionEntry::message(message);
    append_session_entry(session_id, &entry).await
}

/// Load a session from its NDJSON transcript file.
///
/// Reads all lines, parses each as a SessionEntry, and reconstructs
/// the SessionData from message entries.
pub async fn load_session_jsonl(
    session_id: &str,
) -> Result<Option<SessionData>, crate::error::AgentError> {
    let path = get_jsonl_path(session_id);
    match fs::read_to_string(&path).await {
        Ok(content) => {
            let mut messages = Vec::new();
            let mut metadata: Option<SessionMetadata> = None;

            for line in content.lines() {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let entry: SessionEntry =
                    serde_json::from_str(&line).map_err(crate::error::AgentError::Json)?;
                if entry.entry_type.as_deref() == Some("message") {
                    if let Some(data) = &entry.data {
                        let msg: Message =
                            serde_json::from_value(data.clone()).map_err(crate::error::AgentError::Json)?;
                        messages.push(msg);
                    }
                } else if entry.entry_type.as_deref() == Some("metadata") {
                    if let Some(data) = &entry.data {
                        metadata =
                            Some(serde_json::from_value(data.clone()).map_err(crate::error::AgentError::Json)?);
                    }
                }
            }

            if messages.is_empty() && metadata.is_none() {
                return Ok(None);
            }

            let final_metadata = metadata.unwrap_or_else(|| SessionMetadata {
                id: session_id.to_string(),
                cwd: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                model: "claude-sonnet-4-6".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                message_count: messages.len() as u32,
                summary: None,
                tag: None,
            });

            Ok(Some(SessionData {
                metadata: final_metadata,
                messages,
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::error::AgentError::Io(e)),
    }
}

/// Per-session pending NDJSON lines (global static).
static SESSION_PENDING: LazyLock<std::sync::Mutex<HashMap<String, Vec<String>>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Whether background drain task is running.
static SESSION_DRAINING: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(false));

/// Drain interval in milliseconds (matches TS FLUSH_INTERVAL_MS = 100).
const SESSION_FLUSH_INTERVAL_MS: u64 = 100;

pub struct SessionWriter;

impl SessionWriter {
    /// Enqueue an NDJSON line for a session. Starts a background drain
    /// task if one isn't already running.
    pub fn enqueue(session_id: &str, line: String) {
        {
            let mut pending = SESSION_PENDING.lock().unwrap();
            pending
                .entry(session_id.to_string())
                .or_default()
                .push(line);
        }

        // Start background drain if not already running
        if SESSION_DRAINING
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            return;
        }
        tokio::spawn(Self::drain_loop());
    }

    /// Background drain loop: flushes all pending writes, then sleeps.
    /// Exits when all queues are empty.
    async fn drain_loop() {
        loop {
            time::sleep(time::Duration::from_millis(SESSION_FLUSH_INTERVAL_MS)).await;
            if Self::drain().await {
                SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);
                break;
            }
        }
    }

    /// Drain all pending writes to disk. Returns true if all queues are now empty.
    pub async fn drain() -> bool {
        let to_drain = {
            let mut pending = SESSION_PENDING.lock().unwrap();
            let mut batch = HashMap::new();
            for (session_id, lines) in pending.iter_mut() {
                if !lines.is_empty() {
                    batch.insert(session_id.clone(), lines.clone());
                    lines.clear();
                }
            }
            batch
        };

        let mut futures_vec = Vec::new();
        for (session_id, lines) in to_drain {
            let path = get_jsonl_path(&session_id);
            let content: String = lines.join("\n");
            futures_vec.push(async move {
                let _ = fs::create_dir_all(path.parent().unwrap()).await;
                let mut file = match fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .await
                {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let _ = file.write_all(format!("{content}\n").as_bytes()).await;
            });
        }

        futures_util::future::join_all(futures_vec).await;
        SESSION_PENDING.lock().unwrap().is_empty()
    }

    /// Flush a specific session's pending writes immediately.
    pub async fn flush(_session_id: &str) {
        Self::drain().await;
    }
}

/// Enqueue a message for NDJSON streaming write.
///
/// This is the primary way to persist session messages incrementally.
/// The global write queue will drain to disk every 100ms.
pub fn enqueue_session_message(session_id: &str, message: &Message) {
    let line = serialize_to_ndjson(&SessionEntry::message(message))
        .unwrap_or_default();
    SessionWriter::enqueue(session_id, line);
}

/// Enqueue metadata for NDJSON streaming write.
pub fn enqueue_session_metadata(session_id: &str, metadata: &SessionMetadata) {
    let line = serialize_to_ndjson(&SessionEntry::metadata(metadata))
        .unwrap_or_default();
    SessionWriter::enqueue(session_id, line);
}

/// Drain all pending session writes. Call on shutdown.
pub async fn drain_all_sessions() {
    loop {
        if SessionWriter::drain().await {
            break;
        }
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    #[test]
    fn test_deduplicate_messages() {
        let messages = vec![
            Message {
                role: crate::types::MessageRole::User,
                content: "hello".to_string(),
                ..Default::default()
            },
            Message {
                role: crate::types::MessageRole::User,
                content: "hello".to_string(),
                ..Default::default()
            },
            Message {
                role: crate::types::MessageRole::Assistant,
                content: "hi back".to_string(),
                ..Default::default()
            },
        ];
        let deduped = deduplicate_messages(messages);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_deduplicate_preserves_order() {
        let messages = vec![
            Message {
                role: crate::types::MessageRole::User,
                content: "first".to_string(),
                ..Default::default()
            },
            Message {
                role: crate::types::MessageRole::Assistant,
                content: "second".to_string(),
                ..Default::default()
            },
            Message {
                role: crate::types::MessageRole::User,
                content: "first".to_string(),
                ..Default::default()
            },
        ];
        let deduped = deduplicate_messages(messages);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].content, "first");
        assert_eq!(deduped[1].content, "second");
    }

    #[tokio::test]
    async fn test_resume_session_not_found() {
        let config = ResumeConfig::default();
        let result = resume_session("nonexistent-id", &config).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.messages.is_empty());
        assert!(r.metadata.is_none());
    }

    #[test]
    fn test_create_preserved_segment() {
        let messages: Vec<Message> = (0..10)
            .map(|i| Message {
                role: crate::types::MessageRole::User,
                content: format!("msg {}", i),
                ..Default::default()
            })
            .collect();
        let segment = create_preserved_segment(&messages, 100, 5);
        assert!(!segment.is_empty());
        assert!(segment.len() <= 5);
        // Messages should be in chronological order
        for i in 1..segment.len() {
            assert!(segment[i].content > segment[i - 1].content);
        }
    }

    #[test]
    fn test_create_preserved_segment_respects_token_budget() {
        let messages: Vec<Message> = (0..100)
            .map(|i| Message {
                role: crate::types::MessageRole::User,
                content: "x".repeat(10_000),
                ..Default::default()
            })
            .collect();
        let segment = create_preserved_segment(&messages, 5_000, 10);
        assert!(segment.len() <= 2);
    }

    #[test]
    fn test_is_message_uuid_matches_tool_call_id() {
        let msg = Message {
            tool_call_id: Some("abc-123".to_string()),
            ..Default::default()
        };
        assert!(is_message_uuid(&msg, "abc-123"));
        assert!(!is_message_uuid(&msg, "other-id"));
    }

    #[test]
    fn test_md5_hash_deterministic() {
        let h1 = md5_hash("hello world");
        let h2 = md5_hash("hello world");
        assert_eq!(h1, h2);
        assert_ne!(h1, md5_hash("different"));
    }
}
mod tests {
    use super::*;
    use crate::types::MessageRole;

    fn create_test_message(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: content.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_get_sessions_dir() {
        let dir = get_sessions_dir();
        assert!(dir.to_string_lossy().contains(".open-agent-sdk"));
    }

    #[tokio::test]
    async fn test_save_and_load_session() {
        let session_id = "test-session-1";
        let messages = vec![create_test_message("Hello")];

        // Save
        save_session(session_id, messages.clone(), None)
            .await
            .unwrap();

        // Load
        let loaded = load_session(session_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().messages.len(), 1);

        // Cleanup
        delete_session(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_load_nonexistent_session() {
        let loaded = load_session("nonexistent-session").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_fork_session() {
        let source_id = "fork-source-test";
        let messages = vec![
            create_test_message("First"),
            Message {
                role: MessageRole::Assistant,
                content: "Response".to_string(),
                ..Default::default()
            },
        ];

        // Save original
        save_session(source_id, messages, None).await.unwrap();

        // Fork
        let fork_id = fork_session(source_id, None).await.unwrap();
        assert!(fork_id.is_some());

        // Verify fork has messages
        let fork_messages = get_session_messages(fork_id.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(fork_messages.len(), 2);

        // Cleanup
        delete_session(source_id).await.unwrap();
        delete_session(fork_id.as_ref().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn test_append_to_session() {
        let session_id = "append-test-session";

        // Create with initial message
        save_session(session_id, vec![create_test_message("Initial")], None)
            .await
            .unwrap();

        // Append
        append_to_session(
            session_id,
            Message {
                role: MessageRole::Assistant,
                content: "Response".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Verify
        let loaded = load_session(session_id).await.unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);

        // Cleanup
        delete_session(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_rename_session() {
        let session_id = "rename-test-session";
        save_session(session_id, vec![create_test_message("Test")], None)
            .await
            .unwrap();

        rename_session(session_id, "My Session").await.unwrap();

        let info = get_session_info(session_id).await.unwrap().unwrap();
        assert_eq!(info.summary, Some("My Session".to_string()));

        // Cleanup
        delete_session(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_tag_session() {
        let session_id = "tag-test-session";
        save_session(session_id, vec![create_test_message("Test")], None)
            .await
            .unwrap();

        tag_session(session_id, Some("important")).await.unwrap();

        let info = get_session_info(session_id).await.unwrap().unwrap();
        assert_eq!(info.tag, Some("important".to_string()));

        // Cleanup
        delete_session(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_session() {
        let session_id = "delete-test-session";
        save_session(session_id, vec![create_test_message("Test")], None)
            .await
            .unwrap();

        let result = delete_session(session_id).await.unwrap();
        assert!(result);

        // Should not exist now
        let loaded = load_session(session_id).await.unwrap();
        assert!(loaded.is_none());
    }
}

#[cfg(test)]
mod ndjson_tests {
    use super::*;

    #[test]
    fn test_session_entry_message() {
        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "hello world".to_string(),
            ..Default::default()
        };
        let entry = SessionEntry::message(&msg);
        assert_eq!(entry.entry_type, Some("message".to_string()));
        assert!(entry.timestamp.is_some());
        assert!(entry.data.is_some());
    }

    #[test]
    fn test_session_entry_metadata() {
        let meta = SessionMetadata {
            id: "test-session".to_string(),
            cwd: "/tmp".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: 5,
            summary: None,
            tag: None,
        };
        let entry = SessionEntry::metadata(&meta);
        assert_eq!(entry.entry_type, Some("metadata".to_string()));
    }

    #[test]
    fn test_session_entry_serializes() {
        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "test message".to_string(),
            ..Default::default()
        };
        let entry = SessionEntry::message(&msg);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        assert!(json.contains("\"t\""));
    }

    #[test]
    fn test_session_entry_serializes_with_unicode() {
        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "test\u{2028}line\u{2029}sep".to_string(),
            ..Default::default()
        };
        let entry = SessionEntry::message(&msg);
        let json = serialize_to_ndjson(&entry).unwrap();
        // Should escape U+2028/U+2029
        assert!(json.contains("\\u2028"));
        assert!(json.contains("\\u2029"));
        // Must be valid JSON
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
    }

    #[test]
    fn test_get_jsonl_path() {
        let path = get_jsonl_path("test-session-123");
        assert!(path.to_string_lossy().contains("test-session-123"));
        assert!(path.extension().map(|e| e == "jsonl").unwrap_or(false));
    }

    #[tokio::test]
    async fn test_append_session_entry() {
        let session_id = format!("ndjson-append-test-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "first message".to_string(),
            ..Default::default()
        };
        let entry = SessionEntry::message(&msg);

        append_session_entry(&session_id, &entry).await.unwrap();

        // Verify file was created
        let path = get_jsonl_path(&session_id);
        assert!(path.exists());

        // Verify content is valid NDJSON
        let content = fs::read_to_string(&path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: SessionEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.entry_type, Some("message".to_string()));

        // Append second message
        let msg2 = Message {
            role: crate::types::MessageRole::Assistant,
            content: "response".to_string(),
            ..Default::default()
        };
        let entry2 = SessionEntry::message(&msg2);
        append_session_entry(&session_id, &entry2).await.unwrap();

        let content = fs::read_to_string(&path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed2: SessionEntry = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(parsed2.entry_type, Some("message".to_string()));

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id)).await;
    }

    #[tokio::test]
    async fn test_load_session_jsonl() {
        let session_id = format!("ndjson-load-test-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());

        // Create session dir and append entries
        let dir = get_session_path(&session_id);
        fs::create_dir_all(&dir).await.unwrap();

        let msg1 = Message {
            role: crate::types::MessageRole::User,
            content: "hello".to_string(),
            ..Default::default()
        };
        let msg2 = Message {
            role: crate::types::MessageRole::Assistant,
            content: "hi there".to_string(),
            ..Default::default()
        };
        append_session_entry(&session_id, &SessionEntry::message(&msg1)).await.unwrap();
        append_session_entry(&session_id, &SessionEntry::message(&msg2)).await.unwrap();

        // Load back
        let data = load_session_jsonl(&session_id).await.unwrap();
        assert!(data.is_some());
        let data = data.unwrap();
        assert_eq!(data.messages.len(), 2);
        assert_eq!(data.messages[0].content, "hello");
        assert_eq!(data.messages[1].content, "hi there");

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id)).await;
    }

    #[tokio::test]
    async fn test_append_session_message() {
        let session_id = format!("ndjson-append-msg-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());

        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "quick test".to_string(),
            ..Default::default()
        };
        append_session_message(&session_id, &msg).await.unwrap();

        let path = get_jsonl_path(&session_id);
        assert!(path.exists());

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id)).await;
    }

    #[tokio::test]
    async fn test_load_empty_jsonl() {
        let session_id = format!("ndjson-empty-test-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let result = load_session_jsonl(&session_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_enqueue_and_drain() {
        // Reset global state for test isolation
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);

        let session_id = format!("ndjson-enqueue-test-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());

        SessionWriter::enqueue(&session_id, "{\"test\":1}".to_string());
        SessionWriter::enqueue(&session_id, "{\"test\":2}".to_string());

        // Drain immediately
        SessionWriter::drain().await;

        // Verify file was created
        let path = get_jsonl_path(&session_id);
        assert!(path.exists());
        let content = fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("{\"test\":1}"));
        assert!(content.contains("{\"test\":2}"));

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id)).await;
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_enqueue_session_message() {
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);

        let session_id = format!("ndjson-enqueue-msg-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());

        let msg = Message {
            role: crate::types::MessageRole::User,
            content: "streaming test".to_string(),
            ..Default::default()
        };
        enqueue_session_message(&session_id, &msg);

        // Force drain
        SessionWriter::drain().await;

        let path = get_jsonl_path(&session_id);
        assert!(path.exists());
        let content = fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("streaming test"));

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id)).await;
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_multiple_sessions_drain() {
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);

        let session_id1 = format!("ndjson-multi-1-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());
        let session_id2 = format!("ndjson-multi-2-{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos());

        SessionWriter::enqueue(&session_id1, "{\"s\":1}".to_string());
        SessionWriter::enqueue(&session_id2, "{\"s\":2}".to_string());
        SessionWriter::enqueue(&session_id1, "{\"s\":3}".to_string());

        SessionWriter::drain().await;

        let content1 = fs::read_to_string(get_jsonl_path(&session_id1)).await.unwrap();
        let content2 = fs::read_to_string(get_jsonl_path(&session_id2)).await.unwrap();

        let lines1: Vec<&str> = content1.lines().collect();
        let lines2: Vec<&str> = content2.lines().collect();
        assert_eq!(lines1.len(), 2);
        assert_eq!(lines2.len(), 1);

        // Cleanup
        let _ = fs::remove_dir_all(get_session_path(&session_id1)).await;
        let _ = fs::remove_dir_all(get_session_path(&session_id2)).await;
        SESSION_PENDING.lock().unwrap().clear();
        SESSION_DRAINING.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
