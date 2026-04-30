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
    Success {
        dep_note: String,
    },
    LocalSourceNoLocation {
        plugin_name: String,
    },
    SettingsWriteFailed {
        message: String,
    },
    ResolutionFailed {
        resolution: String,
    },
    BlockedByPolicy {
        plugin_name: String,
    },
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
// Settings Operations
// ============================================================================

/// Get settings for a specific source
fn get_settings_for_source(source: SettingSource) -> Option<SettingsJson> {
    // Policy and flag settings are not persisted to editable files
    match source {
        SettingSource::PolicySettings | SettingSource::FlagSettings => None,
        _ => {
            let editable = match source {
                SettingSource::UserSettings => {
                    crate::utils::settings::EditableSettingSource::UserSettings
                }
                SettingSource::ProjectSettings => {
                    crate::utils::settings::EditableSettingSource::ProjectSettings
                }
                SettingSource::LocalSettings => {
                    crate::utils::settings::EditableSettingSource::LocalSettings
                }
                _ => return None,
            };
            let value = crate::utils::settings::get_settings_for_source(&editable)?;
            let ep = value.get("enabledPlugins").and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                });
            Some(SettingsJson { enabled_plugins: ep })
        }
    }
}

/// Update settings for a specific source
/// Returns error message if the update failed
fn update_settings_for_source(
    source: SettingSource,
    settings: &SettingsJson,
) -> Result<(), String> {
    // Policy and flag settings are not written back
    match source {
        SettingSource::PolicySettings | SettingSource::FlagSettings => return Ok(()),
        _ => { }
    }

    let editable = match source {
        SettingSource::UserSettings => {
            crate::utils::settings::EditableSettingSource::UserSettings
        }
        SettingSource::ProjectSettings => {
            crate::utils::settings::EditableSettingSource::ProjectSettings
        }
        SettingSource::LocalSettings => {
            crate::utils::settings::EditableSettingSource::LocalSettings
        }
        _ => return Ok(()),
    };

    let mut map = serde_json::Map::new();
    if let Some(ref ep) = settings.enabled_plugins {
        let ep_obj = serde_json::Value::Object(
            ep.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        );
        map.insert("enabledPlugins".to_string(), ep_obj);
    }

    crate::utils::settings::update_settings_for_source(&editable, &serde_json::Value::Object(map))
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
    match crate::utils::plugins::loader::load_all_plugins().await {
        Ok(result) => {
            let enabled: Vec<LoadedPlugin> = result
                .enabled
                .into_iter()
                .map(|p| LoadedPlugin {
                    name: p.name,
                    source: Some(p.source),
                    manifest: serde_json::to_value(&p.manifest).ok(),
                })
                .collect();
            let disabled: Vec<LoadedPlugin> = result
                .disabled
                .into_iter()
                .map(|p| LoadedPlugin {
                    name: p.name,
                    source: Some(p.source),
                    manifest: serde_json::to_value(&p.manifest).ok(),
                })
                .collect();
            (enabled, disabled)
        }
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// Load installed plugins from disk
fn load_installed_plugins_from_disk() -> InstalledPluginsV2 {
    match crate::utils::plugins::installed_plugins_manager::load_installed_plugins_from_disk() {
        Ok(data) => InstalledPluginsV2 {
            plugins: data
                .plugins
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        v.into_iter()
                            .map(|e| {
                                let scope_str = match e.scope {
                                    crate::utils::plugins::schemas::PluginScope::User => "user",
                                    crate::utils::plugins::schemas::PluginScope::Managed => "managed",
                                    crate::utils::plugins::schemas::PluginScope::Project => "project",
                                    crate::utils::plugins::schemas::PluginScope::Local => "local",
                                };
                                PluginInstallationEntry {
                                    scope: scope_str.to_string(),
                                    project_path: e.project_path,
                                    install_path: e.install_path,
                                    version: e.version,
                                    git_commit_sha: e.git_commit_sha,
                                }
                            })
                            .collect(),
                    )
                })
                .collect(),
        },
        Err(e) => {
            log::debug!("Failed to load installed plugins from disk: {}", e);
            InstalledPluginsV2::default()
        }
    }
}

/// Load installed plugins V2
fn load_installed_plugins_v2() -> InstalledPluginsV2 {
    load_installed_plugins_from_disk()
}

