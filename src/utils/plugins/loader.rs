//! Plugin loader - cache-only loading functions
//!
//! Ported from ~/claudecode/openclaudecode/src/utils/plugins/marketplaceManager.ts
//!
//! This module provides cache-only loading functions for marketplaces and plugins.
//! These functions are used for startup paths that should never block on network.

use std::fs;
use std::path::PathBuf;

use super::types::{KnownMarketplacesFile, PluginMarketplace, PluginMarketplaceEntry};
use crate::plugin::types::{LoadedPlugin, PluginError, PluginLoadResult};
use crate::utils::config::get_global_config_path;

/// Get the path to the known marketplaces config file
fn get_known_marketplaces_file() -> PathBuf {
    get_global_config_path().join("known_marketplaces.json")
}

/// Read a cached marketplace from disk
async fn read_cached_marketplace(install_location: &str) -> Option<PluginMarketplace> {
    let marketplace_path = PathBuf::from(install_location)
        .join(".ai-plugin")
        .join("marketplace.json");

    if !marketplace_path.exists() {
        return None;
    }

    match fs::read_to_string(&marketplace_path) {
        Ok(content) => match serde_json::from_str::<PluginMarketplace>(&content) {
            Ok(marketplace) => Some(marketplace),
            Err(e) => {
                eprintln!(
                    "Failed to parse marketplace at {}: {}",
                    marketplace_path.display(),
                    e
                );
                None
            }
        },
        Err(e) => {
            eprintln!(
                "Failed to read marketplace at {}: {}",
                marketplace_path.display(),
                e
            );
            None
        }
    }
}

/// Load known marketplaces config from cache
async fn load_known_marketplaces_config() -> Option<KnownMarketplacesFile> {
    let config_file = get_known_marketplaces_file();

    if !config_file.exists() {
        return None;
    }

    match fs::read_to_string(&config_file) {
        Ok(content) => match serde_json::from_str::<KnownMarketplacesFile>(&content) {
            Ok(config) => Some(config),
            Err(e) => {
                eprintln!("Failed to parse known marketplaces: {}", e);
                None
            }
        },
        Err(e) => {
            eprintln!("Failed to read known marketplaces file: {}", e);
            None
        }
    }
}

/// Parse plugin identifier into name and marketplace
///
/// # Arguments
/// * `plugin_id` - Plugin ID in format "name@marketplace"
///
/// # Returns
/// Tuple of (name, marketplace) or (None, None) if invalid
pub fn parse_plugin_identifier(plugin_id: &str) -> (Option<String>, Option<String>) {
    if let Some(at_pos) = plugin_id.rfind('@') {
        let name = plugin_id[..at_pos].to_string();
        let marketplace = plugin_id[at_pos + 1..].to_string();
        if !name.is_empty() && !marketplace.is_empty() {
            return (Some(name), Some(marketplace));
        }
    }
    (None, None)
}

/// Get a marketplace by name from cache only (no network)
///
/// Use this for startup paths that should never block on network.
///
/// # Arguments
/// * `name` - Marketplace name
///
/// # Returns
/// The marketplace or null if not found/cache missing
pub async fn get_marketplace_cache_only(name: &str) -> Option<PluginMarketplace> {
    let config_file = get_known_marketplaces_file();

    if !config_file.exists() {
        return None;
    }

    match fs::read_to_string(&config_file) {
        Ok(content) => {
            match serde_json::from_str::<KnownMarketplacesFile>(&content) {
                Ok(config) => {
                    if let Some(entry) = config.get(name) {
                        // Try to read the marketplace from the install location
                        if let Some(marketplace) =
                            read_cached_marketplace(&entry.install_location).await
                        {
                            return Some(marketplace);
                        }
                    }
                    None
                }
                Err(e) => {
                    eprintln!("Failed to parse known marketplaces config: {}", e);
                    None
                }
            }
        }
        Err(_) => None,
    }
}

