// Source: ~/claudecode/openclaudecode/src/services/plugins/pluginOperations.ts
#![allow(dead_code)]

//! Core plugin operations (install, uninstall, enable, disable, update)
//!
//! This module provides pure library functions that can be used by both:
//! - CLI commands (`claude plugin install/uninstall/enable/disable/update`)
//! - Interactive UI (ManagePlugins.tsx)
//!
//! Functions in this module:
//! - Do NOT call process::exit()
//! - Do NOT write to console
//! - Return result objects indicating success/failure with messages
//! - Can throw errors for unexpected failures

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::utils::plugins::loader::parse_plugin_identifier;
use crate::utils::plugins::types::{PluginMarketplace, PluginMarketplaceEntry, PluginSource};

// ============================================================================
// Constants and Types
// ============================================================================

/// Valid installable scopes (excludes 'managed' which can only be installed from managed-settings.json)
pub const VALID_INSTALLABLE_SCOPES: &[&str] = &["user", "project", "local"];

/// Valid scopes for update operations (includes 'managed' since managed plugins can be updated)
pub const VALID_UPDATE_SCOPES: &[&str] = &["user", "project", "local", "managed"];

/// Installation scope type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallableScope {
    User,
    Project,
    Local,
}

impl std::fmt::Display for InstallableScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
            Self::Local => write!(f, "local"),
        }
    }
}

impl TryFrom<&str> for InstallableScope {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "local" => Ok(Self::Local),
            _ => Err(format!(
                "Invalid scope \"{}\". Must be one of: {}",
                value,
                VALID_INSTALLABLE_SCOPES.join(", ")
            )),
        }
    }
}

/// Plugin scope (broader, includes 'managed')
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginScope {
    User,
    Project,
    Local,
    Managed,
}

impl std::fmt::Display for PluginScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
            Self::Local => write!(f, "local"),
            Self::Managed => write!(f, "managed"),
        }
    }
}

impl TryFrom<&str> for PluginScope {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "local" => Ok(Self::Local),
            "managed" => Ok(Self::Managed),
            _ => Err(format!("Invalid plugin scope: {}", value)),
        }
    }
}

impl From<InstallableScope> for PluginScope {
    fn from(scope: InstallableScope) -> Self {
        match scope {
            InstallableScope::User => Self::User,
            InstallableScope::Project => Self::Project,
            InstallableScope::Local => Self::Local,
        }
    }
}

/// Setting source mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
    FlagSettings,
}

impl std::fmt::Display for SettingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserSettings => write!(f, "userSettings"),
            Self::ProjectSettings => write!(f, "projectSettings"),
            Self::LocalSettings => write!(f, "localSettings"),
            Self::PolicySettings => write!(f, "policySettings"),
            Self::FlagSettings => write!(f, "flagSettings"),
        }
    }
}