/// Remove a plugin installation from disk
fn remove_plugin_installation(plugin_id: &str, scope: &str, project_path: Option<&str>) {
    let scope_enum = match scope {
        "user" => crate::utils::plugins::schemas::PluginScope::User,
        "managed" => crate::utils::plugins::schemas::PluginScope::Managed,
        "project" => crate::utils::plugins::schemas::PluginScope::Project,
        "local" => crate::utils::plugins::schemas::PluginScope::Local,
        _ => {
            log::debug!("Invalid scope for plugin removal: {}", scope);
            return;
        }
    };
    crate::utils::plugins::installed_plugins_manager::remove_plugin_installation(
        plugin_id,
        scope_enum,
        project_path,
    );
}

/// Update installation path on disk
fn update_installation_path_on_disk(
    plugin_id: &str,
    scope: &str,
    project_path: Option<&str>,
    new_path: &str,
    new_version: &str,
    git_commit_sha: Option<&str>,
) {
    let scope_enum = match scope {
        "user" => crate::utils::plugins::schemas::PluginScope::User,
        "managed" => crate::utils::plugins::schemas::PluginScope::Managed,
        "project" => crate::utils::plugins::schemas::PluginScope::Project,
        "local" => crate::utils::plugins::schemas::PluginScope::Local,
        _ => {
            log::debug!("Invalid scope for installation path update: {}", scope);
            return;
        }
    };
    crate::utils::plugins::installed_plugins_manager::update_installation_path_on_disk(
        plugin_id,
        scope_enum,
        project_path,
        new_path,
        new_version,
        git_commit_sha,
    );
}

// ============================================================================
// Marketplace Operations (stubs)
// ============================================================================

/// Load known marketplaces config
async fn load_known_marketplaces_config() -> HashMap<String, serde_json::Value> {
    match crate::utils::plugins::marketplace_manager::load_known_marketplaces_config().await {
        Ok(config) => config
            .into_iter()
            .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_else(|_| serde_json::Value::Null)))
            .collect(),
        Err(e) => {
            log::debug!("Failed to load known marketplaces config: {}", e);
            HashMap::new()
        }
    }
}

/// Get a marketplace by name
async fn get_marketplace(name: &str) -> Option<PluginMarketplace> {
    match crate::utils::plugins::marketplace_manager::get_marketplace(name).await {
        Ok(mp) => Some(mp),
        Err(e) => {
            log::debug!("Failed to get marketplace {}: {}", name, e);
            None
        }
    }
}

/// Get a plugin by ID from marketplace
async fn get_plugin_by_id(plugin: &str) -> Option<PluginInfo> {
    match crate::utils::plugins::marketplace_manager::get_plugin_by_id(plugin).await {
        Some(result) => Some(PluginInfo {
            entry: result.entry,
            marketplace_install_location: result.marketplace_install_location,
        }),
        None => None,
    }
}

// ============================================================================
// Cache Operations (stubs)
// ============================================================================

/// Clear all caches
fn clear_all_caches() {
    crate::utils::plugins::cache_utils::clear_all_caches();
}

/// Clear plugin cache with optional reason
fn clear_plugin_cache(reason: &str) {
    log::debug!("Clearing plugin cache: {}", reason);
    crate::utils::plugins::loader::clear_plugin_cache(None);
}

/// Mark a plugin version as orphaned
async fn mark_plugin_version_orphaned(install_path: &str) {
    if let Err(e) =
        crate::utils::plugins::cache_utils::mark_plugin_version_orphaned(
            std::path::Path::new(install_path),
        )
        .await
    {
        log::debug!("Failed to mark plugin version orphaned: {}", e);
    }
}