/// Get a plugin by ID from cache only (no network)
///
/// # Arguments
/// * `plugin_id` - Plugin ID in format "name@marketplace"
///
/// # Returns
/// The plugin entry and marketplace install location, or null if not found/cache missing
pub async fn get_plugin_by_id_cache_only(
    plugin_id: &str,
) -> Option<(PluginMarketplaceEntry, String)> {
    let (plugin_name, marketplace_name) = parse_plugin_identifier(plugin_id);
    let plugin_name = plugin_name?;
    let marketplace_name = marketplace_name?;

    let config_file = get_known_marketplaces_file();

    if !config_file.exists() {
        return None;
    }

    match fs::read_to_string(&config_file) {
        Ok(content) => {
            match serde_json::from_str::<KnownMarketplacesFile>(&content) {
                Ok(config) => {
                    // Get marketplace config
                    let marketplace_config = config.get(&marketplace_name)?;

                    // Get the marketplace itself
                    let marketplace = get_marketplace_cache_only(&marketplace_name).await?;

                    // Find the plugin in the marketplace
                    marketplace
                        .plugins
                        .into_iter()
                        .find(|p| p.name == plugin_name)
                        .map(|entry| (entry, marketplace_config.install_location.clone()))
                }
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}

/// Get all known marketplace names
pub async fn get_known_marketplace_names() -> Vec<String> {
    match load_known_marketplaces_config().await {
        Some(config) => config.keys().cloned().collect(),
        None => vec![],
    }
}

/// Load all plugins from cache (no network).
///
/// This is the main entry point for loading plugins at startup.
/// Returns enabled/disabled plugins and any errors encountered.
pub async fn load_all_plugins() -> Result<PluginLoadResult, Box<dyn std::error::Error + Send + Sync>>
{
    use super::installed_plugins_manager::load_installed_plugins_from_disk;
    use crate::plugin::types::{LoadedPlugin, PluginManifest, PluginError};

    let mut enabled = Vec::new();
    let mut disabled = Vec::new();
    let mut errors = Vec::new();

    let installed = match load_installed_plugins_from_disk() {
        Ok(d) => d,
        Err(e) => {
            log::debug!("Failed to load installed plugins: {}", e);
            return Ok(PluginLoadResult {
                enabled,
                disabled,
                errors,
            });
        }
    };

    for (plugin_id, installations) in &installed.plugins {
        for inst in installations {
            // Try to load plugin manifest from install path
            let manifest_path =
                PathBuf::from(&inst.install_path).join(".ai-plugin").join("plugin.json");
            let legacy_manifest_path =
                PathBuf::from(&inst.install_path).join("plugin.json");

            let manifest = if manifest_path.exists() {
                load_plugin_manifest(&manifest_path, plugin_id).await
            } else if legacy_manifest_path.exists() {
                load_plugin_manifest(&legacy_manifest_path, plugin_id).await
            } else {
                // Create minimal manifest from what we know
                Some(PluginManifest {
                    name: plugin_id.clone(),
                    version: inst.version.clone(),
                    description: None,
                    author: None,
                    homepage: None,
                    repository: None,
                    license: None,
                    keywords: None,
                    dependencies: None,
                    commands: None,
                    agents: None,
                    skills: None,
                    hooks: None,
                    output_styles: None,
                    channels: None,
                    mcp_servers: None,
                    lsp_servers: None,
                    settings: None,
                    user_config: None,
                })
            };

            let Some(manifest) = manifest else {
                errors.push(PluginError::ManifestParseError {
                    source: "load_all_plugins".to_string(),
                    plugin: Some(plugin_id.clone()),
                    manifest_path: inst.install_path.clone(),
                    parse_error: "Failed to load manifest".to_string(),
                });
                continue;
            };

            let loaded = LoadedPlugin {
                name: manifest.name.clone(),
                manifest,
                path: inst.install_path.clone(),
                source: plugin_id.clone(),
                repository: plugin_id.clone(),
                enabled: Some(true),
                is_builtin: None,
                sha: inst.git_commit_sha.clone(),
                commands_path: None,
                commands_paths: None,
                commands_metadata: None,
                agents_path: None,
                agents_paths: None,
                skills_path: None,
                skills_paths: None,
                output_styles_path: None,
                output_styles_paths: None,
                hooks_config: None,
                mcp_servers: None,
                lsp_servers: None,
                settings: None,
            };

            // All loaded plugins from installed_plugins.json are considered enabled
            enabled.push(loaded);
        }
    }

    Ok(PluginLoadResult {
        enabled,
        disabled,
        errors,
    })
}

/// Load a plugin manifest from a file path.
async fn load_plugin_manifest(
    path: &std::path::Path,
    plugin_id: &str,
) -> Option<crate::plugin::types::PluginManifest> {
    use crate::plugin::types::PluginManifest;

    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&content).ok().or_else(|| {
        log::debug!(
            "Failed to parse plugin manifest at {}: expected valid PluginManifest JSON",
            path.display()
        );
        None
    })
}

/// Load all plugins from cache only (strictly no network).
///
/// Same as load_all_plugins but guaranteed never to hit the network.
pub async fn load_all_plugins_cache_only()
-> Result<PluginLoadResult, Box<dyn std::error::Error + Send + Sync>> {
    load_all_plugins().await
}

/// Get the plugin cache root directory.
pub fn get_plugin_cache_path() -> String {
    get_global_config_path()
        .join("plugins")
        .to_string_lossy()
        .to_string()
}

/// Get a versioned cache path for a specific plugin version.
pub fn get_versioned_cache_path(plugin_id: &str, version: &str) -> String {
    let (name, marketplace) = parse_plugin_identifier(plugin_id);
    let marketplace = marketplace.unwrap_or_else(|| "unknown".to_string());
    let name = name.unwrap_or_else(|| plugin_id.to_string());
    get_global_config_path()
        .join("plugins")
        .join(&marketplace)
        .join(&name)
        .join(version)
        .to_string_lossy()
        .to_string()
}

/// Get a versioned zip cache path for a specific plugin version.
pub fn get_versioned_zip_cache_path(plugin_id: &str, version: &str) -> String {
    format!("{}.zip", get_versioned_cache_path(plugin_id, version))
}

/// Cache a plugin and return the cached path.
pub async fn cache_plugin(
    source: &super::schemas::PluginSource,
    entry: &PluginMarketplaceEntry,
) -> Result<CachePluginResult, Box<dyn std::error::Error + Send + Sync>> {
    use std::time::SystemTime;

    let cache_path = get_plugin_cache_path();
    std::fs::create_dir_all(&cache_path)?;

    let temp_name = format!(
        "temp_{}_{}",
        plugin_source_prefix(source),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let temp_path = format!("{}/{}", cache_path, temp_name);

    let git_commit_sha = install_plugin_source(source, &temp_path)
        .map_err(|e| format!("Failed to install plugin source: {}", e))?;

    // Read manifest from .ai-plugin/plugin.json or plugin.json
    let manifest_path = format!("{}/.ai-plugin/plugin.json", temp_path);
    let legacy_manifest_path = format!("{}/plugin.json", temp_path);
    let manifest: crate::plugin::types::PluginManifest = if std::path::Path::new(&manifest_path).exists() {
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Invalid manifest JSON: {}", e))?
    } else if std::path::Path::new(&legacy_manifest_path).exists() {
        let content = std::fs::read_to_string(&legacy_manifest_path)
            .map_err(|e| format!("Failed to read manifest: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Invalid manifest JSON: {}", e))?
    } else {
        return Err(format!("No manifest found at {} or {}", manifest_path, legacy_manifest_path).into());
    };

    let final_name = manifest.name.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-");
    let final_path = format!("{}/{}", cache_path, final_name);

    // Remove old cached version if exists
    if std::path::Path::new(&final_path).exists() {
        std::fs::remove_dir_all(&final_path)?;
    }

    std::fs::rename(&temp_path, &final_path)
        .map_err(|e| format!("Failed to move cached plugin: {}", e))?;

    Ok(CachePluginResult {
        path: final_path,
        manifest: serde_json::to_value(&manifest)?,
        git_commit_sha,
    })
}

/// Generate a temp name prefix from a plugin source.
fn plugin_source_prefix(source: &super::schemas::PluginSource) -> &str {
    match source {
        super::schemas::PluginSource::Relative(p) => p.as_str(),
        super::schemas::PluginSource::Npm { package, .. } => package.as_str(),
        super::schemas::PluginSource::Pip { package, .. } => package.as_str(),
        super::schemas::PluginSource::Github { repo, .. } => repo.as_str(),
        super::schemas::PluginSource::GitSubdir { repo, .. } => repo.as_str(),
        super::schemas::PluginSource::Git { url, .. } => url.as_str(),
        super::schemas::PluginSource::Url { url, .. } => url.as_str(),
        super::schemas::PluginSource::Settings { .. } => "settings",
    }
}

/// Install a plugin source into a target directory.
fn install_plugin_source(
    source: &super::schemas::PluginSource,
    target: &str,
) -> Result<Option<String>, String> {
    std::fs::create_dir_all(target).map_err(|e| format!("Failed to create dir {}: {}", target, e))?;
    match source {
        super::schemas::PluginSource::Relative(p) => {
            if !std::path::Path::new(p).exists() {
                return Err(format!("Local plugin path does not exist: {}", p));
            }
            copy_dir(p, target)?;
            Ok(None)
        }
        super::schemas::PluginSource::Git { url, ref_, .. } => {
            run_git_clone(url, target, ref_)?;
            let sha = git_head_sha(target)?;
            Ok(Some(sha))
        }
        super::schemas::PluginSource::Github { repo, ref_, .. } => {
            // Clone GitHub repo
            let clone_url = if repo.starts_with("http") || repo.starts_with("git") {
                repo.clone()
            } else {
                format!("https://github.com/{}.git", repo)
            };
            run_git_clone(&clone_url, target, ref_)?;
            let sha = git_head_sha(target)?;
            Ok(Some(sha))
        }
        super::schemas::PluginSource::GitSubdir { repo, subdir, ref_, .. } => {
            let temp_git = format!("{}_git", target);
            let clone_url = if repo.starts_with("http") || repo.starts_with("git") {
                repo.clone()
            } else {
                format!("https://github.com/{}.git", repo)
            };
            run_git_clone(&clone_url, &temp_git, ref_)?;
            let sha = git_head_sha(&temp_git)?;
            let subdir_path = format!("{}/{}", temp_git, subdir);
            if !std::path::Path::new(&subdir_path).is_dir() {
                return Err(format!("Subdir not found: {}", subdir));
            }
            // Copy subdir contents to target
            for entry in std::fs::read_dir(&subdir_path)
                .map_err(|e| format!("Failed to read subdir: {}", e))?
            {
                let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
                let dest = format!("{}/{}", target, entry.file_name().to_string_lossy());
                if entry.path().is_dir() {
                    copy_dir(
                        entry.path().to_string_lossy().as_ref(),
                        &dest,
                    )?;
                } else {
                    std::fs::copy(entry.path(), &dest)
                        .map_err(|e| format!("Failed to copy: {}", e))?;
                }
            }
            std::fs::remove_dir_all(&temp_git).map_err(|e| format!("Failed to clean up temp dir: {}", e))?;
            Ok(Some(sha))
        }
        super::schemas::PluginSource::Npm { package, version, .. } => {
            let version_str = version.as_deref().unwrap_or("latest");
            let npm_pkg = format!("{}@{}", package, version_str);
            let output = std::process::Command::new("npm")
                .args(["install", "--prefix", target, &npm_pkg, "--save", "--no-package-lock"])
                .output()
                .map_err(|e| format!("npm install failed: {}", e))?;
            if !output.status.success() {
                return Err(format!(
                    "npm install failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(None)
        }
        super::schemas::PluginSource::Pip { .. } => {
            return Err("Python package plugins are not yet supported".into());
        }
        super::schemas::PluginSource::Url { url, .. } => {
            // Download and extract tarball/zip via git clone
            let output = std::process::Command::new("git")
                .args(["clone", url, target])
                .output()
                .map_err(|e| format!("git clone failed: {}", e))?;
            if !output.status.success() {
                return Err(format!(
                    "git clone failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let sha = git_head_sha(target)?;
            Ok(Some(sha))
        }
        super::schemas::PluginSource::Settings { .. } => {
            return Err("Settings plugins cannot be cached directly".into());
        }
    }
}

fn run_git_clone(url: &str, target: &str, ref_: &Option<String>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["clone", url, target]);
    if let Some(r) = ref_ {
        cmd.args(["--branch", r]);
    }
    let output = cmd.output().map_err(|e| format!("git clone failed: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn git_head_sha(dir: &str) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git rev-parse failed: {}", e))?;
    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            return Ok(sha);
        }
    }
    Err("Failed to get git SHA".into())
}

fn copy_dir(src: &str, dst: &str) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("Failed to create dir {}: {}", dst, e))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("Failed to read dir {}: {}", src, e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = std::path::Path::new(dst).join(entry.file_name());
        if src_path.is_dir() {
            copy_dir(
                src_path.to_string_lossy().as_ref(),
                dst_path.to_string_lossy().as_ref(),
            )?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", src_path, dst_path, e))?;
        }
    }
    Ok(())
}

/// Result of caching a plugin.
pub struct CachePluginResult {
    pub path: String,
    pub manifest: serde_json::Value,
    pub git_commit_sha: Option<String>,
}

/// Clear the plugin cache for a specific marketplace, or all if None.
pub fn clear_plugin_cache(_marketplace: Option<&str>) {
    crate::utils::plugins::installed_plugins_manager::clear_installed_plugins_cache();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plugin_identifier_basic() {
        let (name, marketplace) = parse_plugin_identifier("my-plugin@my-marketplace");
        assert_eq!(name, Some("my-plugin".to_string()));
        assert_eq!(marketplace, Some("my-marketplace".to_string()));
    }

    #[test]
    fn test_parse_plugin_identifier_invalid() {
        let (name, marketplace) = parse_plugin_identifier("invalid");
        assert_eq!(name, None);
        assert_eq!(marketplace, None);
    }

    #[test]
    fn test_parse_plugin_identifier_empty_marketplace() {
        let (name, marketplace) = parse_plugin_identifier("my-plugin@");
        assert_eq!(name, None);
        assert_eq!(marketplace, None);
    }
}