/// Convert installable scope to setting source
pub fn scope_to_setting_source(scope: InstallableScope) -> SettingSource {
    match scope {
        InstallableScope::User => SettingSource::UserSettings,
        InstallableScope::Project => SettingSource::ProjectSettings,
        InstallableScope::Local => SettingSource::LocalSettings,
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of a plugin operation
#[derive(Debug, Clone)]
pub struct PluginOperationResult {
    pub success: bool,
    pub message: String,
    pub plugin_id: Option<String>,
    pub plugin_name: Option<String>,
    pub scope: Option<String>,
    /// Plugins that declare this plugin as a dependency (warning on uninstall/disable)
    pub reverse_dependents: Option<Vec<String>>,
}

/// Result of a plugin update operation
#[derive(Debug, Clone)]
pub struct PluginUpdateResult {
    pub success: bool,
    pub message: String,
    pub plugin_id: Option<String>,
    pub new_version: Option<String>,
    pub old_version: Option<String>,
    pub already_up_to_date: Option<bool>,
    pub scope: Option<String>,
}

// ============================================================================
// Installed Plugins Data Structures
// ============================================================================

/// Installation entry in installed_plugins_v2.json
#[derive(Debug, Clone)]
pub struct PluginInstallationEntry {
    pub scope: String,
    pub project_path: Option<String>,
    pub install_path: String,
    pub version: Option<String>,
    pub git_commit_sha: Option<String>,
}

/// Installed plugins data (V2 format)
#[derive(Debug, Clone, Default)]
pub struct InstalledPluginsV2 {
    pub plugins: HashMap<String, Vec<PluginInstallationEntry>>,
}

/// Plugin info from marketplace lookup
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub entry: PluginMarketplaceEntry,
    pub marketplace_install_location: String,
}

/// Resolution result for install
#[derive(Debug)]
pub enum InstallResolutionResult {
    Success { dep_note: String },
    LocalSourceNoLocation { plugin_name: String },
    SettingsWriteFailed { message: String },
    ResolutionFailed { resolution: String },
    BlockedByPolicy { plugin_name: String },
    DependencyBlockedByPolicy {
        plugin_name: String,
        blocked_dependency: String,
    },
}

/// Settings JSON structure
#[derive(Debug, Clone, Default)]
pub struct SettingsJson {
    pub enabled_plugins: Option<BTreeMap<String, serde_json::Value>>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Assert that a scope is a valid installable scope at runtime
pub fn assert_installable_scope(scope: &str) -> Result<InstallableScope, String> {
    InstallableScope::try_from(scope)
}

/// Type guard to check if a scope is an installable scope (not 'managed')
pub fn is_installable_scope(scope: &str) -> bool {
    VALID_INSTALLABLE_SCOPES.contains(&scope)
}

/// Get the project path for scopes that are project-specific.
/// Returns the original cwd for 'project' and 'local' scopes, None otherwise.
pub fn get_project_path_for_scope(scope: &str) -> Option<String> {
    if scope == "project" || scope == "local" {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Pluralize helper
pub(crate) fn plural(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        format!("{}s", singular)
    }
}

/// Check if a plugin is a built-in plugin ID
pub(crate) fn is_builtin_plugin_id(plugin: &str) -> bool {
    // Built-in plugins would be defined here. For now, check against known list.
    const BUILTIN_PLUGINS: &[&str] = &[];
    BUILTIN_PLUGINS.contains(&plugin)
}

// ============================================================================
// Settings Operations (stubs - would integrate with actual settings system)
// ============================================================================

/// Get settings for a specific source
fn get_settings_for_source(_source: SettingSource) -> Option<SettingsJson> {
    // In production, this would load from the actual settings file
    None
}

/// Update settings for a specific source
/// Returns error message if the update failed
fn update_settings_for_source(
    _source: SettingSource,
    _settings: &SettingsJson,
) -> Result<(), String> {
    // In production, this would write to the actual settings file
    Ok(())
}

// ============================================================================
// Plugin Loader Operations (stubs)
// ============================================================================

/// Loaded plugin structure
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub name: String,
    pub source: Option<String>,
    pub manifest: Option<serde_json::Value>,
}

/// Load all plugins (enabled and disabled)
async fn load_all_plugins() -> (Vec<LoadedPlugin>, Vec<LoadedPlugin>) {
    // In production, this would load plugins from disk/cache
    (Vec::new(), Vec::new())
}

/// Load installed plugins from disk
fn load_installed_plugins_from_disk() -> InstalledPluginsV2 {
    // In production, this would load installed_plugins_v2.json
    InstalledPluginsV2::default()
}

/// Load installed plugins V2
fn load_installed_plugins_v2() -> InstalledPluginsV2 {
    load_installed_plugins_from_disk()
}

/// Remove a plugin installation from disk
fn remove_plugin_installation(
    _plugin_id: &str,
    _scope: &str,
    _project_path: Option<&str>,
) {
    // In production, this would update installed_plugins_v2.json
    log::debug!(
        "Removing plugin installation: {} scope={} project_path={:?}",
        _plugin_id,
        _scope,
        _project_path
    );
}

/// Update installation path on disk
fn update_installation_path_on_disk(
    _plugin_id: &str,
    _scope: &str,
    _project_path: Option<&str>,
    _new_path: &str,
    _new_version: &str,
    _git_commit_sha: Option<&str>,
) {
    // In production, this would update installed_plugins_v2.json
    log::debug!(
        "Updating installation path: {} -> {} version={}",
        _plugin_id,
        _new_path,
        _new_version
    );
}

// ============================================================================
// Marketplace Operations (stubs)
// ============================================================================

/// Load known marketplaces config
async fn load_known_marketplaces_config() -> HashMap<String, serde_json::Value> {
    // In production, this would load known_marketplaces.json
    HashMap::new()
}

/// Get a marketplace by name
async fn get_marketplace(name: &str) -> Option<PluginMarketplace> {
    // In production, this would load the marketplace from cache/disk
    log::debug!("Getting marketplace: {}", name);
    None
}

/// Get a plugin by ID from marketplace
async fn get_plugin_by_id(_plugin: &str) -> Option<PluginInfo> {
    // In production, this would search all marketplaces for the plugin
    log::debug!("Getting plugin by id: {}", _plugin);
    None
}

// ============================================================================
// Cache Operations (stubs)
// ============================================================================

/// Clear all caches
fn clear_all_caches() {
    log::debug!("Clearing all caches");
}

/// Clear plugin cache with optional reason
fn clear_plugin_cache(reason: &str) {
    log::debug!("Clearing plugin cache: {}", reason);
}

/// Mark a plugin version as orphaned
async fn mark_plugin_version_orphaned(_install_path: &str) {
    log::debug!("Marking plugin version orphaned: {}", _install_path);
}

/// Cache a plugin (download to temp)
async fn cache_plugin(
    _source: &PluginSource,
    _options: CachePluginOptions,
) -> Result<CachePluginResult, String> {
    Err("cache_plugin not implemented".to_string())
}

/// Cache plugin options
#[derive(Debug, Clone, Default)]
pub struct CachePluginOptions {
    pub manifest: Option<serde_json::Value>,
}

/// Cache plugin result
#[derive(Debug, Clone)]
pub struct CachePluginResult {
    pub path: String,
    pub manifest: serde_json::Value,
    pub git_commit_sha: Option<String>,
}

/// Copy plugin to versioned cache directory
async fn copy_plugin_to_versioned_cache(
    _source_path: &str,
    _plugin_id: &str,
    _new_version: &str,
    _entry: &PluginMarketplaceEntry,
) -> Result<String, String> {
    Err("copy_plugin_to_versioned_cache not implemented".to_string())
}

/// Get versioned cache path for a plugin.
/// Format: ~/.claude/plugins/cache/{marketplace}/{plugin}/{version}/
fn get_versioned_cache_path(plugin_id: &str, version: &str) -> String {
    use crate::utils::plugins::plugin_directories::get_plugins_directory;
    let plugins_dir = get_plugins_directory();
    format!("{}/cache/{}/{}", plugins_dir, plugin_id, version)
}

/// Get versioned ZIP cache path for a plugin.
/// This is the zip cache variant of getVersionedCachePath.
fn get_versioned_zip_cache_path(plugin_id: &str, version: &str) -> String {
    format!("{}.zip", get_versioned_cache_path(plugin_id, version))
}

/// Load plugin manifest from path
async fn load_plugin_manifest(
    _manifest_path: &str,
    _name: &str,
    _source: &str,
) -> Result<serde_json::Value, String> {
    Err("load_plugin_manifest not implemented".to_string())
}

/// Calculate plugin version
async fn calculate_plugin_version(
    _plugin_id: &str,
    _source: &PluginSource,
    _manifest: Option<serde_json::Value>,
    _source_path: &str,
    _entry_version: Option<&str>,
    _git_commit_sha: Option<&str>,
) -> Result<String, String> {
    Err("calculate_plugin_version not implemented".to_string())
}

// ============================================================================
// Plugin Policy (stubs)
// ============================================================================

/// Check if a plugin is blocked by org policy
fn is_plugin_blocked_by_policy(_plugin_id: &str) -> bool {
    // In production, this would check managed-settings.json
    false
}

// ============================================================================
// Plugin Directories (stubs)
// ============================================================================

/// Delete plugin data directory
async fn delete_plugin_data_dir(_plugin_id: &str) -> Result<(), String> {
    log::debug!("Deleting plugin data dir: {}", _plugin_id);
    Ok(())
}

// ============================================================================
// Plugin Options Storage (stubs)
// ============================================================================

/// Delete plugin options
fn delete_plugin_options(_plugin_id: &str) {
    log::debug!("Deleting plugin options: {}", _plugin_id);
}

// ============================================================================
// Plugin Editable Scopes (stubs)
// ============================================================================

/// Get plugin editable scopes - returns set of enabled plugin IDs
fn get_plugin_editable_scopes() -> BTreeSet<String> {
    // In production, this would check all editable settings scopes
    BTreeSet::new()
}

// ============================================================================
// Dependency Resolution (stubs)
// ============================================================================

/// Find reverse dependents of a plugin
fn find_reverse_dependents(
    _plugin_id: &str,
    _all_plugins: &[LoadedPlugin],
) -> Vec<String> {
    Vec::new()
}

/// Format reverse dependents suffix for warning messages
fn format_reverse_dependents_suffix(reverse_dependents: Option<&[String]>) -> String {
    if let Some(deps) = reverse_dependents {
        if !deps.is_empty() {
            return format!(
                ". Warning: {} depend{} on this plugin: {}",
                plural(deps.len(), "plugin"),
                if deps.len() == 1 { "s" } else { "" },
                deps.join(", ")
            );
        }
    }
    String::new()
}

// ============================================================================
// Plugin Installation Helpers
// ============================================================================

/// Format resolution error message
pub(crate) fn format_resolution_error(resolution: &str) -> String {
    format!("Failed to resolve plugin: {}", resolution)
}

/// Install a resolved plugin
async fn install_resolved_plugin(
    _plugin_id: &str,
    _entry: &PluginMarketplaceEntry,
    _scope: InstallableScope,
    _marketplace_install_location: Option<&str>,
) -> InstallResolutionResult {
    // In production, this would:
    // 1. Check org policy
    // 2. Write settings (enable the plugin)
    // 3. Cache the plugin
    // 4. Record version hint
    InstallResolutionResult::Success {
        dep_note: String::new(),
    }
}

// ============================================================================
// Core Operation Helpers
// ============================================================================

/// Is this plugin enabled (value === true) in .claude/settings.json?
///
/// Distinct from V2 installed_plugins.json scope: that file tracks where a
/// plugin was *installed from*, but the same plugin can also be enabled at
/// project scope via settings. The uninstall UI needs to check THIS, because
/// a user-scope install with a project-scope enablement means "uninstall"
/// would succeed at removing the user install while leaving the project
/// enablement active -- the plugin keeps running.
pub fn is_plugin_enabled_at_project_scope(plugin_id: &str) -> bool {
    get_settings_for_source(SettingSource::ProjectSettings)
        .and_then(|s| s.enabled_plugins)
        .and_then(|ep| ep.get(plugin_id).cloned())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Search all editable settings scopes for a plugin ID matching the given input.
///
/// If `plugin` contains `@`, it's treated as a full plugin_id and returned if
/// found in any scope. If `plugin` is a bare name, searches for any key
/// starting with `{plugin}@` in any scope.
///
/// Returns the most specific scope where the plugin is mentioned (regardless
/// of enabled/disabled state) plus the resolved full plugin_id.
///
/// Precedence: local > project > user (most specific wins).
struct PluginInSettingsResult {
    plugin_id: String,
    scope: InstallableScope,
}

fn find_plugin_in_settings(plugin: &str) -> Option<PluginInSettingsResult> {
    let has_marketplace = plugin.contains('@');
    // Most specific first -- first match wins
    let search_order = [InstallableScope::Local, InstallableScope::Project, InstallableScope::User];

    for scope in search_order {
        let source = scope_to_setting_source(scope);
        let settings = get_settings_for_source(source)?;
        let enabled_plugins = settings.enabled_plugins?;

        for key in enabled_plugins.keys() {
            if has_marketplace {
                if key == plugin {
                    return Some(PluginInSettingsResult {
                        plugin_id: key.clone(),
                        scope,
                    });
                }
            } else if key.starts_with(&format!("{}@", plugin)) {
                return Some(PluginInSettingsResult {
                    plugin_id: key.clone(),
                    scope,
                });
            }
        }
    }
    None
}

/// Helper function to find a plugin from loaded plugins
fn find_plugin_by_identifier<'a>(
    plugin: &str,
    plugins: &'a [LoadedPlugin],
) -> Option<&'a LoadedPlugin> {
    let (name, marketplace) = parse_plugin_identifier(plugin);
    let name = name.as_deref().unwrap_or(plugin);

    plugins.iter().find(|p| {
        // Check exact name match
        if p.name == plugin || p.name == name {
            return true;
        }

        // If marketplace specified, check if it matches the source
        if let Some(ref mp) = marketplace {
            if let Some(ref source) = p.source {
                return p.name == name && source.contains(&format!("@{}", mp));
            }
        }

        false
    })
}

/// Resolve a plugin ID from V2 installed plugins data for a plugin that may
/// have been delisted from its marketplace. Returns None if the plugin is not
/// found in V2 data.
struct ResolvedDelistedPlugin {
    plugin_id: String,
    plugin_name: String,
}

fn resolve_delisted_plugin_id(plugin: &str) -> Option<ResolvedDelistedPlugin> {
    let (name, _) = parse_plugin_identifier(plugin);
    let plugin_name = name.as_deref().unwrap_or(plugin);
    let installed_data = load_installed_plugins_v2();

    // Try exact match first, then search by name
    if installed_data
        .plugins
        .get(plugin)
        .map_or(false, |v| !v.is_empty())
    {
        return Some(ResolvedDelistedPlugin {
            plugin_id: plugin.to_string(),
            plugin_name: plugin_name.to_string(),
        });
    }

    let matching_key = installed_data.plugins.keys().find(|key| {
        let (key_name, _) = parse_plugin_identifier(key);
        let key_name = key_name.as_deref().unwrap_or(key);
        key_name == plugin_name
            && installed_data
                .plugins
                .get(key.as_str())
                .map_or(false, |v| !v.is_empty())
    });

    matching_key.map(|key| ResolvedDelistedPlugin {
        plugin_id: key.clone(),
        plugin_name: plugin_name.to_string(),
    })
}

/// Get the most relevant installation for a plugin from V2 data.
/// For project/local scoped plugins, prioritizes installations matching the current project.
/// Priority order: local (matching project) > project (matching project) > user > first available
pub fn get_plugin_installation_from_v2(plugin_id: &str) -> (String, Option<String>) {
    // Returns (scope, project_path)
    let installed_data = load_installed_plugins_v2();
    let installations = installed_data.plugins.get(plugin_id);

    let installations = match installations {
        Some(insts) if !insts.is_empty() => insts,
        _ => return ("user".to_string(), None),
    };

    let current_project_path = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    // Find installations by priority: local > project > user > managed
    if let Some(local_install) = installations.iter().find(|inst| {
        inst.scope == "local" && inst.project_path == current_project_path
    }) {
        return (
            local_install.scope.clone(),
            local_install.project_path.clone(),
        );
    }

    if let Some(project_install) = installations.iter().find(|inst| {
        inst.scope == "project" && inst.project_path == current_project_path
    }) {
        return (
            project_install.scope.clone(),
            project_install.project_path.clone(),
        );
    }

    if let Some(user_install) = installations.iter().find(|inst| inst.scope == "user") {
        return (user_install.scope.clone(), user_install.project_path.clone());
    }

    // Fall back to first installation (could be managed)
    (
        installations[0].scope.clone(),
        installations[0].project_path.clone(),
    )
}

// ============================================================================
// Core Operations
// ============================================================================

/// Install a plugin (settings-first).
///
/// Order of operations:
///   1. Search materialized marketplaces for the plugin
///   2. Write settings (THE ACTION -- declares intent)
///   3. Cache plugin + record version hint (materialization)
///
/// Marketplace reconciliation is NOT this function's responsibility -- startup
/// reconcile handles declared-but-not-materialized marketplaces. If the
/// marketplace isn't found, "not found" is the correct error.
///
/// # Arguments
/// * `plugin` - Plugin identifier (name or plugin@marketplace)
/// * `scope` - Installation scope: user, project, or local (defaults to 'user')
///
/// # Returns
/// Result indicating success/failure
pub async fn install_plugin_op(
    plugin: &str,
    scope: InstallableScope,
) -> PluginOperationResult {
    let (plugin_name, marketplace_name) = parse_plugin_identifier(plugin);
    let plugin_name = plugin_name.unwrap_or_else(|| plugin.to_string());

    // Search materialized marketplaces for the plugin
    let mut found_plugin: Option<PluginMarketplaceEntry> = None;
    let mut found_marketplace: Option<String> = None;
    let mut marketplace_install_location: Option<String> = None;

    if let Some(ref mp_name) = marketplace_name {
        if let Some(plugin_info) = get_plugin_by_id(&format!("{}@{}", plugin_name, mp_name)).await {
            found_plugin = Some(plugin_info.entry);
            found_marketplace = Some(mp_name.clone());
            marketplace_install_location = Some(plugin_info.marketplace_install_location);
        }
    } else {
        let marketplaces = load_known_marketplaces_config().await;
        for (mkt_name, mkt_config) in &marketplaces {
            if let Ok(Some(marketplace)) =
                tokio::time::timeout(std::time::Duration::from_secs(5), get_marketplace(mkt_name))
                    .await
            {
                if let Some(plugin_entry) = marketplace.plugins.iter().find(|p| p.name == plugin_name) {
                    found_plugin = Some(plugin_entry.clone());
                    found_marketplace = Some(mkt_name.clone());
                    marketplace_install_location = mkt_config
                        .get("installLocation")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    break;
                }
            }
        }
    }

    let (entry, marketplace) = match (found_plugin, found_marketplace) {
        (Some(entry), Some(marketplace)) => (entry, marketplace),
        _ => {
            let location = marketplace_name
                .map(|m| format!("marketplace \"{}\"", m))
                .unwrap_or_else(|| "any configured marketplace".to_string());
            return PluginOperationResult {
                success: false,
                message: format!("Plugin \"{}\" not found in {}", plugin_name, location),
                plugin_id: None,
                plugin_name: None,
                scope: None,
                reverse_dependents: None,
            };
        }
    };

    let plugin_id = format!("{}@{}", entry.name, marketplace);

    let result = install_resolved_plugin(
        &plugin_id,
        &entry,
        scope,
        marketplace_install_location.as_deref(),
    )
    .await;

    match result {
        InstallResolutionResult::Success { dep_note } => PluginOperationResult {
            success: true,
            message: format!(
                "Successfully installed plugin: {} (scope: {}){}",
                plugin_id, scope, dep_note
            ),
            plugin_id: Some(plugin_id),
            plugin_name: Some(entry.name.clone()),
            scope: Some(scope.to_string()),
            reverse_dependents: None,
        },
        InstallResolutionResult::LocalSourceNoLocation { plugin_name } => {
            PluginOperationResult {
                success: false,
                message: format!(
                    "Cannot install local plugin \"{}\" without marketplace install location",
                    plugin_name
                ),
                plugin_id: None,
                plugin_name: None,
                scope: None,
                reverse_dependents: None,
            }
        }
        InstallResolutionResult::SettingsWriteFailed { message } => {
            PluginOperationResult {
                success: false,
                message: format!("Failed to update settings: {}", message),
                plugin_id: None,
                plugin_name: None,
                scope: None,
                reverse_dependents: None,
            }
        }
        InstallResolutionResult::ResolutionFailed { resolution } => PluginOperationResult {
            success: false,
            message: format_resolution_error(&resolution),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
        InstallResolutionResult::BlockedByPolicy { plugin_name } => {
            PluginOperationResult {
                success: false,
                message: format!(
                    "Plugin \"{}\" is blocked by your organization's policy and cannot be installed",
                    plugin_name
                ),
                plugin_id: None,
                plugin_name: None,
                scope: None,
                reverse_dependents: None,
            }
        }
        InstallResolutionResult::DependencyBlockedByPolicy {
            plugin_name,
            blocked_dependency,
        } => PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" depends on \"{}\", which is blocked by your organization's policy",
                plugin_name, blocked_dependency
            ),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
    }
}

/// Uninstall a plugin
///
/// # Arguments
/// * `plugin` - Plugin name or plugin@marketplace identifier
/// * `scope` - Uninstall from scope: user, project, or local (defaults to 'user')
/// * `delete_data_dir` - Whether to delete the plugin's data directory
///
/// # Returns
/// Result indicating success/failure
pub async fn uninstall_plugin_op(
    plugin: &str,
    scope: InstallableScope,
    delete_data_dir: bool,
) -> PluginOperationResult {
    let (enabled, disabled) = load_all_plugins().await;
    let all_plugins: Vec<LoadedPlugin> = enabled
        .into_iter()
        .chain(disabled.into_iter())
        .collect();

    // Find the plugin
    let found_plugin = find_plugin_by_identifier(plugin, &all_plugins);

    let setting_source = scope_to_setting_source(scope);
    let settings = get_settings_for_source(setting_source);

    let (plugin_id, plugin_name) = if let Some(found) = found_plugin {
        // Find the matching settings key for this plugin
        let plugin_id = settings
            .as_ref()
            .and_then(|s| s.enabled_plugins.as_ref())
            .and_then(|ep| {
                ep.keys().find(|k| {
                    **k == plugin || **k == found.name || k.starts_with(&format!("{}@", found.name))
                })
            })
            .cloned()
            .unwrap_or_else(|| {
                if plugin.contains('@') {
                    plugin.to_string()
                } else {
                    found.name.clone()
                }
            });
        (plugin_id, found.name.clone())
    } else {
        // Plugin not found via marketplace lookup -- may have been delisted
        match resolve_delisted_plugin_id(plugin) {
            Some(resolved) => (resolved.plugin_id, resolved.plugin_name),
            None => {
                return PluginOperationResult {
                    success: false,
                    message: format!("Plugin \"{}\" not found in installed plugins", plugin),
                    plugin_id: None,
                    plugin_name: None,
                    scope: None,
                    reverse_dependents: None,
                };
            }
        }
    };
    let plugin_name_clone = plugin_name.clone();

    // Check if the plugin is installed in this scope (in V2 file)
    let project_path = get_project_path_for_scope(&scope.to_string());
    let installed_data = load_installed_plugins_v2();
    let installations = installed_data.plugins.get(&plugin_id);

    let scope_installation = installations.and_then(|insts| {
        insts.iter().find(|i| {
            i.scope == scope.to_string() && i.project_path == project_path
        })
    });

    let scope_installation = match scope_installation {
        Some(inst) => inst,
        None => {
            // Try to find where the plugin is actually installed
            let (actual_scope, _) = get_plugin_installation_from_v2(&plugin_id);
            if actual_scope != scope.to_string()
                && installations.map_or(false, |i| !i.is_empty())
            {
                // Project scope is special
                if actual_scope == "project" {
                    return PluginOperationResult {
                        success: false,
                        message: format!(
                            "Plugin \"{}\" is enabled at project scope (.claude/settings.json, shared with your team). To disable just for you: claude plugin disable {} --scope local",
                            plugin, plugin
                        ),
                        plugin_id: Some(plugin_id.to_string()),
                        plugin_name: Some(plugin_name.clone()),
                        scope: Some(scope.to_string()),
                        reverse_dependents: None,
                    };
                }
                return PluginOperationResult {
                    success: false,
                    message: format!(
                        "Plugin \"{}\" is installed in {} scope, not {}. Use --scope {} to uninstall.",
                        plugin, actual_scope, scope, actual_scope
                    ),
                    plugin_id: Some(plugin_id.to_string()),
                    plugin_name: Some(plugin_name.clone()),
                    scope: Some(scope.to_string()),
                    reverse_dependents: None,
                };
            }
            return PluginOperationResult {
                success: false,
                message: format!(
                    "Plugin \"{}\" is not installed in {} scope. Use --scope to specify the correct scope.",
                    plugin, scope
                ),
                plugin_id: Some(plugin_id.to_string()),
                plugin_name: Some(plugin_name.clone()),
                scope: Some(scope.to_string()),
                reverse_dependents: None,
            };
        }
    };

    let install_path = scope_installation.install_path.clone();

    // Remove the plugin from the appropriate settings file
    let mut new_enabled_plugins: BTreeMap<String, Option<serde_json::Value>> = settings
        .as_ref()
        .and_then(|s| s.enabled_plugins.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, Some(v)))
        .collect();
    new_enabled_plugins.insert(plugin_id.to_string(), None);

    let _ = update_settings_for_source(setting_source, &SettingsJson {
        enabled_plugins: Some(
            new_enabled_plugins
                .into_iter()
                .filter_map(|(k, v)| v.map(|val| (k, val)))
                .collect(),
        ),
    });

    clear_all_caches();

    // Remove from installed_plugins_v2.json for this scope
    remove_plugin_installation(&plugin_id, &scope.to_string(), project_path.as_deref());

    // Check if this is the last scope installation
    let updated_data = load_installed_plugins_v2();
    let remaining_installations = updated_data.plugins.get(&plugin_id);
    let is_last_scope = remaining_installations.map_or(true, |i| i.is_empty());

    if is_last_scope {
        mark_plugin_version_orphaned(&install_path).await;
        // Delete plugin options and data dir
        delete_plugin_options(&plugin_id);
        if delete_data_dir {
            let _ = delete_plugin_data_dir(&plugin_id).await;
        }
    }

    // Warn (don't block) if other enabled plugins depend on this one
    let reverse_dependents = find_reverse_dependents(&plugin_id, &all_plugins);
    let dep_warn = format_reverse_dependents_suffix(if reverse_dependents.is_empty() {
        None
    } else {
        Some(&reverse_dependents)
    });

    PluginOperationResult {
        success: true,
        message: format!(
            "Successfully uninstalled plugin: {} (scope: {}){}",
            plugin_name, scope, dep_warn
        ),
        plugin_id: Some(plugin_id.to_string()),
        plugin_name: Some(plugin_name),
        scope: Some(scope.to_string()),
        reverse_dependents: if reverse_dependents.is_empty() {
            None
        } else {
            Some(reverse_dependents)
        },
    }
}