/// Cache a plugin (download to temp)
async fn cache_plugin(
    source: &PluginSource,
    options: CachePluginOptions,
) -> Result<CachePluginResult, String> {
    use crate::utils::plugins::plugin_directories::get_plugins_directory;
    use std::time::SystemTime;

    let cache_path = format!("{}/cache", get_plugins_directory());
    std::fs::create_dir_all(&cache_path).map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let temp_name = format!(
        "temp_{}_{}",
        plugin_source_prefix(source),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let temp_path = format!("{}/{}", cache_path, temp_name);

    // Install source into temp path
    let git_commit_sha = install_plugin_source(source, &temp_path).await?;

    // Load manifest from .claude-plugin/plugin.json or plugin.json
    let manifest_path = format!("{}/.claude-plugin/plugin.json", temp_path);
    let legacy_manifest_path = format!("{}/plugin.json", temp_path);
    let manifest = if Path::new(&manifest_path).exists() {
        load_plugin_manifest(&manifest_path, &temp_name, "cached").await?
    } else if Path::new(&legacy_manifest_path).exists() {
        load_plugin_manifest(&legacy_manifest_path, &temp_name, "cached").await?
    } else {
        options.manifest.clone().unwrap_or_else(|| {
            serde_json::json!({
                "name": temp_name,
                "description": format!("Plugin cached from {}", plugin_source_type(source)),
            })
        })
    };

    let final_name = manifest["name"]
        .as_str()
        .map(|n| n.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-"))
        .unwrap_or_else(|| temp_name.clone());
    let final_path = format!("{}/{}", cache_path, final_name);

    // Remove old cached version if exists
    if Path::new(&final_path).exists() {
        let _ = std::fs::remove_dir_all(&final_path);
    }

    std::fs::rename(&temp_path, &final_path)
        .map_err(|e| format!("Failed to move cached plugin: {}", e))?;

    Ok(CachePluginResult {
        path: final_path,
        manifest,
        git_commit_sha,
    })
}

/// Generate a temp name prefix from a plugin source.
fn plugin_source_prefix(source: &PluginSource) -> &str {
    match source {
        PluginSource::Relative(p) => p.as_str(),
        PluginSource::Npm { package, .. } => package.as_str(),
        PluginSource::Pip { package, .. } => package.as_str(),
        PluginSource::Github { repo, .. } => repo.as_str(),
        PluginSource::GitSubdir { repo, .. } => repo.as_str(),
        PluginSource::Git { url, .. } => url.as_str(),
        PluginSource::Url { url, .. } => url.as_str(),
        PluginSource::Settings { .. } => "settings",
    }
}

/// Return the source type string for a plugin source.
fn plugin_source_type(source: &PluginSource) -> &str {
    match source {
        PluginSource::Relative(_) => "local path",
        PluginSource::Npm { .. } => "npm",
        PluginSource::Pip { .. } => "pip",
        PluginSource::Github { .. } => "github",
        PluginSource::GitSubdir { .. } => "git-subdir",
        PluginSource::Git { .. } => "git",
        PluginSource::Url { .. } => "url",
        PluginSource::Settings { .. } => "settings",
    }
}

/// Install a plugin source into a target directory.
/// Returns optional git commit SHA for git-based sources.
async fn install_plugin_source(
    source: &PluginSource,
    target: &str,
) -> Result<Option<String>, String> {
    match source {
        PluginSource::Relative(p) => {
            // Copy local directory
            let src = Path::new(p);
            if !src.exists() {
                return Err(format!("Local plugin path does not exist: {}", p));
            }
            copy_directory(src, Path::new(target))?;
            Ok(None)
        }
        PluginSource::Git { url, ref_, .. }
        | PluginSource::Github {
            repo: url,
            ref_,
            ..
        } => {
            run_git_clone(url, target, ref_)?;
            let sha = get_git_head_sha(target)?;
            Ok(Some(sha))
        }
        PluginSource::GitSubdir {
            repo: url,
            ref_,
            subdir,
            ..
        } => {
            run_git_sparse_clone(url, target, ref_, subdir)?;
            let sha = get_git_head_sha(target)?;
            Ok(Some(sha))
        }
        PluginSource::Npm { package, .. } => {
            // Install from npm: run `npm pack` + extract
            std::process::Command::new("npm")
                .args(["pack", package])
                .output()
                .map_err(|e| format!("npm pack failed: {}", e))?;
            // Find the .tgz and extract
            let dir = std::fs::read_dir(".")
                .map_err(|e| format!("Failed to read cwd: {}", e))?
                .filter_map(|e| e.ok())
                .find(|e| e.path().extension().map_or(false, |ext| ext == "tgz"))
                .map(|e| e.path())
                .ok_or_else(|| "npm pack did not produce a .tgz file".to_string())?;
            // Extract using tar
            let output = std::process::Command::new("tar")
                .args(["-xzf", dir.to_str().unwrap_or("package.tgz")])
                .current_dir(target)
                .output()
                .map_err(|e| format!("tar extraction failed: {}", e))?;
            if !output.status.success() {
                return Err(format!("tar extraction failed"));
            }
            // Remove the .tgz
            let _ = std::fs::remove_file(&dir);
            Ok(None)
        }
        PluginSource::Url { url, .. } => {
            // Download from URL - expected to be a .zip or .tgz
            let output = std::process::Command::new("curl")
                .args(["-fsSL", "-o", "/tmp/plugin_download.tgz", url])
                .output()
                .map_err(|e| format!("curl failed: {}", e))?;
            if !output.status.success() {
                return Err(format!("Failed to download plugin from {}", url));
            }
            std::fs::create_dir_all(target)
                .map_err(|e| format!("Failed to create target dir: {}", e))?;
            let extract_output = std::process::Command::new("tar")
                .args(["-xzf", "/tmp/plugin_download.tgz"])
                .current_dir(target)
                .output()
                .map_err(|e| format!("Failed to extract: {}", e))?;
            if !extract_output.status.success() {
                // Try zip as fallback
                let zip_output = std::process::Command::new("unzip")
                    .args(["-o", "/tmp/plugin_download.tgz", "-d", target])
                    .output()
                    .map_err(|e| format!("Failed to unzip: {}", e))?;
                if !zip_output.status.success() {
                    return Err("Failed to extract downloaded plugin (tried tar and zip)".to_string());
                }
            }
            let _ = std::fs::remove_file("/tmp/plugin_download.tgz");
            Ok(None)
        }
        PluginSource::Pip { .. } => Err("Python package plugins are not yet supported".to_string()),
        PluginSource::Settings { .. } => {
            // Settings source has no filesystem to install
            Err("Settings plugins cannot be cached".to_string())
        }
    }
}

/// Copy a directory recursively
fn copy_directory(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("Failed to create dir: {}", e))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("Failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_entry = entry.path();
        let dst_entry = dst.join(entry.file_name());
        if src_entry.is_dir() {
            copy_directory(&src_entry, &dst_entry)?;
        } else {
            std::fs::copy(&src_entry, &dst_entry)
                .map_err(|e| format!("Failed to copy {}: {}", src_entry.display(), e))?;
        }
    }
    Ok(())
}

/// Run git clone into target directory
fn run_git_clone(url: &str, target: &str, ref_: &Option<String>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["clone", url, target]);
    if let Some(r) = ref_ {
        cmd.args(["--branch", r]);
    }
    let output = cmd.output().map_err(|e| format!("git clone failed: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git clone failed: {}", stderr));
    }
    Ok(())
}

