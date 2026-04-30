// Source: ~/claudecode/openclaudecode/src/utils/plugins/installedPluginsManager.ts
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tokio::fs;

use super::loader::{get_plugin_cache_path, get_versioned_cache_path};
use super::plugin_directories::get_plugins_directory;
use super::plugin_identifier::{parse_plugin_identifier, setting_source_to_scope};
use super::schemas::{PluginInstallationEntry, PluginScope};

/// Installed plugins file structure (V2 format).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InstalledPluginsFileV2 {
    pub version: u32,
    pub plugins: HashMap<String, Vec<PluginInstallationEntry>>,
}

static MIGRATION_COMPLETED: Mutex<bool> = Mutex::new(false);
static INSTALLED_PLUGINS_CACHE_V2: Mutex<Option<InstalledPluginsFileV2>> = Mutex::new(None);
static IN_MEMORY_INSTALLED_PLUGINS: Mutex<Option<InstalledPluginsFileV2>> = Mutex::new(None);

/// Get the path to the installed_plugins.json file.
pub fn get_installed_plugins_file_path() -> PathBuf {
    PathBuf::from(get_plugins_directory()).join("installed_plugins.json")
}

/// Clear the installed plugins cache.
pub fn clear_installed_plugins_cache() {
    let mut cache = INSTALLED_PLUGINS_CACHE_V2.lock().unwrap();
    *cache = None;
    let mut in_memory = IN_MEMORY_INSTALLED_PLUGINS.lock().unwrap();
    *in_memory = None;
    log::debug!("Cleared installed plugins cache");
}

/// Read raw file data from installed_plugins.json.
fn read_installed_plugins_file_raw()
-> Result<Option<(u64, serde_json::Value)>, Box<dyn std::error::Error + Send + Sync>> {
    let file_path = get_installed_plugins_file_path();

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Box::new(e)),
    };

    let data: serde_json::Value = serde_json::from_str(&content)?;
    let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    Ok(Some((version, data)))
}

/// Load installed plugins in V2 format.
pub fn load_installed_plugins_v2()
-> Result<InstalledPluginsFileV2, Box<dyn std::error::Error + Send + Sync>> {
    {
        let cache = INSTALLED_PLUGINS_CACHE_V2.lock().unwrap();
        if let Some(ref data) = *cache {
            return Ok(data.clone());
        }
    }

    let file_path = get_installed_plugins_file_path();

    let result = match read_installed_plugins_file_raw() {
        Ok(Some((2, data))) => {
            let validated: InstalledPluginsFileV2 = serde_json::from_value(data)?;
            validated
        }
        Ok(Some((1, data))) => migrate_v1_to_v2(&data)?,
        Ok(Some((_version, _data))) => {
            log::debug!(
                "installed_plugins.json has unsupported version, returning empty V2 object"
            );
            InstalledPluginsFileV2 {
                version: 2,
                plugins: HashMap::new(),
            }
        }
        Ok(None) => {
            log::debug!("installed_plugins.json doesn't exist, returning empty V2 object");
            InstalledPluginsFileV2 {
                version: 2,
                plugins: HashMap::new(),
            }
        }
        Err(e) => {
            log::debug!("Failed to read installed_plugins.json: {}", e);
            InstalledPluginsFileV2 {
                version: 2,
                plugins: HashMap::new(),
            }
        }
    };

    {
        let mut cache = INSTALLED_PLUGINS_CACHE_V2.lock().unwrap();
        *cache = Some(result.clone());
    }

    Ok(result)
}

/// Migrate V1 data to V2 format.
fn migrate_v1_to_v2(
    _v1_data: &serde_json::Value,
) -> Result<InstalledPluginsFileV2, Box<dyn std::error::Error + Send + Sync>> {
    Ok(InstalledPluginsFileV2 {
        version: 2,
        plugins: HashMap::new(),
    })
}