/// Set plugin enabled/disabled status (settings-first).
///
/// Resolves the plugin ID and scope from settings -- does NOT pre-gate on
/// installed_plugins.json. Settings declares intent; if the plugin isn't
/// cached yet, the next load will cache it.
///
/// # Arguments
/// * `plugin` - Plugin name or plugin@marketplace identifier
/// * `enabled` - true to enable, false to disable
/// * `scope` - Optional scope. If not provided, auto-detects the most specific
///   scope where the plugin is mentioned in settings.
///
/// # Returns
/// Result indicating success/failure
pub async fn set_plugin_enabled_op(
    plugin: &str,
    enabled: bool,
    scope: Option<InstallableScope>,
) -> PluginOperationResult {
    let operation = if enabled { "enable" } else { "disable" };

    // Built-in plugins: always use user-scope settings, bypass the normal
    // scope-resolution + installed_plugins lookup (they're not installed).
    if is_builtin_plugin_id(plugin) {
        let current_settings = get_settings_for_source(SettingSource::UserSettings);
        let mut enabled_plugins = current_settings
            .and_then(|s| s.enabled_plugins)
            .unwrap_or_default();
        enabled_plugins.insert(
            plugin.to_string(),
            serde_json::Value::Bool(enabled),
        );

        match update_settings_for_source(
            SettingSource::UserSettings,
            &SettingsJson {
                enabled_plugins: Some(enabled_plugins),
            },
        ) {
            Ok(()) => {
                clear_all_caches();
                let (_, plugin_name) = parse_plugin_identifier(plugin);
                return PluginOperationResult {
                    success: true,
                    message: format!(
                        "Successfully {}d built-in plugin: {}",
                        operation,
                        plugin_name.as_deref().unwrap_or(plugin)
                    ),
                    plugin_id: Some(plugin.to_string()),
                    plugin_name: plugin_name,
                    scope: Some("user".to_string()),
                    reverse_dependents: None,
                };
            }
            Err(error) => {
                return PluginOperationResult {
                    success: false,
                    message: format!("Failed to {} built-in plugin: {}", operation, error),
                    plugin_id: None,
                    plugin_name: None,
                    scope: None,
                    reverse_dependents: None,
                };
            }
        }
    }

    // Validate scope if provided
    if let Some(s) = scope {
        if let Err(e) = assert_installable_scope(&s.to_string()) {
            return PluginOperationResult {
                success: false,
                message: e,
                plugin_id: None,
                plugin_name: None,
                scope: None,
                reverse_dependents: None,
            };
        }
    }

    // Resolve pluginId and scope from settings
    let (plugin_id, resolved_scope) = match scope {
        Some(explicit_scope) => {
            // Explicit scope: use it. Resolve pluginId from settings if possible,
            // otherwise require a full plugin@marketplace identifier.
            if let Some(found) = find_plugin_in_settings(plugin) {
                (found.plugin_id, explicit_scope)
            } else if plugin.contains('@') {
                (plugin.to_string(), explicit_scope)
            } else {
                return PluginOperationResult {
                    success: false,
                    message: format!(
                        "Plugin \"{}\" not found in settings. Use plugin@marketplace format.",
                        plugin
                    ),
                    plugin_id: None,
                    plugin_name: None,
                    scope: None,
                    reverse_dependents: None,
                };
            }
        }
        None => {
            // Auto-detect scope from settings
            if let Some(found) = find_plugin_in_settings(plugin) {
                (found.plugin_id, found.scope)
            } else if plugin.contains('@') {
                // Not in any settings scope, but full pluginId given -- default to user scope
                (plugin.to_string(), InstallableScope::User)
            } else {
                return PluginOperationResult {
                    success: false,
                    message: format!(
                        "Plugin \"{}\" not found in any editable settings scope. Use plugin@marketplace format.",
                        plugin
                    ),
                    plugin_id: None,
                    plugin_name: None,
                    scope: None,
                    reverse_dependents: None,
                };
            }
        }
    };

    // Policy guard: org-blocked plugins cannot be enabled at any scope
    if enabled && is_plugin_blocked_by_policy(&plugin_id) {
        return PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is blocked by your organization's policy and cannot be enabled",
                plugin_id
            ),
            plugin_id: Some(plugin_id),
            plugin_name: None,
            scope: Some(resolved_scope.to_string()),
            reverse_dependents: None,
        };
    }

    let setting_source = scope_to_setting_source(resolved_scope);
    let scope_settings_value = get_settings_for_source(setting_source)
        .and_then(|s| s.enabled_plugins)
        .and_then(|ep| ep.get(&plugin_id).cloned())
        .and_then(|v| v.as_bool());

    // Cross-scope hint: explicit scope given but plugin is elsewhere
    let scope_precedence = |s: InstallableScope| -> usize {
        match s {
            InstallableScope::User => 0,
            InstallableScope::Project => 1,
            InstallableScope::Local => 2,
        }
    };

    let is_override = scope
        .zip(find_plugin_in_settings(plugin))
        .map(|(s, found)| scope_precedence(s) > scope_precedence(found.scope))
        .unwrap_or(false);

    if scope.is_some()
        && scope_settings_value.is_none()
        && find_plugin_in_settings(plugin).as_ref().is_some_and(|found| {
            let found_scope = found.scope;
            scope
                .map(|s| s != found_scope && !is_override)
                .unwrap_or(false)
        })
    {
        let found = find_plugin_in_settings(plugin).unwrap();
        return PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is installed at {} scope, not {}. Use --scope {} or omit --scope to auto-detect.",
                plugin, found.scope, resolved_scope, found.scope
            ),
            plugin_id: Some(plugin_id),
            plugin_name: None,
            scope: Some(resolved_scope.to_string()),
            reverse_dependents: None,
        };
    }

    // Check current state (for idempotency messaging)
    let is_currently_enabled = if scope.is_some() && !is_override {
        scope_settings_value.unwrap_or(false)
    } else {
        get_plugin_editable_scopes().contains(&plugin_id)
    };

    if enabled == is_currently_enabled {
        let scope_suffix = scope.map(|s| format!(" at {} scope", s)).unwrap_or_default();
        return PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is already {}{}",
                plugin,
                if enabled { "enabled" } else { "disabled" },
                scope_suffix
            ),
            plugin_id: Some(plugin_id),
            plugin_name: None,
            scope: Some(resolved_scope.to_string()),
            reverse_dependents: None,
        };
    }

    // On disable: capture reverse dependents from the pre-disable snapshot
    let mut reverse_dependents: Option<Vec<String>> = None;
    if !enabled {
        let (loaded_enabled, disabled) = load_all_plugins().await;
        let all: Vec<LoadedPlugin> = loaded_enabled
            .into_iter()
            .chain(disabled.into_iter())
            .collect();
        let rdeps = find_reverse_dependents(&plugin_id, &all);
        if !rdeps.is_empty() {
            reverse_dependents = Some(rdeps);
        }
    }

    // Write settings
    let current_settings = get_settings_for_source(setting_source);
    let mut enabled_plugins = current_settings
        .and_then(|s| s.enabled_plugins)
        .unwrap_or_default();
    enabled_plugins.insert(
        plugin_id.clone(),
        serde_json::Value::Bool(enabled),
    );

    if let Err(error) = update_settings_for_source(
        setting_source,
        &SettingsJson {
            enabled_plugins: Some(enabled_plugins),
        },
    ) {
        return PluginOperationResult {
            success: false,
            message: format!("Failed to {} plugin: {}", operation, error),
            plugin_id: Some(plugin_id),
            plugin_name: None,
            scope: Some(resolved_scope.to_string()),
            reverse_dependents: None,
        };
    }

    clear_all_caches();

    let (_, plugin_name) = parse_plugin_identifier(&plugin_id);
    let dep_warn = format_reverse_dependents_suffix(reverse_dependents.as_deref());

    PluginOperationResult {
        success: true,
        message: format!(
            "Successfully {}d plugin: {} (scope: {}){}",
            operation,
            plugin_name.as_deref().unwrap_or(&plugin_id),
            resolved_scope,
            dep_warn
        ),
        plugin_id: Some(plugin_id),
        plugin_name,
        scope: Some(resolved_scope.to_string()),
        reverse_dependents,
    }
}