/// Run git sparse clone into target directory
fn run_git_sparse_clone(url: &str, target: &str, ref_: &Option<String>, subdir: &str) -> Result<(), String> {
    std::process::Command::new("git")
        .args([
            "clone", "--no-checkout", "--filter=blob:none",
            url, target,
        ])
        .output()
        .map_err(|e| format!("git clone failed: {}", e))?;

    if let Some(r) = ref_ {
        std::process::Command::new("git")
            .args(["checkout", r])
            .current_dir(target)
            .output()
            .map_err(|e| format!("git checkout failed: {}", e))?;
    }

    // Use sparse checkout to extract only the subdir
    let normalized_subdir = subdir.strip_prefix("./").unwrap_or(subdir);
    std::process::Command::new("git")
        .args(["sparse-checkout", "init"])
        .current_dir(target)
        .output()
        .map_err(|e| format!("git sparse-checkout init failed: {}", e))?;

    std::process::Command::new("git")
        .args(["sparse-checkout", "set", normalized_subdir])
        .current_dir(target)
        .output()
        .map_err(|e| format!("git sparse-checkout set failed: {}", e))?;

    // Move subdir contents to target root
    let subdir_path = Path::new(target).join(normalized_subdir);
    if subdir_path.exists() {
        for entry in std::fs::read_dir(&subdir_path)
            .map_err(|e| format!("Failed to read subdir: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let dst = Path::new(target).join(entry.file_name());
            std::fs::rename(entry.path(), &dst)
                .map_err(|e| format!("Failed to move file: {}", e))?;
        }
        let _ = std::fs::remove_dir_all(&subdir_path);
    }

    Ok(())
}

/// Get the git HEAD commit SHA from a directory
fn get_git_head_sha(dir: &str) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git rev-parse failed: {}", e))?;
    if !output.status.success() {
        return Err("Failed to get git SHA".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
    source_path: &str,
    plugin_id: &str,
    new_version: &str,
    entry: &PluginMarketplaceEntry,
) -> Result<String, String> {
    use std::path::Path;

    let zip_cache_mode = crate::utils::plugins::zip_cache::is_plugin_zip_cache_enabled();
    let cache_path = get_versioned_cache_path(plugin_id, new_version);
    let zip_path = get_versioned_zip_cache_path(plugin_id, new_version);

    // If cache already exists, return it
    if zip_cache_mode {
        if Path::new(&zip_path).exists() {
            return Ok(zip_path);
        }
    } else if Path::new(&cache_path).exists() {
        match std::fs::read_dir(&cache_path) {
            Ok(entries) => {
                if entries.count() > 0 {
                    return Ok(cache_path);
                }
                // Empty dir - remove it so we can recreate
                let _ = std::fs::remove_dir_all(&cache_path);
            }
            Err(_) => { /* can't read dir, will try to create */ }
        }
    }

    // Seed cache hit — return seed path in place (read-only, no copy)
    if let Some(seed_path) = probe_seed_cache(plugin_id, new_version).await {
        return Ok(seed_path);
    }

    // Create parent directories
    if let Some(parent) = Path::new(&cache_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache parent dir: {}", e))?;
    }

    // Copy source directory to cache
    let src_path = Path::new(source_path);
    if !src_path.exists() {
        return Err(format!(
            "Plugin source directory not found: {}",
            source_path
        ));
    }
    copy_directory(src_path, Path::new(&cache_path))?;

    // Remove .git directory from cache if present
    let git_path = format!("{}/.git", cache_path);
    let _ = std::fs::remove_dir_all(&git_path);

    // Validate that cache has content
    match std::fs::read_dir(&cache_path) {
        Ok(entries) => {
            if entries.count() == 0 {
                return Err(format!(
                    "Failed to copy plugin {} to versioned cache: destination is empty after copy",
                    plugin_id
                ));
            }
        }
        Err(_) => {
            return Err(format!("Failed to read cache directory after copy: {}", cache_path));
        }
    }

    // Zip cache mode: convert directory to ZIP and remove the directory
    if zip_cache_mode {
        // Use zip crate to create archive
        return create_plugin_zip(&cache_path, &zip_path);
    }

    Ok(cache_path)
}

/// Probe seed directories for a populated cache at this plugin version.
async fn probe_seed_cache(plugin_id: &str, version: &str) -> Option<String> {
    let seed_dirs = crate::utils::plugins::plugin_directories::get_plugin_seed_dirs();
    for seed_dir in &seed_dirs {
        let (name, marketplace) = crate::utils::plugins::loader::parse_plugin_identifier(plugin_id);
        let marketplace = marketplace.unwrap_or_else(|| "unknown".to_string());
        let name = name.unwrap_or_else(|| plugin_id.to_string());
        let seed_path = seed_dir
            .join("cache")
            .join(&marketplace)
            .join(&name)
            .join(version);
        match std::fs::read_dir(&seed_path) {
            Ok(entries) => {
                if entries.count() > 0 {
                    return Some(seed_path.to_string_lossy().to_string());
                }
            }
            Err(_) => continue,
        }
    }
    None
}

/// Create a ZIP archive of the plugin directory.
fn create_plugin_zip(dir_path: &str, zip_path: &str) -> Result<String, String> {
    use std::io::Write;

    let dir = Path::new(dir_path);
    if let Some(parent) = Path::new(zip_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create zip parent dir: {}", e))?;
    }

    let file = std::fs::File::create(zip_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;

    let mut encoder = zip::ZipWriter::new(file);

    let mut queue = vec![dir.to_path_buf()];

    while let Some(current) = queue.pop() {
        for entry in std::fs::read_dir(&current).map_err(|e| format!("Failed to read dir {}: {}", current.display(), e))? {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            let stripped = path.strip_prefix(dir).unwrap_or(&path);

            if path.is_dir() {
                queue.push(path);
            } else if path.is_file() {
                let options: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);
                encoder.start_file(stripped.to_string_lossy(), options)
                    .map_err(|e| format!("Failed to add file to zip: {}", e))?;
                let data = std::fs::read(&path)
                    .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;
                encoder.write_all(&data)
                    .map_err(|e| format!("Failed to write file to zip: {}", e))?;
            }
        }
    }

    encoder.finish()
        .map_err(|e| format!("Failed to finish zip: {}", e))?;

    // Remove the directory after successful zip creation
    let _ = std::fs::remove_dir_all(dir_path);

    Ok(zip_path.to_string())
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
    manifest_path: &str,
    name: &str,
    source: &str,
) -> Result<serde_json::Value, String> {
    let content = match tokio::fs::read_to_string(manifest_path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({
                "name": name,
                "description": format!("Plugin from {}", source),
            }));
        }
        Err(e) => return Err(format!("Failed to read manifest: {}", e)),
    };

    let parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Plugin {} has a corrupt manifest file at {}.\n\nJSON parse error: {}", name, manifest_path, e))?;

    // Basic validation: must have a "name" field that is a string
    if parsed.get("name").and_then(|v| v.as_str()).is_none() {
        return Err(format!(
            "Plugin {} has an invalid manifest file at {}.\n\nValidation errors: missing 'name' field",
            name, manifest_path
        ));
    }

    Ok(parsed)
}