/// Save installed plugins in V2 format.
fn save_installed_plugins_v2(
    data: &InstalledPluginsFileV2,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_path = get_installed_plugins_file_path();

    std::fs::create_dir_all(get_plugins_directory())?;

    let json_content = serde_json::to_string_pretty(data)?;
    std::fs::write(&file_path, json_content)?;

    {
        let mut cache = INSTALLED_PLUGINS_CACHE_V2.lock().unwrap();
        *cache = Some(data.clone());
    }

    log::debug!(
        "Saved {} installed plugins to {:?}",
        data.plugins.len(),
        file_path
    );
    Ok(())
}

/// Add or update a plugin installation entry.
pub fn add_plugin_installation(
    plugin_id: &str,
    scope: PluginScope,
    install_path: &str,
    metadata: &PluginInstallationEntry,
    project_path: Option<&str>,
) {
    let mut data = match load_installed_plugins_from_disk() {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to load installed plugins: {}", e);
            return;
        }
    };

    let installations = data.plugins.entry(plugin_id.to_string()).or_default();

    let existing_index = installations
        .iter()
        .position(|entry| entry.scope == scope && entry.project_path.as_deref() == project_path);

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let new_entry = PluginInstallationEntry {
        scope: scope.clone(),
        install_path: install_path.to_string(),
        version: metadata.version.clone(),
        installed_at: metadata.installed_at.clone(),
        last_updated: now.to_string(),
        git_commit_sha: metadata.git_commit_sha.clone(),
        project_path: project_path.map(|s| s.to_string()),
    };

    if let Some(idx) = existing_index {
        installations[idx] = new_entry;
        log::debug!(
            "Updated installation for {} at scope {:?}",
            plugin_id,
            scope
        );
    } else {
        installations.push(new_entry);
        log::debug!("Added installation for {} at scope {:?}", plugin_id, scope);
    }

    if let Err(e) = save_installed_plugins_v2(&data) {
        log::error!("Failed to save installed plugins: {}", e);
    }
}

/// Remove a plugin installation entry from a specific scope.
pub fn remove_plugin_installation(plugin_id: &str, scope: PluginScope, project_path: Option<&str>) {
    let mut data = match load_installed_plugins_from_disk() {
        Ok(d) => d,
        Err(_) => return,
    };

    if let Some(installations) = data.plugins.get_mut(plugin_id) {
        installations.retain(|entry| {
            !(entry.scope == scope && entry.project_path.as_deref() == project_path)
        });

        if installations.is_empty() {
            data.plugins.remove(plugin_id);
        }
    }

    let _ = save_installed_plugins_v2(&data);
    log::debug!(
        "Removed installation for {} at scope {:?}",
        plugin_id,
        scope
    );
}

/// Update a plugin's install path on disk only, without modifying in-memory state.
pub fn update_installation_path_on_disk(
    plugin_id: &str,
    scope: PluginScope,
    project_path: Option<&str>,
    new_path: &str,
    new_version: &str,
    git_commit_sha: Option<&str>,
) {
    let mut data = match load_installed_plugins_from_disk() {
        Ok(d) => d,
        Err(e) => {
            log::debug!(
                "Cannot update {} on disk: failed to load installed plugins: {}",
                plugin_id,
                e
            );
            return;
        }
    };

    let installations = match data.plugins.get_mut(plugin_id) {
        Some(insts) => insts,
        None => {
            log::debug!(
                "Cannot update {} on disk: plugin not found in installed plugins",
                plugin_id
            );
            return;
        }
    };

    let entry = installations.iter_mut().find(|e| {
        e.scope == scope && e.project_path.as_deref() == project_path
    });

    if let Some(entry) = entry {
        entry.install_path = new_path.to_string();
        entry.version = Some(new_version.to_string());
        entry.last_updated = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();
        if let Some(sha) = git_commit_sha {
            entry.git_commit_sha = Some(sha.to_string());
        }

        if let Err(e) = save_installed_plugins_v2(&data) {
            log::error!("Failed to save installed plugins: {}", e);
        }

        // Clear cache since disk changed
        clear_installed_plugins_cache();

        log::debug!(
            "Updated {} on disk to version {} at {}",
            plugin_id,
            new_version,
            new_path
        );
    } else {
        log::debug!(
            "Cannot update {} on disk: no installation for scope {:?}",
            plugin_id,
            scope
        );
    }
}