/// Enable a plugin
///
/// # Arguments
/// * `plugin` - Plugin name or plugin@marketplace identifier
/// * `scope` - Optional scope. If not provided, finds the most specific scope for the current project.
///
/// # Returns
/// Result indicating success/failure
pub async fn enable_plugin_op(
    plugin: &str,
    scope: Option<InstallableScope>,
) -> PluginOperationResult {
    set_plugin_enabled_op(plugin, true, scope).await
}

/// Disable a plugin
///
/// # Arguments
/// * `plugin` - Plugin name or plugin@marketplace identifier
/// * `scope` - Optional scope. If not provided, finds the most specific scope for the current project.
///
/// # Returns
/// Result indicating success/failure
pub async fn disable_plugin_op(
    plugin: &str,
    scope: Option<InstallableScope>,
) -> PluginOperationResult {
    set_plugin_enabled_op(plugin, false, scope).await
}

/// Disable all enabled plugins
///
/// # Returns
/// Result indicating success/failure with count of disabled plugins
pub async fn disable_all_plugins_op() -> PluginOperationResult {
    let enabled_plugins = get_plugin_editable_scopes();

    if enabled_plugins.is_empty() {
        return PluginOperationResult {
            success: true,
            message: "No enabled plugins to disable".to_string(),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        };
    }

    let mut disabled: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for plugin_id in enabled_plugins {
        let result = set_plugin_enabled_op(&plugin_id, false, None).await;
        if result.success {
            disabled.push(plugin_id);
        } else {
            errors.push(format!("{}: {}", plugin_id, result.message));
        }
    }

    if !errors.is_empty() {
        return PluginOperationResult {
            success: false,
            message: format!(
                "Disabled {} {}, {} failed:\n{}",
                disabled.len(),
                plural(disabled.len(), "plugin"),
                errors.len(),
                errors.join("\n")
            ),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        };
    }

    PluginOperationResult {
        success: true,
        message: format!(
            "Disabled {} {}",
            disabled.len(),
            plural(disabled.len(), "plugin")
        ),
        plugin_id: None,
        plugin_name: None,
        scope: None,
        reverse_dependents: None,
    }
}