/// Calculate plugin version
async fn calculate_plugin_version(
    plugin_id: &str,
    source: &PluginSource,
    manifest: Option<serde_json::Value>,
    source_path: &str,
    entry_version: Option<&str>,
    git_commit_sha: Option<&str>,
) -> Result<String, String> {
    // 1. Use explicit version from plugin.json if available
    if let Some(ref m) = manifest {
        if let Some(version) = m.get("version").and_then(|v| v.as_str()) {
            return Ok(version.to_string());
        }
    }

    // 2. Use provided version (typically from marketplace entry)
    if let Some(v) = entry_version {
        return Ok(v.to_string());
    }

    // 3. Use pre-resolved git SHA
    if let Some(sha) = git_commit_sha {
        let short_sha = &sha[..sha.len().min(12)];
        // For git-subdir sources, encode the subdir path in the version
        if let PluginSource::GitSubdir { subdir, .. } = source {
            let normalized = subdir.replace('\\', "/");
            let norm_path = normalized.strip_prefix("./").unwrap_or(&normalized).trim_end_matches('/').to_string();
            let path_hash = sha256_hash_subdir(&norm_path);
            return Ok(format!("{}-{}", short_sha, path_hash));
        }
        return Ok(short_sha.to_string());
    }

    // 4. Try to get git SHA from source path
    if let Ok(sha) = get_git_head_sha(source_path) {
        let short_sha = &sha[..sha.len().min(12)];
        return Ok(short_sha.to_string());
    }

    // 5. Return 'unknown' as last resort
    Ok("unknown".to_string())
}

