// Source: ~/claudecode/openclaudecode/src/utils/plugins/pluginAutoupdate.ts
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use super::installed_plugins_manager::{
    get_pending_updates_details, has_pending_updates, is_installation_relevant_to_current_project,
    load_installed_plugins_from_disk,
};
use super::plugin_identifier::parse_plugin_identifier;

/// Callback type for notifying when plugins have been updated.
type PluginAutoUpdateCallback = Box<dyn Fn(Vec<String>) + Send + Sync>;

static PLUGIN_UPDATE_CALLBACK: Lazy<Mutex<Option<PluginAutoUpdateCallback>>> =
    Lazy::new(|| Mutex::new(None));
static PENDING_NOTIFICATION: Lazy<Mutex<Option<Vec<String>>>> = Lazy::new(|| Mutex::new(None));

/// Register a callback to be notified when plugins are auto-updated.
pub fn on_plugins_auto_updated(
    callback: impl Fn(Vec<String>) + Send + Sync + 'static,
) -> Box<dyn FnOnce()> {
    let cb: PluginAutoUpdateCallback = Box::new(callback);

    {
        let mut pending = PENDING_NOTIFICATION.lock().unwrap();
        if let Some(ref updates) = *pending {
            if !updates.is_empty() {
                cb(updates.clone());
                *pending = None;
            }
        }
    }

    {
        let mut callback_lock = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
        *callback_lock = Some(cb);
    }

    Box::new(|| {
        let mut callback_lock = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
        *callback_lock = None;
    })
}

/// Check if pending updates came from autoupdate.
pub fn get_auto_updated_plugin_names() -> Vec<String> {
    if !has_pending_updates() {
        return Vec::new();
    }

    get_pending_updates_details()
        .into_iter()
        .map(|d| parse_plugin_identifier(&d.plugin_id).name)
        .collect()
}

/// Get the set of marketplaces that have autoUpdate enabled.
async fn get_auto_update_enabled_marketplaces() -> HashSet<String> {
    let config = match super::marketplace_manager::load_known_marketplaces_config().await {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Failed to load known marketplaces config for auto-update: {}", e);
            return HashSet::new();
        }
    };
    let declared = super::marketplace_manager::get_declared_marketplaces();
    let mut enabled = HashSet::new();

    for (name, entry) in &config {
        // Settings-declared autoUpdate takes precedence over JSON state
        let declared_auto_update = declared.get(name).and_then(|d| d.auto_update);
        let auto_update = match declared_auto_update {
            Some(v) => v,
            None => super::schemas::is_marketplace_auto_update(name, &serde_json::Value::Null),
        };
        if auto_update {
            enabled.insert(name.to_lowercase());
        }
    }

    enabled
}

/// Update a single plugin's installations.
/// Returns the plugin ID if any installation was updated, null otherwise.
async fn update_plugin(
    plugin_id: &str,
    installations: &[(super::schemas::PluginScope, Option<String>)],
) -> Option<String> {
    let mut was_updated = false;

    for (scope, _project_path) in installations {
        let scope_str = match scope {
            super::schemas::PluginScope::User => "user",
            super::schemas::PluginScope::Managed => "managed",
            super::schemas::PluginScope::Project => "project",
            super::schemas::PluginScope::Local => "local",
        };
        // Call the service-layer update operation
        let result = crate::services::plugins::plugin_operations::update_plugin_op(
            plugin_id,
            scope_str,
        )
        .await;
        if result.success && !result.already_up_to_date.unwrap_or(false) {
            was_updated = true;
            log::debug!(
                "Plugin autoupdate: updated {} from {} to {}",
                plugin_id,
                result.old_version.unwrap_or_default(),
                result.new_version.unwrap_or_default(),
            );
        } else if !result.already_up_to_date.unwrap_or(false) {
            log::debug!(
                "Plugin autoupdate: failed to update {}: {}",
                plugin_id,
                result.message,
            );
        }
    }

    if was_updated {
        Some(plugin_id.to_string())
    } else {
        None
    }
}

/// Update all project-relevant installed plugins from the given marketplaces.
pub async fn update_plugins_for_marketplaces(marketplace_names: &HashSet<String>) -> Vec<String> {
    let installed_plugins = match super::installed_plugins_manager::load_installed_plugins_from_disk() {
        Ok(p) => p,
        Err(e) => {
            log::debug!("Failed to load installed plugins for auto-update: {}", e);
            return Vec::new();
        }
    };

    let plugin_ids: Vec<_> = installed_plugins.plugins.keys().cloned().collect();

    if plugin_ids.is_empty() {
        return Vec::new();
    }

    let mut updated = Vec::new();

    for plugin_id in plugin_ids {
        let parsed = super::plugin_identifier::parse_plugin_identifier(&plugin_id);
        let marketplace = match parsed.marketplace {
            Some(m) => m,
            None => continue,
        };
        if !marketplace_names.contains(marketplace.to_lowercase().as_str()) {
            continue;
        }

        let all_installations = match installed_plugins.plugins.get(&plugin_id) {
            Some(insts) if !insts.is_empty() => insts,
            _ => continue,
        };

        // Filter to installations relevant to current project
        let relevant: Vec<_> = all_installations
            .iter()
            .filter(|e| {
                // User/managed scope always relevant; project/local must match cwd
                matches!(
                    e.scope,
                    super::schemas::PluginScope::User | super::schemas::PluginScope::Managed
                ) || super::installed_plugins_manager::is_installation_relevant_to_current_project(e)
            })
            .collect();
        if relevant.is_empty() {
            continue;
        }

        let scope_project: Vec<_> = relevant
            .iter()
            .map(|e| (e.scope.clone(), e.project_path.clone()))
            .collect();

        if let Some(updated_id) = update_plugin(&plugin_id, &scope_project).await {
            updated.push(updated_id);
        }
    }

    updated
}

/// Auto-update marketplaces and plugins in the background.
pub fn auto_update_marketplaces_and_plugins_in_background() {
    tokio::spawn(async move {
        if let Err(e) = auto_update_inner().await {
            log::error!("Plugin autoupdate error: {}", e);
        }
    });
}

async fn auto_update_inner() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let auto_update_enabled = get_auto_update_enabled_marketplaces().await;
    if auto_update_enabled.is_empty() {
        return Ok(());
    }

    // Refresh marketplaces with autoUpdate enabled
    for name in &auto_update_enabled {
        if let Err(e) = super::marketplace_manager::refresh_marketplace(name, None::<fn(&str)>, None).await {
            log::debug!(
                "Plugin autoupdate: failed to refresh marketplace {}: {}",
                name,
                e,
            );
        }
    }

    log::debug!("Plugin autoupdate: checking installed plugins");
    let updated_plugins = update_plugins_for_marketplaces(&auto_update_enabled).await;

    if !updated_plugins.is_empty() {
        // Notify callback if registered
        let mut callback_lock = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
        if let Some(ref callback) = *callback_lock {
            callback(updated_plugins.clone());
        } else {
            // Store for later delivery
            *PENDING_NOTIFICATION.lock().unwrap() = Some(updated_plugins);
        }
    }

    Ok(())
}