/// Update a plugin to the latest version.
///
/// This function performs a NON-INPLACE update:
/// 1. Gets the plugin info from the marketplace
/// 2. For remote plugins: downloads to temp dir and calculates version
/// 3. For local plugins: calculates version from marketplace source
/// 4. If version differs from currently installed, copies to new versioned cache directory
/// 5. Updates installation in V2 file (memory stays unchanged until restart)
/// 6. Cleans up old version if no longer referenced by any installation
///
/// # Arguments
/// * `plugin` - Plugin name or plugin@marketplace identifier
/// * `scope` - Scope to update. Unlike install/uninstall/enable/disable, managed scope IS allowed.
///
/// # Returns
/// Result indicating success/failure with version info
pub async fn update_plugin_op(
    plugin: &str,
    scope: &str,
) -> PluginUpdateResult {
    // Parse the plugin identifier to get the full plugin ID
    let (plugin_name, marketplace_name) = parse_plugin_identifier(plugin);
    let plugin_name = plugin_name.unwrap_or_else(|| plugin.to_string());
    let plugin_id = marketplace_name
        .map(|m| format!("{}@{}", plugin_name, m))
        .unwrap_or_else(|| plugin.to_string());

    let scope = match PluginScope::try_from(scope) {
        Ok(s) => s,
        Err(e) => {
            return PluginUpdateResult {
                success: false,
                message: e,
                plugin_id: Some(plugin_id),
                new_version: None,
                old_version: None,
                already_up_to_date: None,
                scope: None,
            };
        }
    };

    // Get plugin info from marketplace
    let plugin_info = match get_plugin_by_id(&plugin_id).await {
        Some(info) => info,
        None => {
            return PluginUpdateResult {
                success: false,
                message: format!("Plugin \"{}\" not found", plugin_name),
                plugin_id: Some(plugin_id),
                new_version: None,
                old_version: None,
                already_up_to_date: None,
                scope: Some(scope.to_string()),
            };
        }
    };

    let entry = plugin_info.entry;
    let marketplace_install_location = plugin_info.marketplace_install_location;

    // Get installations from disk
    let disk_data = load_installed_plugins_from_disk();
    let installations = disk_data.plugins.get(&plugin_id);

    if installations.is_none() || installations.map_or(true, |i| i.is_empty()) {
        return PluginUpdateResult {
            success: false,
            message: format!("Plugin \"{}\" is not installed", plugin_name),
            plugin_id: Some(plugin_id),
            new_version: None,
            old_version: None,
            already_up_to_date: None,
            scope: Some(scope.to_string()),
        };
    }

    // Determine project_path based on scope
    let project_path = get_project_path_for_scope(&scope.to_string());

    // Find the installation for this scope
    let installations = installations.unwrap();
    let installation = installations.iter().find(|inst| {
        inst.scope == scope.to_string() && inst.project_path == project_path
    });

    let installation = match installation {
        Some(inst) => inst,
        None => {
            let scope_desc = project_path
                .map(|p| format!("{} ({})", scope, p))
                .unwrap_or_else(|| scope.to_string());
            return PluginUpdateResult {
                success: false,
                message: format!(
                    "Plugin \"{}\" is not installed at scope {}",
                    plugin_name, scope_desc
                ),
                plugin_id: Some(plugin_id),
                new_version: None,
                old_version: None,
                already_up_to_date: None,
                scope: Some(scope.to_string()),
            };
        }
    };

    perform_plugin_update(
        &plugin_id,
        &plugin_name,
        &entry,
        &marketplace_install_location,
        installation,
        scope,
        project_path,
    )
    .await
}