/// Hash a subdir path for version calculation (SHA-256, first 8 hex chars).
fn sha256_hash_subdir(path: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(path.as_bytes());
    hex::encode(&hash[..]).chars().take(8).collect()
}

// ============================================================================
// Plugin Policy (stubs)
// ============================================================================

/// Check if a plugin is blocked by org policy
fn is_plugin_blocked_by_policy(plugin_id: &str) -> bool {
    crate::utils::plugins::plugin_policy::is_plugin_blocked_by_policy(plugin_id)
}

// ============================================================================
// Plugin Directories (stubs)
// ============================================================================

/// Delete plugin data directory
async fn delete_plugin_data_dir(plugin_id: &str) -> Result<(), String> {
    crate::utils::plugins::plugin_directories::delete_plugin_data_dir(plugin_id).await;
    Ok(())
}

// ============================================================================
// Plugin Options Storage (stubs)
// ============================================================================

/// Delete plugin options
fn delete_plugin_options(plugin_id: &str) {
    crate::utils::plugins::plugin_options_storage::delete_plugin_options(plugin_id);
}

// ============================================================================
// Plugin Editable Scopes (stubs)
// ============================================================================

/// Get plugin editable scopes - returns set of enabled plugin IDs
fn get_plugin_editable_scopes() -> BTreeSet<String> {
    let mut result = BTreeSet::new();

    // Check all editable settings sources (later overrides earlier)
    let sources = [
        SettingSource::UserSettings,
        SettingSource::ProjectSettings,
        SettingSource::LocalSettings,
    ];

    for source in sources {
        if let Some(settings) = get_settings_for_source(source) {
            if let Some(ep) = settings.enabled_plugins {
                for (id, val) in ep {
                    if val.as_bool() == Some(true) {
                        result.insert(id);
                    } else if val.as_bool() == Some(false) {
                        result.remove(&id);
                    }
                }
            }
        }
    }

    result
}

// ============================================================================
// Dependency Resolution (stubs)
// ============================================================================

