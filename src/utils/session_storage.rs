// Source: /data/home/swei/claudecode/openclaudecode/src/utils/sessionStorage.ts
//! Session storage utilities - file-based session persistence

use crate::constants::env::system;
use crate::session::SessionData;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Get the session storage directory
pub fn get_session_storage_dir() -> PathBuf {
    let home = std::env::var(system::HOME)
        .or_else(|_| std::env::var(system::USERPROFILE))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".open-agent-sdk")
        .join("session_storage")
}

/// Get the transcript file path for a session
pub fn get_transcript_path(session_id: &str) -> PathBuf {
    get_session_storage_dir()
        .join(session_id)
        .join("transcript.json")
}

/// Get the session state file path for a session
pub fn get_session_state_path(session_id: &str) -> PathBuf {
    get_session_storage_dir()
        .join(session_id)
        .join("state.json")
}

/// Ensure the session storage directory exists
fn ensure_storage_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(get_session_storage_dir())
}

/// Ensure a session-specific directory exists
fn ensure_session_dir(session_id: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(get_session_storage_dir().join(session_id))
}

/// Check if a session's transcript file exists
pub fn session_exists(session_id: &str) -> bool {
    get_transcript_path(session_id).exists()
}

/// Internal transcript entry stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Load transcript for a session from disk
pub fn load_transcript(session_id: &str) -> Vec<String> {
    let path = get_transcript_path(session_id);
    if !path.exists() {
        return vec![];
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<Vec<TranscriptEntry>>(&content) {
                Ok(entries) => entries.into_iter().map(|e| e.content).collect(),
                Err(_) => {
                    // Try parsing as Vec<String> for backward compatibility
                    serde_json::from_str::<Vec<String>>(&content).unwrap_or_default()
                }
            }
        }
        Err(_) => vec![],
    }
}

/// Load full session data including transcript entries with metadata
pub fn load_transcript_with_metadata(session_id: &str) -> Result<Vec<TranscriptEntry>, String> {
    let path = get_transcript_path(session_id);
    if !path.exists() {
        return Err(format!("Transcript not found for session: {}", session_id));
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read transcript: {}", e))?;

    serde_json::from_str::<Vec<TranscriptEntry>>(&content)
        .map_err(|e| format!("Failed to parse transcript: {}", e))
}

/// Save transcript for a session to disk
pub fn save_transcript(session_id: &str, transcript: &[String]) -> Result<(), String> {
    ensure_session_dir(session_id).map_err(|e| format!("Failed to create session dir: {}", e))?;

    let entries: Vec<TranscriptEntry> = transcript
        .iter()
        .map(|content| TranscriptEntry {
            role: "assistant".to_string(),
            content: content.clone(),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|e| format!("Failed to serialize transcript: {}", e))?;

    let path = get_transcript_path(session_id);
    std::fs::write(&path, json).map_err(|e| format!("Failed to write transcript: {}", e))?;

    Ok(())
}

/// Append a message to an existing transcript
pub fn append_to_transcript(session_id: &str, role: &str, content: &str) -> Result<(), String> {
    let path = get_transcript_path(session_id);

    let mut entries = if path.exists() {
        let existing = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read existing transcript: {}", e))?;
        serde_json::from_str::<Vec<TranscriptEntry>>(&existing)
            .map_err(|e| format!("Failed to parse existing transcript: {}", e))?
    } else {
        ensure_session_dir(session_id)
            .map_err(|e| format!("Failed to create session dir: {}", e))?;
        vec![]
    };

    entries.push(TranscriptEntry {
        role: role.to_string(),
        content: content.to_string(),
        timestamp: Some(chrono::Utc::now().to_rfc3339()),
    });

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|e| format!("Failed to serialize transcript: {}", e))?;

    std::fs::write(&path, json).map_err(|e| format!("Failed to write transcript: {}", e))?;

    Ok(())
}

/// Delete a session's stored data
pub fn delete_session_storage(session_id: &str) -> Result<(), String> {
    let session_dir = get_session_storage_dir().join(session_id);
    if session_dir.exists() {
        std::fs::remove_dir_all(&session_dir)
            .map_err(|e| format!("Failed to delete session storage: {}", e))?;
    }
    Ok(())
}

/// Flush all pending session storage writes to disk.
/// Forces fsync on all session files to prevent data loss before error/success results.
/// Matches TypeScript's flushSessionStorage() which calls getProject().flush().
///
/// In Rust, std::fs::write is synchronous but data may still be in OS page cache.
/// This function ensures durability by calling fsync via File::sync_all().
pub fn flush_session_storage() -> Result<(), String> {
    let session_dir = get_session_storage_dir();
    if !session_dir.exists() {
        return Ok(()); // Nothing to flush
    }

    for entry in std::fs::read_dir(&session_dir)
        .map_err(|e| format!("Failed to read session storage dir: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            // Flush transcript file
            let transcript = entry.path().join("transcript.json");
            if transcript.exists() {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&transcript)
                    .map_err(|e| format!("Failed to open {}: {}", transcript.display(), e))?;
                let _ = f.sync_all(); // Best effort - don't fail on sync error
            }
            // Flush state file
            let state = entry.path().join("state.json");
            if state.exists() {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&state)
                    .map_err(|e| format!("Failed to open {}: {}", state.display(), e))?;
                let _ = f.sync_all();
            }
        }
    }
    Ok(())
}

/// List all stored session IDs
pub fn list_stored_sessions() -> Vec<String> {
    let dir = get_session_storage_dir();
    if !dir.exists() {
        return vec![];
    }

    let mut sessions = vec![];
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let transcript_path = entry.path().join("transcript.json");
                    if transcript_path.exists() {
                        sessions.push(name.to_string());
                    }
                }
            }
        }
    }
    sessions
}

/// Get the size of stored transcript in bytes
pub fn get_transcript_size(session_id: &str) -> u64 {
    let path = get_transcript_path(session_id);
    if !path.exists() {
        return 0;
    }
    std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
}

/// Check if session data is corrupted by attempting to parse it
pub fn is_session_data_valid(session_id: &str) -> bool {
    let path = get_transcript_path(session_id);
    if !path.exists() {
        return false;
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            serde_json::from_str::<Vec<TranscriptEntry>>(&content).is_ok()
                || serde_json::from_str::<Vec<String>>(&content).is_ok()
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_transcript_path() {
        let path = get_transcript_path("test-session-123");
        assert!(path.to_string_lossy().contains("test-session-123"));
        assert!(path.to_string_lossy().contains("transcript.json"));
    }

    #[test]
    fn test_get_session_state_path() {
        let path = get_session_state_path("test-session-456");
        assert!(path.to_string_lossy().contains("test-session-456"));
        assert!(path.to_string_lossy().contains("state.json"));
    }

    #[test]
    fn test_session_not_exists() {
        assert!(!session_exists("nonexistent-session-xyz"));
    }

    #[test]
    fn test_list_stored_sessions_empty() {
        let sessions = list_stored_sessions();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_load_transcript_nonexistent() {
        let result = load_transcript("nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_transcript_size_nonexistent() {
        let size = get_transcript_size("nonexistent");
        assert_eq!(size, 0);
    }

    #[test]
    fn test_is_session_data_valid_nonexistent() {
        assert!(!is_session_data_valid("nonexistent"));
    }
}