/// Perform the actual plugin update: fetch source, calculate version, copy to cache, update disk.
/// This is the core update execution extracted from update_plugin_op.
async fn perform_plugin_update(
    plugin_id: &str,
    plugin_name: &str,
    entry: &PluginMarketplaceEntry,
    marketplace_install_location: &str,
    installation: &PluginInstallationEntry,
    scope: PluginScope,
    project_path: Option<String>,
) -> PluginUpdateResult {
    let old_version = installation.version.clone();

    let (source_path, new_version, should_cleanup_source, git_commit_sha) =
        match &entry.source {
            PluginSource::Npm { .. }
            | PluginSource::Pip { .. }
            | PluginSource::Github { .. }
            | PluginSource::GitSubdir { .. }
            | PluginSource::Git { .. }
            | PluginSource::Url { .. } => {
                // Remote plugin: download to temp directory first
                match cache_plugin(
                    &entry.source,
                    CachePluginOptions {
                        manifest: None,
                    },
                )
                .await
                {
                    Ok(cache_result) => {
                        // Calculate version from downloaded plugin
                        let new_version = match calculate_plugin_version(
                            plugin_id,
                            &entry.source,
                            Some(cache_result.manifest.clone()),
                            &cache_result.path,
                            entry.version.as_deref(),
                            cache_result.git_commit_sha.as_deref(),
                        )
                        .await
                        {
                            Ok(v) => v,
                            Err(e) => {
                                return PluginUpdateResult {
                                    success: false,
                                    message: format!("Failed to calculate version: {}", e),
                                    plugin_id: Some(plugin_id.to_string()),
                                    new_version: None,
                                    old_version: old_version.clone(),
                                    already_up_to_date: None,
                                    scope: Some(scope.to_string()),
                                };
                            }
                        };
                        (
                            cache_result.path,
                            new_version,
                            true,
                            cache_result.git_commit_sha,
                        )
                    }
                    Err(e) => {
                        return PluginUpdateResult {
                            success: false,
                            message: format!("Failed to cache plugin: {}", e),
                            plugin_id: Some(plugin_id.to_string()),
                            new_version: None,
                            old_version: old_version.clone(),
                            already_up_to_date: None,
                            scope: Some(scope.to_string()),
                        };
                    }
                }
            }
            PluginSource::Relative(_) => {
                // Local plugin: use path from marketplace
                let marketplace_path = PathBuf::from(marketplace_install_location);
                let marketplace_dir = if marketplace_path.is_dir() {
                    marketplace_path
                } else {
                    marketplace_path
                        .parent()
                        .unwrap_or(&marketplace_path)
                        .to_path_buf()
                };
                let source_path = marketplace_dir.join(
                    if let PluginSource::Relative(rel) = &entry.source {
                        rel
                    } else {
                        ""
                    },
                );

                // Verify source_path exists
                if !source_path.exists() {
                    return PluginUpdateResult {
                        success: false,
                        message: format!(
                            "Plugin source not found at {}",
                            source_path.display()
                        ),
                        plugin_id: Some(plugin_id.to_string()),
                        new_version: None,
                        old_version: old_version.clone(),
                        already_up_to_date: None,
                        scope: Some(scope.to_string()),
                    };
                }

                // Try to load manifest from plugin directory
                let plugin_manifest = load_plugin_manifest(
                    &source_path.join(".claude-plugin").join("plugin.json").to_string_lossy(),
                    &entry.name,
                    if let PluginSource::Relative(rel) = &entry.source {
                        rel
                    } else {
                        ""
                    },
                )
                .await
                .ok();

                // Calculate version from plugin source path
                let new_version = match calculate_plugin_version(
                    plugin_id,
                    &entry.source,
                    plugin_manifest,
                    &source_path.to_string_lossy(),
                    entry.version.as_deref(),
                    None,
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        return PluginUpdateResult {
                            success: false,
                            message: format!("Failed to calculate version: {}", e),
                            plugin_id: Some(plugin_id.to_string()),
                            new_version: None,
                            old_version: old_version.clone(),
                            already_up_to_date: None,
                            scope: Some(scope.to_string()),
                        };
                    }
                };

                (
                    source_path.to_string_lossy().to_string(),
                    new_version,
                    false,
                    None,
                )
            }
            PluginSource::Settings { .. } => {
                return PluginUpdateResult {
                    success: false,
                    message: format!(
                        "Cannot update plugin \"{}\" with settings source",
                        plugin_name
                    ),
                    plugin_id: Some(plugin_id.to_string()),
                    new_version: None,
                    old_version: old_version.clone(),
                    already_up_to_date: None,
                    scope: Some(scope.to_string()),
                };
            }
        };

    // Check if this version already exists in cache
    let versioned_path = get_versioned_cache_path(plugin_id, &new_version);

    // Check if installation is already at the new version
    let zip_path = get_versioned_zip_cache_path(plugin_id, &new_version);
    let is_up_to_date = old_version.as_deref() == Some(&new_version)
        || installation.install_path == versioned_path
        || installation.install_path == zip_path;

    if is_up_to_date {
        return PluginUpdateResult {
            success: true,
            message: format!(
                "{} is already at the latest version ({}).",
                plugin_name, new_version
            ),
            plugin_id: Some(plugin_id.to_string()),
            new_version: Some(new_version),
            old_version,
            already_up_to_date: Some(true),
            scope: Some(scope.to_string()),
        };
    }

    // Copy to versioned cache
    let versioned_path = match copy_plugin_to_versioned_cache(
        &source_path,
        plugin_id,
        &new_version,
        entry,
    )
    .await
    {
        Ok(path) => path,
        Err(e) => {
            return PluginUpdateResult {
                success: false,
                message: format!("Failed to copy plugin to cache: {}", e),
                plugin_id: Some(plugin_id.to_string()),
                new_version: Some(new_version),
                old_version,
                already_up_to_date: None,
                scope: Some(scope.to_string()),
            };
        }
    };

    // Store old version path for potential cleanup
    let old_version_path = installation.install_path.clone();

    // Update disk JSON file for this installation
    update_installation_path_on_disk(
        plugin_id,
        &scope.to_string(),
        project_path.as_deref(),
        &versioned_path,
        &new_version,
        git_commit_sha.as_deref(),
    );

    // Check if old version is still referenced
    let updated_disk_data = load_installed_plugins_from_disk();
    let is_old_version_still_referenced = updated_disk_data.plugins.values().any(
        |plugin_installations| {
            plugin_installations
                .iter()
                .any(|inst| inst.install_path == old_version_path)
        },
    );

    if !is_old_version_still_referenced && !old_version_path.is_empty() {
        mark_plugin_version_orphaned(&old_version_path).await;
    }

    let scope_desc = project_path
        .map(|p| format!("{} ({})", scope, p))
        .unwrap_or_else(|| scope.to_string());
    let message = format!(
        "Plugin \"{}\" updated from {} to {} for scope {}. Restart to apply changes.",
        plugin_name,
        old_version.as_ref().cloned().unwrap_or_else(|| "unknown".to_string()),
        new_version,
        scope_desc
    );

    // Clean up temp source if it was a remote download
    if should_cleanup_source && source_path != get_versioned_cache_path(plugin_id, &new_version) {
        let _ = std::fs::remove_dir_all(&source_path);
    }

    PluginUpdateResult {
        success: true,
        message,
        plugin_id: Some(plugin_id.to_string()),
        new_version: Some(new_version),
        old_version,
        already_up_to_date: None,
        scope: Some(scope.to_string()),
    }
}