/// Find reverse dependents of a plugin
fn find_reverse_dependents(plugin_id: &str, all_plugins: &[LoadedPlugin]) -> Vec<String> {
    // Convert to the type expected by the dependency resolver
    let resolver_plugins: Vec<crate::utils::plugins::dependency_resolver::LoadedPlugin> = all_plugins
        .iter()
        .map(|p| {
            let deps = p.manifest
                .as_ref()
                .and_then(|m| m.get("dependencies"))
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
            crate::utils::plugins::dependency_resolver::LoadedPlugin {
                name: p.name.clone(),
                source: p.source.clone().unwrap_or_default(),
                enabled: true,
                manifest: crate::utils::plugins::dependency_resolver::PluginManifest {
                    dependencies: deps,
                },
            }
        })
        .collect();
    crate::utils::plugins::dependency_resolver::find_reverse_dependents(
        &plugin_id.to_string(),
        &resolver_plugins,
    )
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
    plugin_id: &str,
    entry: &PluginMarketplaceEntry,
    scope: InstallableScope,
    marketplace_install_location: Option<&str>,
) -> InstallResolutionResult {
    // 1. Check if plugin is blocked by policy
    if crate::utils::plugins::plugin_policy::is_plugin_blocked_by_policy(plugin_id) {
        return InstallResolutionResult::BlockedByPolicy {
            plugin_name: entry.name.clone(),
        };
    }

    // 2. Check dependencies are not blocked (manifest not loaded yet,
    // dependency resolution happens during plugin loading)

    // 3. Cache the plugin and register in installed_plugins.json
    let plugin_scope = match scope {
        InstallableScope::User => crate::utils::plugins::schemas::PluginScope::User,
        InstallableScope::Project => crate::utils::plugins::schemas::PluginScope::Project,
        InstallableScope::Local => crate::utils::plugins::schemas::PluginScope::Local,
    };

    let project_path = match scope {
        InstallableScope::Project | InstallableScope::Local => {
            std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())
        }
        _ => None,
    };

    if !crate::utils::plugins::schemas::is_local_plugin_source(&entry.source) {
        // External plugin - cache and register
        if let Err(e) = crate::utils::plugins::plugin_installation_helpers::cache_and_register_plugin(
            plugin_id,
            entry,
            plugin_scope,
            project_path.as_deref(),
            None, // local_source_path
        ).await {
            log::error!("Failed to cache plugin {}: {}", plugin_id, e);
            return InstallResolutionResult::ResolutionFailed {
                resolution: format!("Failed to cache plugin: {}", e),
            };
        }
    } else if let Some(mpl) = marketplace_install_location {
        // Local plugin - register from marketplace location
        let local_rel_path = match &entry.source {
            PluginSource::Relative(p) => p.as_str(),
            PluginSource::Npm { package, .. } => package.as_str(),
            PluginSource::Pip { package, .. } => package.as_str(),
            PluginSource::Github { repo, .. } => repo.as_str(),
            PluginSource::GitSubdir { repo, .. } => repo.as_str(),
            PluginSource::Git { url, .. } => url.as_str(),
            PluginSource::Url { url, .. } => url.as_str(),
            PluginSource::Settings { source } => source.as_str(),
        };
        if let Err(e) = crate::utils::plugins::plugin_installation_helpers::cache_and_register_plugin(
            plugin_id,
            entry,
            plugin_scope,
            project_path.as_deref(),
            Some(&format!("{}/{}", mpl, local_rel_path)),
        ).await {
            log::error!("Failed to register local plugin {}: {}", plugin_id, e);
            return InstallResolutionResult::ResolutionFailed {
                resolution: format!("Failed to register local plugin: {}", e),
            };
        }
    } else {
        return InstallResolutionResult::LocalSourceNoLocation {
            plugin_name: entry.name.clone(),
        };
    }

    // 4. Update settings to enable the plugin
    let setting_source = scope_to_setting_source(scope);
    let mut enabled_plugins = get_settings_for_source(setting_source.clone())
        .and_then(|s| s.enabled_plugins)
        .unwrap_or_default();
    enabled_plugins.insert(plugin_id.to_string(), serde_json::Value::Bool(true));

    if let Err(e) = update_settings_for_source(
        setting_source,
        &SettingsJson {
            enabled_plugins: Some(enabled_plugins),
        },
    ) {
        return InstallResolutionResult::SettingsWriteFailed { message: e };
    }

    InstallResolutionResult::Success { dep_note: String::new() }
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
    let search_order = [
        InstallableScope::Local,
        InstallableScope::Project,
        InstallableScope::User,
    ];

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
    if let Some(local_install) = installations
        .iter()
        .find(|inst| inst.scope == "local" && inst.project_path == current_project_path)
    {
        return (
            local_install.scope.clone(),
            local_install.project_path.clone(),
        );
    }

    if let Some(project_install) = installations
        .iter()
        .find(|inst| inst.scope == "project" && inst.project_path == current_project_path)
    {
        return (
            project_install.scope.clone(),
            project_install.project_path.clone(),
        );
    }

    if let Some(user_install) = installations.iter().find(|inst| inst.scope == "user") {
        return (
            user_install.scope.clone(),
            user_install.project_path.clone(),
        );
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
pub async fn install_plugin_op(plugin: &str, scope: InstallableScope) -> PluginOperationResult {
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
                if let Some(plugin_entry) =
                    marketplace.plugins.iter().find(|p| p.name == plugin_name)
                {
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
        InstallResolutionResult::LocalSourceNoLocation { plugin_name } => PluginOperationResult {
            success: false,
            message: format!(
                "Cannot install local plugin \"{}\" without marketplace install location",
                plugin_name
            ),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
        InstallResolutionResult::SettingsWriteFailed { message } => PluginOperationResult {
            success: false,
            message: format!("Failed to update settings: {}", message),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
        InstallResolutionResult::ResolutionFailed { resolution } => PluginOperationResult {
            success: false,
            message: format_resolution_error(&resolution),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
        InstallResolutionResult::BlockedByPolicy { plugin_name } => PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is blocked by your organization's policy and cannot be installed",
                plugin_name
            ),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: None,
        },
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
    let all_plugins: Vec<LoadedPlugin> = enabled.into_iter().chain(disabled.into_iter()).collect();

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
        insts
            .iter()
            .find(|i| i.scope == scope.to_string() && i.project_path == project_path)
    });

    let scope_installation = match scope_installation {
        Some(inst) => inst,
        None => {
            // Try to find where the plugin is actually installed
            let (actual_scope, _) = get_plugin_installation_from_v2(&plugin_id);
            if actual_scope != scope.to_string() && installations.map_or(false, |i| !i.is_empty()) {
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

    let _ = update_settings_for_source(
        setting_source,
        &SettingsJson {
            enabled_plugins: Some(
                new_enabled_plugins
                    .into_iter()
                    .filter_map(|(k, v)| v.map(|val| (k, val)))
                    .collect(),
            ),
        },
    );

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
        enabled_plugins.insert(plugin.to_string(), serde_json::Value::Bool(enabled));

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
        && find_plugin_in_settings(plugin)
            .as_ref()
            .is_some_and(|found| {
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
        let scope_suffix = scope
            .map(|s| format!(" at {} scope", s))
            .unwrap_or_default();
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
    enabled_plugins.insert(plugin_id.clone(), serde_json::Value::Bool(enabled));

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
pub async fn update_plugin_op(plugin: &str, scope: &str) -> PluginUpdateResult {
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
    let installation = installations
        .iter()
        .find(|inst| inst.scope == scope.to_string() && inst.project_path == project_path);

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

    let (source_path, new_version, should_cleanup_source, git_commit_sha) = match &entry.source {
        PluginSource::Npm { .. }
        | PluginSource::Pip { .. }
        | PluginSource::Github { .. }
        | PluginSource::GitSubdir { .. }
        | PluginSource::Git { .. }
        | PluginSource::Url { .. } => {
            // Remote plugin: download to temp directory first
            match cache_plugin(&entry.source, CachePluginOptions { manifest: None }).await {
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
            let source_path =
                marketplace_dir.join(if let PluginSource::Relative(rel) = &entry.source {
                    rel
                } else {
                    ""
                });

            // Verify source_path exists
            if !source_path.exists() {
                return PluginUpdateResult {
                    success: false,
                    message: format!("Plugin source not found at {}", source_path.display()),
                    plugin_id: Some(plugin_id.to_string()),
                    new_version: None,
                    old_version: old_version.clone(),
                    already_up_to_date: None,
                    scope: Some(scope.to_string()),
                };
            }

            // Try to load manifest from plugin directory
            let plugin_manifest = load_plugin_manifest(
                &source_path
                    .join(".claude-plugin")
                    .join("plugin.json")
                    .to_string_lossy(),
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
    let versioned_path =
        match copy_plugin_to_versioned_cache(&source_path, plugin_id, &new_version, entry).await {
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
    let is_old_version_still_referenced =
        updated_disk_data
            .plugins
            .values()
            .any(|plugin_installations| {
                plugin_installations
                    .iter()
                    .any(|inst| inst.install_path == old_version_path)
            });

    if !is_old_version_still_referenced && !old_version_path.is_empty() {
        mark_plugin_version_orphaned(&old_version_path).await;
    }

    let scope_desc = project_path
        .map(|p| format!("{} ({})", scope, p))
        .unwrap_or_else(|| scope.to_string());
    let message = format!(
        "Plugin \"{}\" updated from {} to {} for scope {}. Restart to apply changes.",
        plugin_name,
        old_version
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
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
