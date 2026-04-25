// Source: ~/claudecode/openclaudecode/src/tools/AgentTool/agentMemorySnapshot.ts
#![allow(dead_code)]

use std::path::PathBuf;
use tokio::fs;

use super::agent_memory::{AgentMemoryScope, get_agent_memory_dir};

const SNAPSHOT_BASE: &str = "agent-memory-snapshots";
const SNAPSHOT_JSON: &str = "snapshot.json";
const SYNCED_JSON: &str = ".snapshot-synced.json";

#[derive(Debug, Clone, serde::Deserialize)]
struct SnapshotMeta {
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SyncedMeta {
    #[serde(rename = "syncedFrom")]
    synced_from: String,
}

/// Action to take when checking for snapshot updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotAction {
    None,
    Initialize { snapshot_timestamp: String },
    PromptUpdate { snapshot_timestamp: String },
}

/// Returns the path to the snapshot directory for an agent in the current project.
fn get_snapshot_dir_for_agent(agent_type: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".claude")
        .join(SNAPSHOT_BASE)
        .join(agent_type)
}

fn get_snapshot_json_path(agent_type: &str) -> PathBuf {
    get_snapshot_dir_for_agent(agent_type).join(SNAPSHOT_JSON)
}

fn get_synced_json_path(agent_type: &str, scope: AgentMemoryScope) -> PathBuf {
    get_agent_memory_dir(agent_type, scope).join(SYNCED_JSON)
}

async fn read_json_file<T>(path: &PathBuf) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    fs::read_to_string(path)
        .await
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

/// Check if a snapshot exists and whether it's newer than what we last synced.
pub async fn check_agent_memory_snapshot(agent_type: &str, scope: AgentMemoryScope) -> SnapshotAction {
    let snapshot_path = get_snapshot_json_path(agent_type);
    let snapshot_meta: Option<SnapshotMeta> = read_json_file(&snapshot_path).await;

    let Some(snapshot_meta) = snapshot_meta else {
        return SnapshotAction::None;
    };

    let local_mem_dir = get_agent_memory_dir(agent_type, scope);

    // Check if local memory exists (has any .md files)
    let has_local_memory = match fs::read_dir(&local_mem_dir).await {
        Ok(mut entries) => {
            let mut has_md = false;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_type().await.map(|ft| ft.is_file()).unwrap_or(false)
                    && entry.file_name().to_string_lossy().ends_with(".md")
                {
                    has_md = true;
                    break;
                }
            }
            has_md
        }
        Err(_) => false,
    };

    if !has_local_memory {
        return SnapshotAction::Initialize {
            snapshot_timestamp: snapshot_meta.updated_at,
        };
    }

    let synced_path = get_synced_json_path(agent_type, scope);
    let synced_meta: Option<SyncedMeta> = read_json_file(&synced_path).await;

    let snapshot_newer = synced_meta
        .as_ref()
        .map(|s| is_newer_timestamp(&snapshot_meta.updated_at, &s.synced_from))
        .unwrap_or(true);

    if snapshot_newer {
        SnapshotAction::PromptUpdate {
            snapshot_timestamp: snapshot_meta.updated_at,
        }
    } else {
        SnapshotAction::None
    }
}

/// Initialize local agent memory from a snapshot (first-time setup).
pub async fn initialize_from_snapshot(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) -> std::io::Result<()> {
    log::debug!(
        "Initializing agent memory for {} from project snapshot",
        agent_type
    );
    copy_snapshot_to_local(agent_type, scope).await?;
    save_synced_meta(agent_type, scope, snapshot_timestamp).await?;
    Ok(())
}

/// Replace local agent memory with the snapshot.
pub async fn replace_from_snapshot(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) -> std::io::Result<()> {
    log::debug!(
        "Replacing agent memory for {} with project snapshot",
        agent_type
    );
    // Remove existing .md files before copying to avoid orphans
    let local_mem_dir = get_agent_memory_dir(agent_type, scope);
    if let Ok(mut entries) = fs::read_dir(&local_mem_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if entry.file_type().await.map(|ft| ft.is_file()).unwrap_or(false)
                && entry.file_name().to_string_lossy().ends_with(".md")
            {
                let _ = fs::remove_file(&path).await;
            }
        }
    }
    copy_snapshot_to_local(agent_type, scope).await?;
    save_synced_meta(agent_type, scope, snapshot_timestamp).await?;
    Ok(())
}

/// Mark the current snapshot as synced without changing local memory.
pub async fn mark_snapshot_synced(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) -> std::io::Result<()> {
    save_synced_meta(agent_type, scope, snapshot_timestamp).await
}

async fn copy_snapshot_to_local(agent_type: &str, scope: AgentMemoryScope) -> std::io::Result<()> {
    let snapshot_dir = get_snapshot_dir_for_agent(agent_type);
    let local_dir = get_agent_memory_dir(agent_type, scope);

    fs::create_dir_all(&local_dir).await?;

    // Copy all files except snapshot.json
    if let Ok(mut entries) = fs::read_dir(&snapshot_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_file() && name != SNAPSHOT_JSON {
                let dest = local_dir.join(&name);
                fs::copy(&path, &dest).await?;
            }
        }
    }

    Ok(())
}

async fn save_synced_meta(
    agent_type: &str,
    scope: AgentMemoryScope,
    snapshot_timestamp: &str,
) -> std::io::Result<()> {
    let synced_path = get_synced_json_path(agent_type, scope);
    let local_dir = get_agent_memory_dir(agent_type, scope);
    fs::create_dir_all(&local_dir).await?;

    let meta = serde_json::json!({
        "syncedFrom": snapshot_timestamp,
    });
    fs::write(&synced_path, serde_json::to_string_pretty(&meta)?).await
}

fn is_newer_timestamp(a: &str, b: &str) -> bool {
    // Simple string comparison for ISO timestamps
    a > b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_timestamp() {
        assert!(is_newer_timestamp(
            "2024-01-02T00:00:00Z",
            "2024-01-01T00:00:00Z"
        ));
        assert!(!is_newer_timestamp(
            "2024-01-01T00:00:00Z",
            "2024-01-02T00:00:00Z"
        ));
    }

    #[test]
    fn test_snapshot_action_none_no_snapshot() {
        let action = tokio::runtime::Runtime::new().unwrap().block_on(
            check_agent_memory_snapshot("nonexistent", AgentMemoryScope::Local),
        );
        assert_eq!(action, SnapshotAction::None);
    }
}