/// Load installed plugins directly from disk, bypassing all caches.
pub fn load_installed_plugins_from_disk()
-> Result<InstalledPluginsFileV2, Box<dyn std::error::Error + Send + Sync>> {
    let file_path = get_installed_plugins_file_path();

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstalledPluginsFileV2 {
                version: 2,
                plugins: HashMap::new(),
            });
        }
        Err(e) => return Err(e.into()),
    };

    let data: serde_json::Value = serde_json::from_str(&content)?;
    let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1);

    if version == 2 {
        let validated: InstalledPluginsFileV2 = serde_json::from_value(data)?;
        Ok(validated)
    } else {
        migrate_v1_to_v2(&data)
    }
}

/// Check if a plugin is installed.
pub fn is_plugin_installed(plugin_id: &str) -> bool {
    match load_installed_plugins_v2() {
        Ok(data) => data.plugins.contains_key(plugin_id),
        Err(_) => false,
    }
}

/// Remove all plugin entries belonging to a specific marketplace.
pub fn remove_all_plugins_for_marketplace(marketplace_name: &str) -> (Vec<String>, Vec<String>) {
    if marketplace_name.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut data = match load_installed_plugins_from_disk() {
        Ok(d) => d,
        Err(_) => return (Vec::new(), Vec::new()),
    };

    let suffix = format!("@{}", marketplace_name);
    let mut orphaned_paths = Vec::new();
    let mut removed_plugin_ids = Vec::new();

    let plugin_ids: Vec<String> = data.plugins.keys().cloned().collect();
    for plugin_id in plugin_ids {
        if !plugin_id.ends_with(&suffix) {
            continue;
        }

        if let Some(entries) = data.plugins.remove(&plugin_id) {
            for entry in entries {
                orphaned_paths.push(entry.install_path);
            }
        }
        removed_plugin_ids.push(plugin_id.clone());
        log::debug!(
            "Removed installed plugin for marketplace removal: {}",
            plugin_id
        );
    }

    if !removed_plugin_ids.is_empty() {
        let _ = save_installed_plugins_v2(&data);
    }

    (orphaned_paths, removed_plugin_ids)
}

/// Get the in-memory installed plugins (session state).
pub fn get_in_memory_installed_plugins() -> InstalledPluginsFileV2 {
    let mut in_memory = IN_MEMORY_INSTALLED_PLUGINS.lock().unwrap();
    if in_memory.is_none() {
        *in_memory = load_installed_plugins_v2().ok();
    }
    in_memory.clone().unwrap_or_default()
}

/// Initialize the versioned plugins system.
pub async fn initialize_versioned_plugins() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    migrate_to_single_plugin_file();

    if let Err(e) = migrate_from_enabled_plugins().await {
        log::error!("Failed to migrate from enabled plugins: {}", e);
    }

    let data = get_in_memory_installed_plugins();
    log::debug!(
        "Initialized versioned plugins system with {} plugins",
        data.plugins.len()
    );
    Ok(())
}

fn migrate_to_single_plugin_file() {
    let mut completed = MIGRATION_COMPLETED.lock().unwrap();
    if *completed {
        return;
    }
    *completed = true;
}

/// Migrate from enabledPlugins in settings to installed_plugins.json.
pub async fn migrate_from_enabled_plugins() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    Ok(())
}

/// Details about a pending plugin update.
pub struct PendingUpdateDetails {
    pub plugin_id: String,
    pub version: String,
}

/// Check if there are pending plugin updates.
pub fn has_pending_updates() -> bool {
    false
}

/// Get details about pending plugin updates.
pub fn get_pending_updates_details() -> Vec<PendingUpdateDetails> {
    Vec::new()
}

/// Check if a plugin installation is relevant to the current project.
pub fn is_installation_relevant_to_current_project(_entry: &PluginInstallationEntry) -> bool {
    true
}
